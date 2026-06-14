//! Subscription manager (ARCP v1.1 §7.6).
//!
//! Every accepted envelope is published into a `tokio::sync::broadcast`
//! channel. Subscriptions filter the live tail by type / `session_id` /
//! `job_id`; backfill replays from the event log. Rich filter
//! authorisation policy lands in a follow-up.

use std::collections::HashSet;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;

use arcp_core::envelope::{Envelope, Priority};
use arcp_core::ids::{JobId, SessionId, StreamId, SubscriptionId, TraceId};
use arcp_core::messages::SubscriptionFilter;

const BROADCAST_CAPACITY: usize = 1024;

/// Map of active subscriptions, keyed by `SubscriptionId`.
#[derive(Clone)]
pub struct SubscriptionManager {
    inner: Arc<Inner>,
}

struct Inner {
    bus: broadcast::Sender<Envelope>,
    subs: DashMap<SubscriptionId, ActiveSubscription>,
}

#[derive(Clone)]
struct ActiveSubscription {
    /// Filter, retained for re-binding scenarios — future phases will allow
    /// querying filters and rebuilding receivers after replays.
    #[allow(dead_code)]
    filter: SubscriptionFilter,
    /// Owning session — used for tear-down on session eviction.
    session_id: SessionId,
}

impl std::fmt::Debug for SubscriptionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionManager")
            .field("active", &self.inner.subs.len())
            .finish()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionManager {
    /// Construct a new manager.
    #[must_use]
    pub fn new() -> Self {
        let (bus, _drop_initial_receiver) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                bus,
                subs: DashMap::new(),
            }),
        }
    }

    /// Publish `envelope` to all subscribers; lossy under backpressure.
    /// Returns the number of subscribers the message was delivered to.
    #[must_use]
    pub fn publish(&self, envelope: &Envelope) -> usize {
        // broadcast::send returns the receiver count even when there are
        // no live receivers (it returns Err in that case); collapse both.
        self.inner.bus.send(envelope.clone()).unwrap_or(0)
    }

    /// Register a new subscription. Returns the new id and a receiver.
    #[must_use]
    pub fn register(
        &self,
        filter: SubscriptionFilter,
        session_id: SessionId,
    ) -> (SubscriptionId, FilteredReceiver) {
        let id = SubscriptionId::new();
        let rx = self.inner.bus.subscribe();
        // Compile the wire filter into a set-backed representation ONCE at
        // registration so the hot fan-out path does O(1)/O(log n) membership
        // checks instead of rescanning Vecs for every envelope (#77).
        let compiled = CompiledFilter::from_wire(&filter);
        self.inner
            .subs
            .insert(id.clone(), ActiveSubscription { filter, session_id });
        (
            id,
            FilteredReceiver {
                inner: rx,
                filter: compiled,
            },
        )
    }

    /// Tear down a subscription. Returns whether it existed.
    #[must_use]
    pub fn unsubscribe(&self, id: &SubscriptionId) -> bool {
        self.inner.subs.remove(id).is_some()
    }

    /// Drop every subscription owned by `session_id` (e.g. on eviction).
    pub fn drop_session(&self, session_id: &SessionId) {
        self.inner.subs.retain(|_, s| s.session_id != *session_id);
    }

    /// Number of active subscriptions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.subs.len()
    }

    /// True if no active subscriptions exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.subs.is_empty()
    }
}

/// Receiver that yields envelopes matching a compiled subscription filter.
pub struct FilteredReceiver {
    inner: broadcast::Receiver<Envelope>,
    filter: CompiledFilter,
}

impl std::fmt::Debug for FilteredReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilteredReceiver").finish_non_exhaustive()
    }
}

