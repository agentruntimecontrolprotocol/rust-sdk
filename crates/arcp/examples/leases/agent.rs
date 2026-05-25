//! One-shot generator: thought + optional tool_call. Stub for the LLM loop.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

pub struct ToolCall {
    pub argv: Vec<String>,
    pub reason: String,
}

pub struct LlmStep {
    pub thought: String,
    pub tool_call: Option<ToolCall>,
    pub final_: Option<String>,
}

/// Yield successive [`LlmStep`]s for the supplied incident. In production
/// this is an Anthropic / OpenAI / local-model loop that decides the next
/// shell command and explains itself.
pub async fn next_step(_incident: &str, _prior_denial: Option<&str>) -> Option<LlmStep> {
    todo!()
}
