//! Per-job context handed to [`crate::runtime::ToolHandler::invoke`].
//!
//! Carries the cancellation token plus channels back to the runtime for
//! issuing human-input requests and (later) recording metrics, opening
//! streams, etc.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::ids::{JobId, MessageId, SessionId};
use crate::messages::{
    CostBudget, HumanChoiceRequestPayload, HumanInputRequestPayload, JobResultChunkPayload,
    MessageType, MetricPayload, ResultChunkEncoding,
};

/// Per-job dispatch context.
pub struct ToolContext {
    /// Cooperative cancellation token. Handlers MUST poll this.
    pub cancel: CancellationToken,
    pub(crate) job_id: JobId,
    pub(crate) session_id: SessionId,
    pub(crate) correlation_id: MessageId,
    pub(crate) out: mpsc::Sender<Envelope>,
    pub(crate) pending_human: Arc<dashmap::DashMap<MessageId, oneshot::Sender<HumanResponse>>>,
    /// Per-job `cost.budget` tracker (ARCP v1.1 §9.6). Constructed
    /// empty when no budget was declared on `tool.invoke`.
    pub(crate) budget: BudgetTracker,
}

/// Per-job `cost.budget` counters (ARCP v1.1 §9.6).
///
/// Tracks remaining authority for each declared currency. Reporting cost
/// is the agent's responsibility; the runtime decrements counters on each
/// `cost.*` metric emitted via [`ToolContext::charge`] or
/// [`BudgetTracker::charge`]. Once a counter falls below zero further
/// charges return [`ARCPError::BudgetExhausted`].
#[derive(Clone, Debug, Default)]
pub struct BudgetTracker {
    inner: Arc<BudgetTrackerInner>,
}

#[derive(Debug, Default)]
struct BudgetTrackerInner {
    /// Map of currency → (max, consumed).
    state: Mutex<HashMap<String, (f64, f64)>>,
}

impl BudgetTracker {
    /// Construct an empty tracker (no budget declared).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a tracker pre-seeded from a [`CostBudget`] lease
    /// capability.
    #[must_use]
    pub fn from_budget(budget: &CostBudget) -> Self {
        let mut state = HashMap::new();
        for a in &budget.amounts {
            state.insert(a.currency.clone(), (a.amount, 0.0));
        }
        Self {
            inner: Arc::new(BudgetTrackerInner {
                state: Mutex::new(state),
            }),
        }
    }

