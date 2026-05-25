//! Final LLM call that fuses fan-out results into a single answer.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use serde_json::Value;

pub struct Job {
    pub target: String,
    pub job_id: Option<String>,
    pub final_: Option<Value>,
    pub error: Option<Value>,
}

pub fn synthesize(_request: &str, _jobs: &[Job]) -> String {
    todo!()
}
