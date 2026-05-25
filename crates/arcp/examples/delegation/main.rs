//! Fan a request out to peer runtimes; tolerate partial failure.
//!
//! `JobMux` is the load-bearing pattern: a single reader on
//! `client.events()` fans envelopes out to per-job channels. Without it,
//! parallel `for await env in client.events()` loops starve each other —
//! only one wins per await. RFC §14.

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

mod synth;

use std::collections::HashMap;
use std::sync::Arc;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use tokio::sync::{mpsc, Mutex};

use crate::synth::{synthesize, Job};

type Client = ARCPClient<MemoryTransport>;

const PEERS: &[&str] = &["research.web", "research.code", "research.docs"];
const TERMINAL: &[&str] = &["job.completed", "job.failed", "job.cancelled"];

async fn delegate(_client: &Client, target: &str, _task: &str, _trace_id: &str) -> Job {
    // accepted = client.request(envelope("agent.delegate", trace_id,
    //   payload={target, task, context: {trace_id}}), timeout=10s)
    // if accepted.type != "job.accepted": Job{error: ...} else Job{job_id}
    let _ = (target, _task, _trace_id);
    todo!()
}

/// Single reader on `client.events()`; fans out by `job_id` over
/// per-job [`mpsc`] channels. `None` means terminal.
struct JobMux {
    queues: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Option<Envelope>>>>>,
}

impl JobMux {
    fn new() -> Self {
        Self {
            queues: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn start(&self, _client: Arc<Client>) {
        let queues = Arc::clone(&self.queues);
        tokio::spawn(async move {
            // for await env in _client.events():
            //   if let Some(jid) = env.job_id:
            //     if let Some(tx) = queues.lock().await.get(jid): tx.send(Some(env))
            //     if env.type in TERMINAL: tx.send(None)
            let _ = queues;
        });
    }

    async fn register(&self, job_id: String) -> mpsc::UnboundedReceiver<Option<Envelope>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.queues.lock().await.insert(job_id, tx);
        rx
    }
}

async fn collect(mux: &JobMux, mut job: Job) -> Job {
    let Some(jid) = job.job_id.clone() else {
        return job;
    };
    let mut rx = mux.register(jid).await;
    while let Some(Some(_env)) = rx.recv().await {
        // match env.payload:
        //   JobCompleted -> job.final_ = ...
        //   JobFailed    -> job.error = {code, message}
        //   JobCancelled -> job.error = {code: "CANCELLED"}
    }
    job
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Arc<Client> = Arc::new(todo!()); // transport, identity, auth elided
    let mux = JobMux::new();
    mux.start(Arc::clone(&client));

    let request = "what changed in our auth stack in the last 30 days?";
    let trace_id = "trace_<uuid>";

    let mut jobs = Vec::new();
    for peer in PEERS {
        let job = delegate(&client, peer, request, trace_id).await;
        jobs.push(job);
    }

    let mut completed = Vec::with_capacity(jobs.len());
    for job in jobs {
        completed.push(collect(&mux, job).await);
    }
    println!("{}", synthesize(request, &completed));
    Ok(())
}
