//! Generator proposes; reviewer holds veto via permission.request.
//!
//! Two clients on two sessions; same wire contract. Idempotency key per
//! (ticket, diff) lets identical patches dedupe at the runtime. RFC §15.4.

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

mod agents;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope, ErrorCode};

use crate::agents::{propose, review, Patch, ReviewVerdict};

type Client = ARCPClient<MemoryTransport>;

const MAX_REVISIONS: u32 = 4;

fn fingerprint(diff: &str) -> String {
    // sha256(diff)[..16] — pinned in the resource path so the lease covers
    // exactly this diff and no other.
    let _ = diff;
    todo!()
}

/// Generator: ask for a `repo.write` lease scoped to this exact diff.
async fn request_apply(
    _client: &Client,
    _ticket_id: &str,
    _patch: &Patch,
) -> Result<String, ARCPError> {
    let _fp = fingerprint(&_patch.diff);
    // reply = client.request(envelope("permission.request",
    //   idempotency_key=f"review:{ticket_id}:{fp}",
    //   payload={permission: "repo.write", resource: f"ticket:{ticket_id}/{fp}",
    //     operation: "apply_patch", reason: "apply patch",
    //     requested_lease_seconds: 90}), timeout=300s)
    // if reply.type == "permission.deny": Err(PermissionDenied{reason})
    todo!()
}

/// Reviewer: grant or typed deny tied to `request.id`.
async fn respond(
    _reviewer: &Client,
    _request: &Envelope,
    _verdict: &ReviewVerdict,
) -> Result<(), ARCPError> {
    // verdict.grant -> envelope("permission.grant", correlation_id=request.id,
    //   payload={permission, resource, operation, lease_seconds: 90})
    // else -> envelope("permission.deny", correlation_id=request.id,
    //   payload={permission, reason, code: "FAILED_PRECONDITION"})
    todo!()
}

async fn reviewer_loop(_reviewer: &Client, _ticket: &str) {
    // for await env in reviewer.events():
    //     if env.type == "permission.request":
    //         verdict = review(ticket, env).await
    //         respond(reviewer, env, verdict).await
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Two sessions, one per agent. In production they're different processes
    // on different runtimes; the message contract is identical.
    let generator: Client = todo!(); // transport, identity, auth elided
    let _reviewer: Client = todo!();

    let ticket_id = "JIRA-4812";
    let ticket = "Reject JWTs whose `aud` does not match the configured audience. Add a unit test.";
    let _rev_task = tokio::spawn(async move { /* reviewer_loop(&reviewer, ticket).await */ });

    let mut prior_denial: Option<String> = None;
    for _ in 0..MAX_REVISIONS {
        let patch = propose(ticket, prior_denial.as_deref()).await;
        match request_apply(&generator, ticket_id, &patch).await {
            Ok(lease) => {
                println!("applied {} lease={lease}", fingerprint(&patch.diff));
                return Ok(());
            }
            Err(ARCPError::PermissionDenied { detail }) => {
                prior_denial = Some(detail);
            }
            Err(other) => return Err(other.into()),
        }
    }
    println!("abandoned after max_revisions");
    Ok(())
}
