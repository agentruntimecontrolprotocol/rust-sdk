//! Supervisor + worker pool. Heartbeat loss reroutes via idempotency_key.
//!
//! Same `idempotency_key` on every re-dispatch (RFC §6.4): a worker that
//! survived the network blip dedupes; it doesn't re-execute. Reaper
//! enforces N=2 missed heartbeats per RFC §10.3.

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

mod work;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::work::do_work;

type Client = ARCPClient<MemoryTransport>;

const HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
const DEADLINE_S: u64 = HEARTBEAT_INTERVAL_SECONDS * 2; // RFC §10.3 default N=2

#[derive(Clone)]
struct Worker {
    worker_id: String,
    role: String,
    last_heartbeat: DateTime<Utc>,
    in_flight_job: Option<String>,
}

#[derive(Clone)]
struct Task {
    task_id: String,
    role: String,
    payload: Value,
    idempotency_key: String,
}

#[derive(Default)]
struct Roster {
    workers: HashMap<String, Worker>,
    by_role: HashMap<String, Vec<String>>,
}

impl Roster {
    fn add(&mut self, w: Worker) {
        self.by_role
            .entry(w.role.clone())
            .or_default()
            .push(w.worker_id.clone());
        self.workers.insert(w.worker_id.clone(), w);
    }

    fn candidates(&self, role: &str) -> Vec<&Worker> {
        self.by_role
            .get(role)
            .into_iter()
            .flatten()
            .filter_map(|id| self.workers.get(id))
            .filter(|w| w.in_flight_job.is_none())
            .collect()
    }
}

// Supervisor side ------------------------------------------------------

async fn dispatch(
    _client: &Client,
    task: Task,
    roster: &mut Roster,
    jobs_to_tasks: &mut HashMap<String, Task>,
) -> Result<(), ARCPError> {
    let candidates = roster.candidates(&task.role);
    let chosen = candidates
        .into_iter()
        .min_by_key(|w| w.last_heartbeat)
        .map(|w| w.worker_id.clone())
        .ok_or_else(|| ARCPError::Unavailable {
            detail: format!("no idle workers for role={}", task.role),
        })?;
    // accepted = client.request(envelope("agent.delegate",
    //   idempotency_key=task.idempotency_key,
    //   payload={target: chosen, task: task.task_id,
    //     context: {task_payload: task.payload}}), timeout=10s)
    let job_id: String = todo!(); // accepted.payload["job_id"]
    if let Some(w) = roster.workers.get_mut(&chosen) {
        w.in_flight_job = Some(job_id.clone());
    }
    jobs_to_tasks.insert(job_id, task);
    Ok(())
}

/// Drain inbound + reap stale workers.
async fn supervise(
    _client: Arc<Client>,
    _roster: Arc<Mutex<Roster>>,
    _jobs: Arc<Mutex<HashMap<String, Task>>>,
) {
    // reaper task: every HEARTBEAT_INTERVAL_SECONDS, anyone older than
    // DEADLINE_S is reaped; their in-flight task is re-dispatched.
    // events loop: job.heartbeat updates last_heartbeat;
    //   job.{completed,failed,cancelled} clears in_flight_job.
    todo!()
}

// Worker side ----------------------------------------------------------

async fn heartbeat_loop(
    _client: &Client,
    _job_id: &str,
    _stop: tokio::sync::watch::Receiver<bool>,
) {
    // every HEARTBEAT_INTERVAL_SECONDS: send envelope("job.heartbeat",
    //   job_id, payload={sequence, deadline_ms: HEARTBEAT_INTERVAL_SECONDS*2000,
    //   state: "running"})
    todo!()
}

async fn execute(_client: &Client, _request: Envelope) -> Result<(), ARCPError> {
    let job_id = "job_<uuid>";
    // send job.accepted (correlation_id=request.id), job.started
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let _hb = tokio::spawn({
        let rx = rx.clone();
        async move {
            // heartbeat_loop(client, job_id, rx).await
            drop(rx);
        }
    });
    let payload: Value = json!({});
    match do_work(payload).await {
        Ok(_result) => {
            // send job.completed
        }
        Err(_exc) => {
            // send job.failed {code: "INTERNAL", message, retryable: true}
        }
    }
    Ok(())
}

async fn run_worker(_client: &Client) {
    // for await env in client.events():
    //   if env.type == "agent.delegate": spawn execute(client, env)
    //   if env.type == "session.evicted": return
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let supervisor: Arc<Client> = Arc::new(todo!()); // identity (privileged), auth elided
    let roster = Arc::new(Mutex::new(Roster::default()));
    let jobs = Arc::new(Mutex::new(HashMap::<String, Task>::new()));

    // In production each worker is its own process; co-hosted here.
    for role in ["indexer", "extractor", "archiver"] {
        for _ in 0..2 {
            let w: Client = todo!();
            tokio::spawn(async move { run_worker(&w).await });
            roster.lock().await.add(Worker {
                worker_id: format!("{role}-<rand>"),
                role: role.into(),
                last_heartbeat: Utc::now(),
                in_flight_job: None,
            });
        }
    }

    tokio::spawn(supervise(
        Arc::clone(&supervisor),
        Arc::clone(&roster),
        Arc::clone(&jobs),
    ));

    for n in 0..6 {
        let role = ["indexer", "extractor", "archiver"][n % 3].to_string();
        dispatch(
            &supervisor,
            Task {
                task_id: format!("t{n:03}"),
                role,
                payload: json!({"shard": n}),
                idempotency_key: format!("openclaw:t{n:03}"),
            },
            &mut *roster.lock().await,
            &mut *jobs.lock().await,
        )
        .await?;
    }

    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(())
}
