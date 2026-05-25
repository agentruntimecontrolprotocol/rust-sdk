//! Integration tests for the subscription manager (RFC §13).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::time::Duration;

use arcp::envelope::{Envelope, Priority};
use arcp::ids::SessionId;
use arcp::messages::{MessageType, MetricPayload, PingPayload, SubscriptionFilter};
use arcp::runtime::SubscriptionManager;

fn ping(session: &SessionId) -> Envelope {
    let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
    env.session_id = Some(session.clone());
    env
}

fn metric(session: &SessionId, name: &str) -> Envelope {
    let mut env = Envelope::new(MessageType::Metric(MetricPayload {
        name: name.into(),
        value: 1.0,
        unit: "count".into(),
        dims: None,
    }));
    env.session_id = Some(session.clone());
    env
}

#[tokio::test]
async fn filter_by_type_only_delivers_matching() {
    let mgr = SubscriptionManager::new();
    let s = SessionId::new();
    let filter = SubscriptionFilter {
        types: vec!["metric".into()],
        ..SubscriptionFilter::default()
    };
    let (_id, mut rx) = mgr.register(filter, s.clone());

    let _ = mgr.publish(&ping(&s));
    let _ = mgr.publish(&metric(&s, "tokens.used"));

    let env = tokio::time::timeout(Duration::from_millis(100), rx.next())
        .await
        .expect("timely")
        .expect("envelope");
    assert_eq!(env.payload.type_name(), "metric");
}

#[tokio::test]
async fn min_priority_filter_drops_lower() {
    let mgr = SubscriptionManager::new();
    let s = SessionId::new();
    let filter = SubscriptionFilter {
        min_priority: Some(Priority::High),
        ..SubscriptionFilter::default()
    };
    let (_id, mut rx) = mgr.register(filter, s.clone());

    let mut low = ping(&s);
    low.priority = Priority::Normal;
    let mut high = ping(&s);
    high.priority = Priority::High;
    let _ = mgr.publish(&low);
    let _ = mgr.publish(&high);

    let env = tokio::time::timeout(Duration::from_millis(100), rx.next())
        .await
        .expect("timely")
        .expect("envelope");
    assert_eq!(env.priority, Priority::High);
}

#[tokio::test]
async fn drop_session_terminates_subscriptions_for_that_session() {
    let mgr = SubscriptionManager::new();
    let s = SessionId::new();
    let (_id, _rx) = mgr.register(SubscriptionFilter::default(), s.clone());
    assert_eq!(mgr.len(), 1);
    mgr.drop_session(&s);
    assert!(mgr.is_empty());
}