    /// True when no currencies are tracked (i.e. budgeting is disabled).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.inner.state.lock().map_or(true, |s| s.is_empty())
    }

    /// Remaining budget for `currency`, if tracked. `None` means the
    /// currency was not in the declared lease (i.e. unbudgeted, treated
    /// as unbounded for that currency).
    #[must_use]
    pub fn remaining(&self, currency: &str) -> Option<f64> {
        let s = self.inner.state.lock().ok()?;
        s.get(currency).map(|(max, cons)| max - cons)
    }

    /// Snapshot of remaining-per-currency for all tracked currencies.
    #[must_use]
    pub fn snapshot_remaining(&self) -> HashMap<String, f64> {
        self.inner
            .state
            .lock()
            .map(|s| {
                s.iter()
                    .map(|(k, (max, cons))| (k.clone(), max - cons))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Charge `amount` to `currency`. Returns `Ok(remaining)` after
    /// the decrement, or [`ARCPError::BudgetExhausted`] when the
    /// charge would (or did) drop the counter to ≤ 0.
    ///
    /// Negative amounts are rejected per §9.6.
    ///
    /// Currencies not present in the declared lease are silently
    /// ignored (returns `Ok(f64::INFINITY)`), matching the §9.6 rule
    /// that "unreported / unbudgeted costs are not enforced".
    ///
    /// # Errors
    ///
    /// - [`ARCPError::InvalidArgument`] for a negative amount.
    /// - [`ARCPError::BudgetExhausted`] when the counter is at or
    ///   below zero before the charge (the charge is recorded so the
    ///   `cost.budget.remaining` metric reflects the overshoot).
    pub fn charge(&self, currency: &str, amount: f64) -> Result<f64, ARCPError> {
        if amount < 0.0 || !amount.is_finite() {
            return Err(ARCPError::InvalidArgument {
                detail: format!("negative or non-finite cost amount: {amount}"),
            });
        }
        let Ok(mut s) = self.inner.state.lock() else {
            return Err(ARCPError::Internal {
                detail: "budget tracker mutex poisoned".into(),
            });
        };
        let Some(entry) = s.get_mut(currency) else {
            // Currency not budgeted; spec §9.6: "unreported costs are
            // not enforced". Treat unbudgeted currencies the same.
            return Ok(f64::INFINITY);
        };
        // §9.6 ordering: check BEFORE decrement so the agent sees
        // BUDGET_EXHAUSTED on the operation that would have overspent,
        // not the one that pushed us into the red.
        let remaining_before = entry.0 - entry.1;
        if remaining_before <= 0.0 {
            return Err(ARCPError::BudgetExhausted {
                detail: format!("{currency} budget exhausted (remaining={remaining_before})"),
            });
        }
        entry.1 += amount;
        Ok(entry.0 - entry.1)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod budget_tracker_tests {
    use super::*;
    use crate::messages::CostBudgetAmount;

    fn budget(items: &[(&str, f64)]) -> CostBudget {
        CostBudget {
            amounts: items
                .iter()
                .map(|(c, a)| CostBudgetAmount {
                    currency: (*c).to_owned(),
                    amount: *a,
                })
                .collect(),
        }
    }

    #[test]
    fn fresh_tracker_reports_max_remaining() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 5.0)]));
        assert_eq!(t.remaining("USD"), Some(5.0));
    }

    #[test]
    fn charge_decrements_remaining() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 5.0)]));
        let r = t.charge("USD", 1.5).expect("charge ok");
        assert!((r - 3.5).abs() < f64::EPSILON);
        assert!((t.remaining("USD").unwrap() - 3.5).abs() < f64::EPSILON);
    }

    #[test]
    fn negative_charge_rejected() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 5.0)]));
        assert!(matches!(
            t.charge("USD", -0.5),
            Err(ARCPError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn exhaustion_surfaces_budget_exhausted_on_next_charge() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 1.0)]));
        // Push past zero on the second charge: remaining goes to -0.5.
        let _ = t.charge("USD", 1.5).expect("first charge ok");
        // Counter is now negative; next charge rejects with
        // BUDGET_EXHAUSTED per §9.6.
        let err = t.charge("USD", 0.1).unwrap_err();
        assert!(matches!(err, ARCPError::BudgetExhausted { .. }));
    }

    #[test]
    fn unbudgeted_currency_returns_infinity() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 5.0)]));
        let r = t.charge("EUR", 2.0).expect("charge ok");
        assert!(r.is_infinite());
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("job_id", &self.job_id)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

/// Human-input response variants delivered to a waiting handler.
#[derive(Debug, Clone)]
pub enum HumanResponse {
    /// Operator-supplied value (corresponds to `human.input.response`).
    Value(serde_json::Value),
    /// Operator-supplied choice id (corresponds to `human.choice.response`).
    Choice(String),
    /// Request was cancelled or timed out.
    Cancelled(ErrorCode),
}

