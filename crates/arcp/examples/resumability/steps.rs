//! Per-step LLM calls. plan / gather / synthesize / critique / finalize.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use serde_json::Value;

pub async fn run_step(
    _client: &(),
    _job_id: &str,
    _step: &str,
    _inputs: Value,
) -> Result<Value, std::io::Error> {
    todo!()
}
