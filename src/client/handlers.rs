//! Handler traits invoked by the client during request/response flows.
//!
//! Phase 2 ships the trait shape; Phase 4 wires them through the runtime.

use async_trait::async_trait;

use crate::messages::{HumanChoiceRequestPayload, HumanInputRequestPayload};

/// Application-supplied handler for `human.input.request` /
/// `human.choice.request` (RFC §12).
#[async_trait]
pub trait HumanInputHandler: Send + Sync {
    /// Respond to a `human.input.request`.
    async fn input(&self, req: HumanInputRequestPayload) -> serde_json::Value;

    /// Respond to a `human.choice.request`.
    async fn choice(&self, req: HumanChoiceRequestPayload) -> String;
}

/// Default handler that returns the request's `default` field for input
/// requests and the first option for choice requests.
#[derive(Debug, Default)]
pub struct NoopHumanInputHandler;

#[async_trait]
impl HumanInputHandler for NoopHumanInputHandler {
    async fn input(&self, req: HumanInputRequestPayload) -> serde_json::Value {
        req.default.unwrap_or(serde_json::Value::Null)
    }

    async fn choice(&self, req: HumanChoiceRequestPayload) -> String {
        req.options
            .first()
            .map(|o| o.id.clone())
            .unwrap_or_default()
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

    use super::*;
    use crate::messages::ChoiceOption;

    #[tokio::test]
    async fn input_returns_default_when_present() {
        let h = NoopHumanInputHandler;
        let req = HumanInputRequestPayload {
            prompt: "?".into(),
            response_schema: serde_json::json!({}),
            default: Some(serde_json::json!({"name": "alice"})),
            expires_at: Utc::now(),
        };
        assert_eq!(h.input(req).await, serde_json::json!({"name": "alice"}));
    }

    #[tokio::test]
    async fn input_returns_null_when_no_default() {
        let h = NoopHumanInputHandler;
        let req = HumanInputRequestPayload {
            prompt: "?".into(),
            response_schema: serde_json::json!({}),
            default: None,
            expires_at: Utc::now(),
        };
        assert_eq!(h.input(req).await, serde_json::Value::Null);
    }

    #[tokio::test]
    async fn choice_picks_first_option() {
        let h = NoopHumanInputHandler;
        let req = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![
                ChoiceOption {
                    id: "a".into(),
                    label: "A".into(),
                },
                ChoiceOption {
                    id: "b".into(),
                    label: "B".into(),
                },
            ],
            expires_at: Utc::now(),
        };
        assert_eq!(h.choice(req).await, "a");
    }

    #[tokio::test]
    async fn choice_returns_empty_string_for_empty_options() {
        let h = NoopHumanInputHandler;
        let req = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![],
            expires_at: Utc::now(),
        };
        assert_eq!(h.choice(req).await, "");
    }
}
