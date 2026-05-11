//! Warehouse DB admin agent. Reads pre-granted; writes prompt operator.
//!
//! Per-table leases. A mid-flight `lease.revoked` invalidates the local
//! cache so the next call re-prompts. RFC §15.5.

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

mod sql;

use std::collections::HashMap;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope, ErrorCode};
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use crate::sql::classify;

type Client = ARCPClient<MemoryTransport>;
type LeaseCache = HashMap<(String, String), (String, DateTime<Utc>)>;

const PRE_GRANTED: &[&str] = &[
    "public.orders",
    "public.customers",
    "warehouse.fct_revenue_daily",
];
const READ_LEASE_SECONDS: u32 = 60 * 60;
const WRITE_LEASE_SECONDS: u32 = 5 * 60;

async fn request_lease(
    _client: &Client,
    _permission: &str,
    _table: &str,
    _operation: &str,
    _seconds: u32,
    _reason: &str,
) -> Result<(String, DateTime<Utc>), ARCPError> {
    // reply = client.request(envelope("permission.request",
    //   payload={permission, resource: f"table:{table}",
    //   operation, reason, requested_lease_seconds: seconds}), timeout=180s)
    // (lease_id, expires_at) from grant payload.
    todo!()
}

async fn authorize(
    client: &Client,
    sql: &str,
    leases: &mut LeaseCache,
) -> Result<String, ARCPError> {
    let klass = classify(sql);
    if klass.tables.is_empty() {
        return Err(ARCPError::InvalidArgument {
            detail: "no table referenced".into(),
        });
    }
    let seconds = if klass.op == "read" {
        READ_LEASE_SECONDS
    } else {
        WRITE_LEASE_SECONDS
    };
    for table in &klass.tables {
        let key = (table.clone(), klass.op.to_string());
        if let Some((_, expires)) = leases.get(&key) {
            if *expires > Utc::now() {
                continue;
            }
        }
        let lease = request_lease(
            client,
            &format!("db.{}", klass.op),
            table,
            klass.op,
            seconds,
            &format!(
                "{} on {table}: {}",
                klass.op.to_uppercase(),
                &sql.chars().take(80).collect::<String>()
            ),
        )
        .await?;
        leases.insert(key, lease);
    }
    Ok(klass.op.to_string())
}

/// Wire `lease.revoked` into the cache so the next call re-prompts.
fn handle_inbound(env: &Envelope, leases: &mut LeaseCache) {
    // if env.type == "lease.revoked":
    //     lid = env.payload["lease_id"]
    //     leases.retain(|_, (id, _)| id != lid)
    let _ = (env, leases);
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!();
    let mut leases: LeaseCache = HashMap::new();

    // Drain task: forwards every inbound envelope through `handle_inbound`.
    let (_tx, mut rx) = mpsc::unbounded_channel::<Envelope>();
    let drain = tokio::spawn(async move {
        while let Some(_env) = rx.recv().await {
            // handle_inbound(&_env, &mut leases) — would need Arc<Mutex<_>>.
        }
    });

    // Pre-grant the broad reads at session open.
    for table in PRE_GRANTED {
        let lease = request_lease(
            &client,
            "db.read",
            table,
            "read",
            READ_LEASE_SECONDS,
            "bootstrap",
        )
        .await?;
        leases.insert(((*table).to_string(), "read".into()), lease);
    }

    // SELECT — covered by the bootstrap lease.
    authorize(
        &client,
        "SELECT count(*) FROM public.orders WHERE shipped_at::date = current_date - 1",
        &mut leases,
    )
    .await?;
    // UPDATE — triggers permission.request; operator must approve.
    authorize(
        &client,
        "UPDATE public.orders SET status='refunded' WHERE id=4812",
        &mut leases,
    )
    .await?;

    drain.abort();
    Ok(())
}