impl FilteredReceiver {
    /// Receive the next matching envelope. Skips over envelopes that don't
    /// match the filter and over lagged broadcasts.
    ///
    /// Returns `None` when the underlying channel is closed.
    pub async fn next(&mut self) -> Option<Envelope> {
        loop {
            match self.inner.recv().await {
                Ok(env) => {
                    if self.filter.matches(&env) {
                        return Some(env);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Set-backed compilation of a [`SubscriptionFilter`] for the live
/// fan-out path (#77).
///
/// The wire [`SubscriptionFilter`] stores list-valued fields as `Vec`s,
/// which are immutable after registration. Compiling them into
/// [`HashSet`]s once at registration turns each per-envelope membership
/// test into O(1) instead of an O(list-size) scan repeated for every
/// envelope on every subscription. The wire/API shape of
/// `SubscriptionFilter` is unchanged.
#[derive(Debug, Clone, Default)]
struct CompiledFilter {
    session_id: HashSet<SessionId>,
    trace_id: HashSet<TraceId>,
    job_id: HashSet<JobId>,
    stream_id: HashSet<StreamId>,
    types: HashSet<String>,
    min_priority: Option<Priority>,
}

impl CompiledFilter {
    fn from_wire(filter: &SubscriptionFilter) -> Self {
        Self {
            session_id: filter.session_id.iter().cloned().collect(),
            trace_id: filter.trace_id.iter().cloned().collect(),
            job_id: filter.job_id.iter().cloned().collect(),
            stream_id: filter.stream_id.iter().cloned().collect(),
            types: filter.types.iter().cloned().collect(),
            min_priority: filter.min_priority,
        }
    }

    /// True if `envelope` satisfies the filter (AND across fields, OR
    /// within list-valued fields), using O(1) set membership.
    fn matches(&self, envelope: &Envelope) -> bool {
        if !self.session_id.is_empty()
            && !envelope
                .session_id
                .as_ref()
                .is_some_and(|s| self.session_id.contains(s))
        {
            return false;
        }
        if !self.trace_id.is_empty()
            && !envelope
                .trace_id
                .as_ref()
                .is_some_and(|t| self.trace_id.contains(t))
        {
            return false;
        }
        if !self.job_id.is_empty()
            && !envelope
                .job_id
                .as_ref()
                .is_some_and(|j| self.job_id.contains(j))
        {
            return false;
        }
        if !self.stream_id.is_empty()
            && !envelope
                .stream_id
                .as_ref()
                .is_some_and(|s| self.stream_id.contains(s))
        {
            return false;
        }
        if !self.types.is_empty() && !self.types.contains(envelope.payload.type_name()) {
            return false;
        }
        if let Some(min) = self.min_priority {
            if priority_rank(envelope.priority) < priority_rank(min) {
                return false;
            }
        }
        true
    }
}

/// True if `envelope` satisfies the filter (AND across fields, OR within
/// list-valued fields).
#[must_use]
pub fn matches(filter: &SubscriptionFilter, envelope: &Envelope) -> bool {
    if !filter.session_id.is_empty() {
        let Some(s) = envelope.session_id.as_ref() else {
            return false;
        };
        if !filter.session_id.contains(s) {
            return false;
        }
    }
    if !filter.trace_id.is_empty() {
        let Some(t) = envelope.trace_id.as_ref() else {
            return false;
        };
        if !filter.trace_id.contains(t) {
            return false;
        }
    }
    if !filter.job_id.is_empty() {
        let Some(j) = envelope.job_id.as_ref() else {
            return false;
        };
        if !filter.job_id.contains(j) {
            return false;
        }
    }
    if !filter.stream_id.is_empty() {
        let Some(s) = envelope.stream_id.as_ref() else {
            return false;
        };
        if !filter.stream_id.contains(s) {
            return false;
        }
    }
    if !filter.types.is_empty() {
        let t = envelope.payload.type_name();
        if !filter.types.iter().any(|filt| filt == t) {
            return false;
        }
    }
    if let Some(min) = filter.min_priority {
        if priority_rank(envelope.priority) < priority_rank(min) {
            return false;
        }
    }
    true
}

const fn priority_rank(p: arcp_core::envelope::Priority) -> u8 {
    match p {
        arcp_core::envelope::Priority::Low => 0,
        arcp_core::envelope::Priority::Normal => 1,
        arcp_core::envelope::Priority::High => 2,
        arcp_core::envelope::Priority::Critical => 3,
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
    use super::*;
    use arcp_core::envelope::Envelope;
    use arcp_core::ids::SessionId;
    use arcp_core::messages::{MessageType, PingPayload};

    fn ping_for(session: &SessionId) -> Envelope {
        let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
        env.session_id = Some(session.clone());
        env
    }

    #[tokio::test]
    async fn subscription_filters_by_session_id() {
        let mgr = SubscriptionManager::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();
        let filter = SubscriptionFilter {
            session_id: vec![s1.clone()],
            ..SubscriptionFilter::default()
        };
        let (_id, mut rx) = mgr.register(filter, s1.clone());

        let _ = mgr.publish(&ping_for(&s2)); // should be filtered out
        let _ = mgr.publish(&ping_for(&s1)); // should pass

        let env = tokio::time::timeout(std::time::Duration::from_millis(100), rx.next())
            .await
            .expect("timely")
            .expect("envelope");
        assert_eq!(env.session_id.as_ref(), Some(&s1));
    }

    #[tokio::test]
    async fn unsubscribe_removes_entry() {
        let mgr = SubscriptionManager::new();
        let s = SessionId::new();
        let (id, _rx) = mgr.register(SubscriptionFilter::default(), s);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.unsubscribe(&id));
        assert!(mgr.is_empty());
    }

    #[tokio::test]
    async fn unsubscribe_returns_false_for_unknown_id() {
        let mgr = SubscriptionManager::new();
        let id = SubscriptionId::new();
        assert!(!mgr.unsubscribe(&id));
    }

    #[tokio::test]
    async fn drop_session_keeps_other_sessions() {
        let mgr = SubscriptionManager::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();
        let (_id1, _rx1) = mgr.register(SubscriptionFilter::default(), s1.clone());
        let (_id2, _rx2) = mgr.register(SubscriptionFilter::default(), s2);
        assert_eq!(mgr.len(), 2);
        mgr.drop_session(&s1);
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn matches_handles_every_field_combination() {
        let session = SessionId::new();
        let trace = arcp_core::ids::TraceId::new("t").expect("non-empty");
        let job = arcp_core::ids::JobId::new();
        let stream = arcp_core::ids::StreamId::new();

        let mut env = ping_for(&session);
        env.trace_id = Some(trace.clone());
        env.job_id = Some(job.clone());
        env.stream_id = Some(stream.clone());

        let filter = SubscriptionFilter {
            session_id: vec![session.clone()],
            trace_id: vec![trace],
            job_id: vec![job],
            stream_id: vec![stream],
            types: vec!["ping".into()],
            min_priority: Some(arcp_core::envelope::Priority::Low),
        };
        assert!(matches(&filter, &env));

        // No session id on envelope but filter requires one => no match.
        let mut bare = Envelope::new(MessageType::Ping(PingPayload::default()));
        bare.session_id = None;
        let session_only = SubscriptionFilter {
            session_id: vec![session],
            ..SubscriptionFilter::default()
        };
        assert!(!matches(&session_only, &bare));
    }

    #[test]
    fn debug_renders() {
        let mgr = SubscriptionManager::new();
        let _ = format!("{mgr:?}");
        let s = SessionId::new();
        let (_id, rx) = mgr.register(SubscriptionFilter::default(), s);
        let _ = format!("{rx:?}");
    }

    #[tokio::test]
    async fn compiled_filter_with_large_lists_matches_correctly() {
        // Regression for #77: a subscription with large id lists receiving
        // many envelopes must still match by set membership. We build a
        // 1000-entry session filter, then publish a burst and confirm only
        // the targeted session is delivered.
        let mgr = SubscriptionManager::new();
        let target = SessionId::new();
        let mut sessions: Vec<SessionId> = (0..1000).map(|_| SessionId::new()).collect();
        sessions.push(target.clone());
        let filter = SubscriptionFilter {
            session_id: sessions,
            ..SubscriptionFilter::default()
        };
        let (_id, mut rx) = mgr.register(filter, target.clone());

        // Publish 50 envelopes for non-matching sessions and one for the
        // target.
        for _ in 0..50 {
            let _ = mgr.publish(&ping_for(&SessionId::new()));
        }
        let _ = mgr.publish(&ping_for(&target));

        let env = tokio::time::timeout(std::time::Duration::from_millis(200), rx.next())
            .await
            .expect("timely")
            .expect("envelope");
        assert_eq!(env.session_id.as_ref(), Some(&target));
    }

    #[tokio::test]
    async fn closed_bus_makes_receiver_yield_none() {
        let mgr = SubscriptionManager::new();
        let s = SessionId::new();
        let (_id, mut rx) = mgr.register(SubscriptionFilter::default(), s);
        drop(mgr); // drop sender side
        assert!(rx.next().await.is_none());
    }
}
