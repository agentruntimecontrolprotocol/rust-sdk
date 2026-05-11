//! What the worker actually does. CrewAI Crew, langchain agent, anything.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use serde_json::Value;

pub async fn do_work(_payload: Value) -> Result<Value, std::io::Error> {
    todo!()
}
