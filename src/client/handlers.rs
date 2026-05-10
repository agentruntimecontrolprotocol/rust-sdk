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
