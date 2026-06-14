//! Per-job context handed to [`crate::runtime::ToolHandler::invoke`].
//!
//! Carries the cancellation token plus channels back to the runtime for
//! recording metrics, opening streams, etc.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use arcp_core::envelope::Envelope;
use arcp_core::error::ARCPError;
use arcp_core::ids::{JobId, MessageId, SessionId};
use arcp_core::messages::{
    CostBudget, JobResultChunkPayload, LeaseRequest, MessageType, MetricPayload,
    ResultChunkEncoding,
};

/// Per-job dispatch context.
pub struct ToolContext {
    /// Cooperative cancellation token. Handlers MUST poll this.
    pub cancel: CancellationToken,
    pub(crate) job_id: JobId,
    pub(crate) session_id: SessionId,
    pub(crate) correlation_id: MessageId,
    pub(crate) out: mpsc::Sender<Envelope>,
    /// Per-job `cost.budget` tracker (ARCP v1.1 §9.6). Constructed
    /// empty when no budget was declared on `tool.invoke`.
    pub(crate) budget: BudgetTracker,
    /// Accepted lease request for this job.
    pub(crate) lease: Option<LeaseRequest>,
    /// Per-job result-stream state (ARCP v1.1 §8.4). Shared with the
    /// runtime task so it can enforce chunk ordering and the
    /// stream-then-inline ban at job completion.
    pub(crate) result_stream: Arc<Mutex<ResultStreamState>>,
}

/// Maximum size of a single `job.result_chunk` `data` field (ARCP v1.1
/// §14 recommends a 1 MB cap; exceeding it is an `INTERNAL_ERROR`).
pub(crate) const MAX_RESULT_CHUNK_BYTES: usize = 1024 * 1024;

/// Maximum total size of an assembled streamed result.
pub(crate) const MAX_RESULT_TOTAL_BYTES: usize = 256 * 1024 * 1024;

/// Per-job runtime state for a `job.result_chunk` stream (ARCP v1.1 §8.4).
///
/// The runtime — not the handler — owns chunk ordering and the terminal
/// `result_id` invariant. A handler emits chunks via
/// [`ToolContext::emit_result_chunk`]; this state validates that the
/// first chunk is `chunk_seq == 0`, subsequent chunks are strictly
/// monotonic for a single `result_id`, no chunk follows the terminal
/// (`more: false`) chunk, and per/total size caps hold.
#[derive(Debug, Default)]
pub(crate) struct ResultStreamState {
    active: Option<ActiveResultStream>,
}

#[derive(Debug)]
struct ActiveResultStream {
    result_id: String,
    next_seq: u64,
    finished: bool,
    total_bytes: usize,
}

impl ResultStreamState {
    /// The `result_id` of the stream that has been started, if any.
    pub(crate) fn active_result_id(&self) -> Option<&str> {
        self.active.as_ref().map(|s| s.result_id.as_str())
    }

    /// True once the terminal (`more: false`) chunk has been emitted.
    pub(crate) fn is_finished(&self) -> bool {
        self.active.as_ref().is_some_and(|s| s.finished)
    }

