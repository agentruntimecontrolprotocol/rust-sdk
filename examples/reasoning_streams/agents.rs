//! Two LLM stubs: primary streamer + critique generator.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use serde_json::Value;

pub async fn primary_step(_request: &str, _last_critique: Option<&Value>) -> String {
    todo!()
}

pub struct Critique {
    pub severity: String,
    pub summary: String,
    pub suggestion: String,
    pub consumed_tokens: u64,
}

pub async fn critique_thought(_content: &str) -> Critique {
    todo!()
}
