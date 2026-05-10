//! Subscription manager (RFC §13).
//!
//! Phase 5 ships a lightweight implementation: every accepted envelope is
//! published into a `tokio::sync::broadcast` channel. Subscriptions filter
//! the live tail by type / `session_id` / `job_id`; backfill replays from
//! the event log. Rich filter authorisation (PLAN.md §A4.10) is reserved
//! for a follow-up phase.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::envelope::Envelope;
use crate::ids::{SessionId, SubscriptionId};
use crate::messages::SubscriptionFilter;

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
        self.inner.subs.insert(
            id.clone(),
            ActiveSubscription {
                filter: filter.clone(),
                session_id,
            },
        );
        (id, FilteredReceiver { inner: rx, filter })
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

/// Receiver that yields envelopes matching a [`SubscriptionFilter`].
pub struct FilteredReceiver {
    inner: broadcast::Receiver<Envelope>,
    filter: SubscriptionFilter,
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
                    if matches(&self.filter, &env) {
                        return Some(env);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
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

const fn priority_rank(p: crate::envelope::Priority) -> u8 {
    match p {
        crate::envelope::Priority::Low => 0,
        crate::envelope::Priority::Normal => 1,
        crate::envelope::Priority::High => 2,
        crate::envelope::Priority::Critical => 3,
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
    use crate::envelope::Envelope;
    use crate::ids::SessionId;
    use crate::messages::{MessageType, PingPayload};

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
}