    /// Validate and record one chunk. Returns the protocol error on any
    /// §8.4 violation.
    fn record_chunk(
        &mut self,
        result_id: &str,
        chunk_seq: u64,
        data_len: usize,
        more: bool,
    ) -> Result<(), ARCPError> {
        if result_id.is_empty() {
            return Err(ARCPError::InvalidRequest {
                detail: "job.result_chunk result_id MUST be non-empty (§8.4)".into(),
            });
        }
        if data_len > MAX_RESULT_CHUNK_BYTES {
            return Err(ARCPError::Internal {
                detail: format!(
                    "job.result_chunk exceeds per-chunk size cap ({data_len} > \
                     {MAX_RESULT_CHUNK_BYTES} bytes, §14)"
                ),
            });
        }
        match self.active.as_mut() {
            None => {
                if chunk_seq != 0 {
                    return Err(ARCPError::FailedPrecondition {
                        detail: format!(
                            "first job.result_chunk MUST have chunk_seq 0, got {chunk_seq} (§8.4)"
                        ),
                    });
                }
                self.active = Some(ActiveResultStream {
                    result_id: result_id.to_owned(),
                    next_seq: 1,
                    finished: !more,
                    total_bytes: data_len,
                });
                Ok(())
            }
            Some(stream) => {
                if stream.finished {
                    return Err(ARCPError::FailedPrecondition {
                        detail: "job.result_chunk emitted after the terminal (more:false) chunk \
                                 (§8.4)"
                            .into(),
                    });
                }
                if result_id != stream.result_id {
                    return Err(ARCPError::FailedPrecondition {
                        detail: format!(
                            "job.result_chunk result_id changed mid-stream (expected {}, got \
                             {result_id}) (§8.4)",
                            stream.result_id
                        ),
                    });
                }
                if chunk_seq != stream.next_seq {
                    return Err(ARCPError::OutOfRange {
                        detail: format!(
                            "job.result_chunk out of order: expected chunk_seq {}, got \
                             {chunk_seq} (§8.4)",
                            stream.next_seq
                        ),
                    });
                }
                let new_total = stream.total_bytes.saturating_add(data_len);
                if new_total > MAX_RESULT_TOTAL_BYTES {
                    return Err(ARCPError::Internal {
                        detail: format!(
                            "streamed result exceeds total size cap ({new_total} > \
                             {MAX_RESULT_TOTAL_BYTES} bytes, §14)"
                        ),
                    });
                }
                stream.total_bytes = new_total;
                stream.next_seq += 1;
                stream.finished = !more;
                Ok(())
            }
        }
    }
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

/// Fixed-point scale for internal budget accounting. 1.0 currency unit
/// is `BUDGET_SCALE` ticks (microunits), giving 6 decimal places of
/// precision. This is more than enough for any real-world money or
/// credit currency and avoids the rounding artifacts of binary f64
/// comparisons at exhaustion boundaries.
const BUDGET_SCALE: i128 = 1_000_000;

#[derive(Debug, Default)]
struct BudgetTrackerInner {
    /// Per-currency `(max, consumed)` in fixed-point microunits
    /// (`BUDGET_SCALE` ticks per currency unit) so equality comparisons
    /// at exhaustion boundaries are exact.
    state: Mutex<HashMap<String, (i128, i128)>>,
}

/// Convert a wire-level f64 amount to fixed-point microunits.
///
/// Returns `None` for non-finite, negative, or out-of-range inputs.
fn to_micros(amount: f64) -> Option<i128> {
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let max_amount = (i128::MAX / BUDGET_SCALE) as f64;
    if amount > max_amount {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let scaled = (amount * BUDGET_SCALE as f64).round() as i128;
    Some(scaled)
}

#[allow(clippy::cast_precision_loss)]
fn from_micros(micros: i128) -> f64 {
    micros as f64 / BUDGET_SCALE as f64
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
            let max = to_micros(a.amount).unwrap_or(0);
            state.insert(a.currency.clone(), (max, 0i128));
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
        s.get(currency).map(|(max, cons)| from_micros(max - cons))
    }

    /// Snapshot of remaining-per-currency for all tracked currencies.
    #[must_use]
    pub fn snapshot_remaining(&self) -> HashMap<String, f64> {
        self.inner
            .state
            .lock()
            .map(|s| {
                s.iter()
                    .map(|(k, (max, cons))| (k.clone(), from_micros(max - cons)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Charge `amount` to `currency`. Returns `Ok(remaining)` after
    /// the decrement, or [`ARCPError::BudgetExhausted`] when the
    /// charge would push the counter below zero.
    ///
    /// Negative amounts are rejected per §9.6.
    ///
    /// Currencies not present in the declared lease are silently
    /// ignored (returns `Ok(f64::INFINITY)`), matching the §9.6 rule
    /// that "unreported / unbudgeted costs are not enforced".
    ///
    /// # Examples
    ///
    /// ```
    /// use arcp_core::messages::{CostBudget, CostBudgetAmount};
    /// use arcp_runtime::runtime::context::BudgetTracker;
    ///
    /// let tracker = BudgetTracker::from_budget(&CostBudget {
    ///     amounts: vec![CostBudgetAmount { currency: "USD".into(), amount: 1.00 }],
    /// });
    /// assert!(tracker.charge("USD", 0.30).is_ok());
    /// assert!(tracker.charge("USD", 5.00).is_err()); // overspend rejected
    /// assert!((tracker.remaining("USD").unwrap() - 0.70).abs() < 1e-9);
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ARCPError::InvalidArgument`] for a negative or non-finite
    ///   amount.
    /// - [`ARCPError::BudgetExhausted`] when the charge would overspend
    ///   the remaining budget. The charge is rejected and the counter
    ///   is left unchanged so the agent sees the canonical signal on
    ///   the first operation that would have overspent.
    pub fn charge(&self, currency: &str, amount: f64) -> Result<f64, ARCPError> {
        let Some(amount_micros) = to_micros(amount) else {
            return Err(ARCPError::InvalidArgument {
                detail: format!("negative, non-finite, or out-of-range cost amount: {amount}"),
            });
        };
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
        let remaining = entry.0.saturating_sub(entry.1);
        if amount_micros > remaining {
            return Err(ARCPError::BudgetExhausted {
                detail: format!(
                    "{currency} budget exhausted (remaining={}, attempted={amount})",
                    from_micros(remaining)
                ),
            });
        }
        entry.1 = entry.1.saturating_add(amount_micros);
        Ok(from_micros(entry.0 - entry.1))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod budget_tracker_tests {
    use super::*;
    use arcp_core::messages::CostBudgetAmount;

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
    fn oversized_single_charge_is_rejected_and_counter_unchanged() {
        // §9.6: the charge that would overspend MUST fail. The counter
        // stays unmoved so subsequent in-budget charges still succeed.
        let t = BudgetTracker::from_budget(&budget(&[("USD", 1.0)]));
        let err = t.charge("USD", 1.5).unwrap_err();
        assert!(matches!(err, ARCPError::BudgetExhausted { .. }));
        let remaining = t.remaining("USD").expect("currency tracked");
        assert!((remaining - 1.0).abs() < f64::EPSILON);
        // A subsequent in-budget charge still works.
        let after = t.charge("USD", 0.4).expect("in-budget charge ok");
        assert!((after - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn exact_exhaustion_succeeds_and_next_charge_fails() {
        // Spending exactly the remaining budget must succeed; spending
        // any amount after that must fail with BUDGET_EXHAUSTED.
        let t = BudgetTracker::from_budget(&budget(&[("USD", 1.0)]));
        let after = t.charge("USD", 1.0).expect("exact-exhaustion ok");
        assert!(after.abs() < f64::EPSILON);
        let err = t.charge("USD", 0.000_001).unwrap_err();
        assert!(matches!(err, ARCPError::BudgetExhausted { .. }));
    }

    #[test]
    fn fractional_decimal_charges_sum_without_floating_point_drift() {
        // 0.10 + 0.20 = 0.30 — would be off-by-an-ulp in raw f64 math
        // and could refuse a 0.70 follow-up against a 1.00 budget. The
        // fixed-point accounting must not exhibit that drift.
        let t = BudgetTracker::from_budget(&budget(&[("USD", 1.0)]));
        t.charge("USD", 0.10).expect("first slice");
        t.charge("USD", 0.20).expect("second slice");
        let after = t.charge("USD", 0.70).expect("third slice ok");
        assert!(after.abs() < f64::EPSILON);
    }

    #[test]
    fn multi_currency_charges_are_tracked_independently() {
        let t = BudgetTracker::from_budget(&budget(&[("USD", 5.0), ("EUR", 2.0)]));
        t.charge("USD", 3.0).expect("usd in budget");
        t.charge("EUR", 1.5).expect("eur in budget");
        let usd_err = t.charge("USD", 2.5).unwrap_err();
        assert!(matches!(usd_err, ARCPError::BudgetExhausted { .. }));
        assert!((t.remaining("USD").unwrap() - 2.0).abs() < f64::EPSILON);
        assert!((t.remaining("EUR").unwrap() - 0.5).abs() < f64::EPSILON);
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

impl ToolContext {
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

    /// Accepted lease request for this job, if one was supplied.
    #[must_use]
    pub const fn lease(&self) -> Option<&LeaseRequest> {
        self.lease.as_ref()
    }

    /// Enforce this job's `model.use` lease capability.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::PermissionDenied`] when the lease declares
    /// `model.use` and `model` matches none of the allowed patterns.
    pub fn enforce_model_use(&self, model: &str) -> Result<(), ARCPError> {
        let Some(model_use) = self
            .lease
            .as_ref()
            .and_then(|lease| lease.model_use.as_ref())
        else {
            return Ok(());
        };
        if model_use.matches(model) {
            Ok(())
        } else {
            Err(ARCPError::PermissionDenied {
                detail: format!("model {model} not permitted by lease model.use"),
            })
        }
    }

    /// Translate an upstream credential budget signal into ARCP's canonical
    /// `BUDGET_EXHAUSTED` error.
    #[must_use]
    pub fn translate_upstream_budget_exhausted(&self, detail: impl Into<String>) -> ARCPError {
        ARCPError::BudgetExhausted {
            detail: detail.into(),
        }
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
    /// The runtime — not the handler — enforces the §8.4 invariants:
    /// the first chunk MUST be `chunk_seq == 0`, subsequent chunks MUST be
    /// strictly monotonic for a single `result_id`, no chunk may follow
    /// the terminal (`more: false`) chunk, and per-chunk / total size caps
    /// apply. The job's terminal `job.completed` MUST then carry the same
    /// `result_id` (enforced by the runtime at completion).
    ///
    /// # Errors
    ///
    /// - [`ARCPError::FailedPrecondition`] for a non-zero first chunk, a
    ///   changed `result_id` mid-stream, or a chunk after the terminal one.
    /// - [`ARCPError::OutOfRange`] for a non-monotonic `chunk_seq`.
    /// - [`ARCPError::InvalidRequest`] for an empty `result_id`.
    /// - [`ARCPError::Internal`] when a size cap is exceeded.
    /// - [`ARCPError::Unavailable`] if the outbound channel is closed.
    pub async fn emit_result_chunk(
        &self,
        result_id: impl Into<String>,
        chunk_seq: u64,
        data: impl Into<String>,
        encoding: ResultChunkEncoding,
        more: bool,
    ) -> Result<(), ARCPError> {
        let result_id = result_id.into();
        let data = data.into();
        // Validate and record ordering/size BEFORE emitting, so a rejected
        // chunk never reaches the wire.
        {
            let mut guard = self.result_stream.lock().map_err(|_| ARCPError::Internal {
                detail: "result stream mutex poisoned".into(),
            })?;
            guard.record_chunk(&result_id, chunk_seq, data.len(), more)?;
        }
        let mut env = Envelope::new(MessageType::JobResultChunk(JobResultChunkPayload {
            result_id,
            chunk_seq,
            data,
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
    use tokio::sync::mpsc;

    use super::*;

    fn build_ctx() -> (ToolContext, mpsc::Receiver<Envelope>) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let ctx = ToolContext {
            cancel: CancellationToken::new(),
            job_id: JobId::new(),
            session_id: SessionId::new(),
            correlation_id: MessageId::new(),
            out: out_tx,
            budget: BudgetTracker::new(),
            lease: None,
            result_stream: Arc::new(Mutex::new(ResultStreamState::default())),
        };
        (ctx, out_rx)
    }

    #[tokio::test]
    async fn accessors_return_internal_ids() {
        let (ctx, _rx) = build_ctx();
        // Just exercise the const accessors so they're covered.
        assert!(ctx.correlation_id().as_str().starts_with("msg_"));
        assert!(ctx.job_id().as_str().starts_with("job_"));
    }

    #[tokio::test]
    async fn ordered_chunks_are_accepted_and_finalize() {
        let (ctx, mut rx) = build_ctx();
        ctx.emit_result_chunk("res_1", 0, "a", ResultChunkEncoding::Utf8, true)
            .await
            .expect("chunk 0");
        ctx.emit_result_chunk("res_1", 1, "b", ResultChunkEncoding::Utf8, false)
            .await
            .expect("chunk 1 terminal");
        // Two envelopes were emitted.
        assert!(rx.recv().await.is_some());
        assert!(rx.recv().await.is_some());
        let (active_id, finished) = {
            let guard = ctx.result_stream.lock().unwrap();
            (
                guard.active_result_id().map(str::to_owned),
                guard.is_finished(),
            )
        };
        assert_eq!(active_id.as_deref(), Some("res_1"));
        assert!(finished);
    }

    #[tokio::test]
    async fn first_chunk_must_be_seq_zero() {
        let (ctx, _rx) = build_ctx();
        let err = ctx
            .emit_result_chunk("res_1", 1, "a", ResultChunkEncoding::Utf8, true)
            .await
            .expect_err("non-zero first chunk rejected");
        assert!(matches!(err, ARCPError::FailedPrecondition { .. }));
    }

    #[tokio::test]
    async fn out_of_order_chunk_is_rejected() {
        let (ctx, _rx) = build_ctx();
        ctx.emit_result_chunk("res_1", 0, "a", ResultChunkEncoding::Utf8, true)
            .await
            .expect("chunk 0");
        let err = ctx
            .emit_result_chunk("res_1", 2, "c", ResultChunkEncoding::Utf8, false)
            .await
            .expect_err("gap rejected");
        assert!(matches!(err, ARCPError::OutOfRange { .. }));
    }

    #[tokio::test]
    async fn chunk_after_terminal_is_rejected() {
        let (ctx, _rx) = build_ctx();
        ctx.emit_result_chunk("res_1", 0, "a", ResultChunkEncoding::Utf8, false)
            .await
            .expect("terminal chunk");
        let err = ctx
            .emit_result_chunk("res_1", 1, "b", ResultChunkEncoding::Utf8, false)
            .await
            .expect_err("post-terminal rejected");
        assert!(matches!(err, ARCPError::FailedPrecondition { .. }));
    }

    #[tokio::test]
    async fn result_id_change_mid_stream_is_rejected() {
        let (ctx, _rx) = build_ctx();
        ctx.emit_result_chunk("res_1", 0, "a", ResultChunkEncoding::Utf8, true)
            .await
            .expect("chunk 0");
        let err = ctx
            .emit_result_chunk("res_2", 1, "b", ResultChunkEncoding::Utf8, false)
            .await
            .expect_err("result_id change rejected");
        assert!(matches!(err, ARCPError::FailedPrecondition { .. }));
    }
}
