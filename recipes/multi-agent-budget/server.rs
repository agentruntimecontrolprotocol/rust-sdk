//! ARCP v1.1 §9.6 — multi-agent-budget: runtime / server side.
//!
//! A planner agent decomposes a research question via an LLM call, then
//! delegates each sub-question to a `worker` agent.  Every worker is
//! granted a budget slice carved from the planner's own remaining
//! allowance, so the USD:0.50 top-level cap cascades naturally through
//! the delegation tree.
//!
//! Worker agents execute three sequential phases (`gather`, `analyze`,
//! `summarize`), each charging `cost.completion`.  A worker that exhausts
//! its slice fails with `BUDGET_EXHAUSTED` while sibling workers continue.
//!
//! Highlights:
//!   - §9.6  `cost.budget` auto-decrement on `cost.*` metrics
//!   - §10   delegation with `agent.delegate` + `cost.budget` subset
//!   - §13.2 lease-subset enforcement: workers cannot request more than
//!           the planner's remaining budget
//!   - "debit-self-for-each-grant" pattern: the planner charges
//!     `cost.delegate` after each grant so the next pre-check sees the
//!     honest remaining balance
//!
//! Run:
//!     `cargo run --example multi-agent-budget-server`

#![allow(
    clippy::todo,
    clippy::unimplemented,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::unused_async,
    clippy::diverging_sub_expression,
    clippy::no_effect_underscore_binding,
    clippy::let_unit_value,
    clippy::used_underscore_binding,
    clippy::let_underscore_untyped,
    clippy::struct_field_names,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::redundant_pub_crate,
    dead_code,
    unreachable_code,
    unused_assignments,
    unused_mut,
    unused_imports,
    unused_variables
)]

use arcp::error::ARCPError;
use arcp::lease::validate_lease_op;
use arcp::transport::MemoryTransport;
use arcp::{ARCPRuntime, JobContext};
use serde_json::{json, Value};

type Runtime = ARCPRuntime<MemoryTransport>;

/// Budget (USD) granted to workers by depth score.
///
/// Deeper (more specific) sub-questions receive a slightly larger slice
/// because they typically require more LLM calls to answer well.
fn grant_for_depth(depth: u8) -> f64 {
    match depth {
        1 => 0.05,
        2 => 0.10,
        _ => 0.15,
    }
}

/// Decompose the question into 5 sub-questions and run workers in parallel.
///
/// Each sub-question carries a `depth` score (1–3).  The planner:
///  1. Calls the LLM to produce the decomposition (charges `cost.completion`).
///  2. For each sub-question, pre-checks whether the remaining budget fits
///     the intended grant; if not, skips that sub-question.
///  3. Delegates the sub-question to a `worker` job with a `cost.budget`
///     subset, then charges `cost.delegate` to keep the local counter honest.
async fn planner_agent(
    _input: &Value,
    _ctx: &mut JobContext<'_>,
) -> Result<Value, ARCPError> {
    // let openai = openai::Client::new();
    //
    // // 1. Decompose
    // let plan = openai.chat()
    //   .model("gpt-4o-mini")
    //   .message("user", format!(
    //     "Decompose into 5 sub-questions. JSON {{subQuestions:[{{question,depth:1|2|3}}]}}. Q: {}",
    //     _input["question"].as_str().unwrap_or("")
    //   ))
    //   .response_format("json_object")
    //   .send().await?;
    //
    // ctx.metric("cost.completion", 5.0, "USD").await?;
    // let sub_questions: Vec<SubQuestion> = serde_json::from_str(&plan.content)?;
    //
    // // 2 + 3. Delegate
    // for (i, sq) in sub_questions.iter().enumerate() {
    //   let grant = grant_for_depth(sq.depth);
    //   let remaining = ctx.budget("USD");
    //   if remaining < grant { continue; }
    //
    //   ctx.delegate(DelegateRequest {
    //     delegate_id: format!("del_{i}"),
    //     agent: "worker",
    //     input: json!(sq),
    //     lease_request: LeaseRequest {
    //       cost_budget:  vec![format!("USD:{grant:.2}")],
    //       tool_call:    vec!["llm.complete".into()],
    //     },
    //   }).await?;
    //
    //   // debit so the next iteration sees the honest committed amount
    //   ctx.metric("cost.delegate", grant, "USD").await?;
    // }
    todo!()
}

/// Three-phase worker: gather → analyze → summarize.
///
/// Before each LLM call the worker validates that the `llm.complete` tool
/// is still within budget.  `validate_lease_op` throws `BUDGET_EXHAUSTED`
/// when the counter reaches zero; the runtime converts that into a terminal
/// `job.failed` event while sibling workers continue.
async fn worker_agent(
    _input: &Value,
    _ctx: &mut JobContext<'_>,
) -> Result<Value, ARCPError> {
    // let openai = openai::Client::new();
    // let question = _input["question"].as_str().unwrap_or("");
    //
    // for phase in ["gather", "analyze", "summarize"] {
    //   // Throws BUDGET_EXHAUSTED when cost.budget counter ≤ 0
    //   validate_lease_op(_ctx.lease(), "tool.call", "llm.complete")?;
    //
    //   let r = openai.chat()
    //     .model("gpt-4o-mini")
    //     .message("user", format!("{phase}: {question}"))
    //     .send().await?;
    //
    //   _ctx.metric("cost.completion", 5.0, "USD").await?;
    // }
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime: Runtime = todo!(); // transport, identity, auth elided

    // runtime.register_agent("planner", |input, ctx| Box::pin(planner_agent(input, ctx)));
    // runtime.register_agent("worker",  |input, ctx| Box::pin(worker_agent(input, ctx)));
    // runtime.serve("127.0.0.1:7899").await?;

    println!("multi-agent-budget runtime ready");
    Ok(())
}