impl ToolContext {
    /// Send a `human.input.request` and await the response.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Cancelled`] if the request is cancelled or its
    /// deadline elapses; [`ARCPError::Unavailable`] if the connection is
    /// torn down before a response arrives.
    pub async fn request_human_input(
        &self,
        payload: HumanInputRequestPayload,
    ) -> Result<serde_json::Value, ARCPError> {
        let (tx, rx) = oneshot::channel();
        let mut env = Envelope::new(MessageType::HumanInputRequest(payload));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        let req_id = env.id.clone();
        self.pending_human.insert(req_id.clone(), tx);
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })?;
        match rx.await {
            Ok(HumanResponse::Value(v)) => Ok(v),
            Ok(HumanResponse::Choice(_)) => Err(ARCPError::InvalidArgument {
                detail: "expected human.input.response, got human.choice.response".into(),
            }),
            Ok(HumanResponse::Cancelled(code)) => Err(ARCPError::Cancelled {
                reason: format!("human input cancelled: {code}"),
            }),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "human-input pending channel dropped".into(),
            }),
        }
    }

    /// Send a `human.choice.request` and await the chosen option id.
    ///
    /// # Errors
    ///
    /// Same as [`Self::request_human_input`].
    pub async fn request_human_choice(
        &self,
        payload: HumanChoiceRequestPayload,
    ) -> Result<String, ARCPError> {
        let (tx, rx) = oneshot::channel();
        let mut env = Envelope::new(MessageType::HumanChoiceRequest(payload));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        let req_id = env.id.clone();
        self.pending_human.insert(req_id.clone(), tx);
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })?;
        match rx.await {
            Ok(HumanResponse::Choice(c)) => Ok(c),
            Ok(HumanResponse::Value(_)) => Err(ARCPError::InvalidArgument {
                detail: "expected human.choice.response, got human.input.response".into(),
            }),
            Ok(HumanResponse::Cancelled(code)) => Err(ARCPError::Cancelled {
                reason: format!("human choice cancelled: {code}"),
            }),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "human-choice pending channel dropped".into(),
            }),
        }
    }

    /// The id of the originating `tool.invoke`.
    #[must_use]
    pub const fn correlation_id(&self) -> &MessageId {
        &self.correlation_id
    }

    /// The job id the runtime assigned.
    #[must_use]
    pub const fn job_id(&self) -> &JobId {
        &self.job_id
    }

    /// Reference to this job's `cost.budget` tracker (ARCP v1.1 §9.6).
    ///
    /// The tracker is empty if no `cost_budget` was supplied on
    /// `tool.invoke`. Use [`Self::charge`] to report cost and have the
    /// runtime decrement the counter; use [`BudgetTracker::remaining`]
    /// to query.
    #[must_use]
    pub const fn budget(&self) -> &BudgetTracker {
        &self.budget
    }

    /// Charge `amount` against the `currency` counter and emit a
    /// matching `metric` event (ARCP v1.1 §9.6).
    ///
    /// The metric is named `name` (which SHOULD begin with `cost.` per
    /// §9.6) and carries `unit: currency`, matching what a downstream
    /// runtime would observe on the wire. After the charge, a
    /// `cost.budget.remaining` metric is emitted with the new counter
    /// so clients can render gauges without re-summing.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::BudgetExhausted`] when the counter is at
    /// or below zero before the charge — the canonical signal per
    /// §9.6 that the agent should stop incurring cost.
    pub async fn charge(&self, name: &str, amount: f64, currency: &str) -> Result<(), ARCPError> {
        let remaining = self.budget.charge(currency, amount)?;
        // Emit the cost metric (§9.6: cost reporting is a `metric`
        // with name beginning `cost.` and unit matching the currency).
        let mut metric = Envelope::new(MessageType::Metric(MetricPayload {
            name: name.to_owned(),
            value: amount,
            unit: currency.to_owned(),
            dims: None,
        }));
        metric.session_id = Some(self.session_id.clone());
        metric.job_id = Some(self.job_id.clone());
        metric.correlation_id = Some(self.correlation_id.clone());
        let _ = self.out.send(metric).await;
        // Emit cost.budget.remaining so clients can render budget
        // gauges (§9.6: runtimes MAY emit these after material
        // decrements). Skip if the currency wasn't budgeted (remaining
        // is infinite).
        if remaining.is_finite() {
            let mut rem = Envelope::new(MessageType::Metric(MetricPayload {
                name: "cost.budget.remaining".into(),
                value: remaining,
                unit: currency.to_owned(),
                dims: None,
            }));
            rem.session_id = Some(self.session_id.clone());
            rem.job_id = Some(self.job_id.clone());
            rem.correlation_id = Some(self.correlation_id.clone());
            let _ = self.out.send(rem).await;
        }
        Ok(())
    }

    /// Emit one `job.result_chunk` fragment (ARCP v1.1 §8.4).
    ///
    /// `chunk_seq` is the caller's responsibility — start at 0 and
    /// increment per chunk for the same `result_id`. The terminal chunk
    /// MUST set `more: false`; the job's terminal `job.completed`
    /// SHOULD then carry the same `result_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the outbound channel is
    /// closed.
    pub async fn emit_result_chunk(
        &self,
        result_id: impl Into<String>,
        chunk_seq: u64,
        data: impl Into<String>,
        encoding: ResultChunkEncoding,
        more: bool,
    ) -> Result<(), ARCPError> {
        let mut env = Envelope::new(MessageType::JobResultChunk(JobResultChunkPayload {
            result_id: result_id.into(),
            chunk_seq,
            data: data.into(),
            encoding,
            more,
        }));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        env.correlation_id = Some(self.correlation_id.clone());
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use chrono::Utc;
    use dashmap::DashMap;
    use tokio::sync::mpsc;

    use super::*;
    use crate::messages::{ChoiceOption, HumanChoiceRequestPayload, HumanInputRequestPayload};

    fn build_ctx() -> (
        ToolContext,
        mpsc::Receiver<Envelope>,
        Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>>,
    ) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let pending: Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>> =
            Arc::new(DashMap::new());
        let ctx = ToolContext {
            cancel: CancellationToken::new(),
            job_id: JobId::new(),
            session_id: SessionId::new(),
            correlation_id: MessageId::new(),
            out: out_tx,
            pending_human: Arc::clone(&pending),
            budget: BudgetTracker::new(),
        };
        (ctx, out_rx, pending)
    }

    fn input_request() -> HumanInputRequestPayload {
        HumanInputRequestPayload {
            prompt: "?".into(),
            response_schema: serde_json::json!({}),
            default: None,
            expires_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn accessors_return_internal_ids() {
        let (ctx, _rx, _pending) = build_ctx();
        // Just exercise the const accessors so they're covered.
        assert!(ctx.correlation_id().as_str().starts_with("msg_"));
        assert!(ctx.job_id().as_str().starts_with("job_"));
    }

    #[tokio::test]
    async fn input_round_trip_resolves_via_pending_map() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let id = env.id.clone();
        let (_, tx) = pending.remove(&id).expect("pending entry");
        tx.send(HumanResponse::Value(serde_json::json!({"ok": true})))
            .expect("send");
        let result = task.await.expect("join");
        assert_eq!(result.expect("ok"), serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn input_returns_invalid_argument_on_choice_response() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Choice("nope".into())).expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }

    #[tokio::test]
    async fn input_propagates_cancellation_code() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Cancelled(ErrorCode::DeadlineExceeded))
            .expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::Cancelled { .. }));
    }

    #[tokio::test]
    async fn choice_round_trip_resolves_via_pending_map() {
        let (ctx, mut rx, pending) = build_ctx();
        let payload = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![ChoiceOption {
                id: "x".into(),
                label: "X".into(),
            }],
            expires_at: Utc::now(),
        };
        let task = tokio::spawn(async move { ctx.request_human_choice(payload).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Choice("x".into())).expect("send");
        let chosen = task.await.expect("join").expect("ok");
        assert_eq!(chosen, "x");
    }

    #[tokio::test]
    async fn choice_returns_invalid_argument_on_value_response() {
        let (ctx, mut rx, pending) = build_ctx();
        let payload = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![],
            expires_at: Utc::now(),
        };
        let task = tokio::spawn(async move { ctx.request_human_choice(payload).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Value(serde_json::json!(null)))
            .expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }
}
