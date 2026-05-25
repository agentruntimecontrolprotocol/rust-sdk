//! ARCP v1.1 §8.4 — `job.result_chunk` streamed-result demo.
//!
//! Hosts a `report-builder` agent that emits the final result as a
//! sequence of `job.result_chunk` events, terminated by a `job.completed`
//! that references the streamed `result_id`. The client uses
//! `ResultChunkAssembler` to reassemble the chunks into the original
//! payload.
//!
//! Run with:
//!     `cargo run --example result_chunk`

#![allow(clippy::similar_names, clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, ResultChunkAssembler,
    ResultChunkEncoding, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::server::STREAMED_RESULT_SENTINEL;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct ReportBuilder;

#[async_trait]
impl ToolHandler for ReportBuilder {
    fn name(&self) -> &'static str {
        "report-builder"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let total = arguments
            .get("chunks")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(8);
        let result_id = format!("res_{}", ctx.job_id().as_str());
        let mut bytes: u64 = 0;
        for i in 0..total {
            let more = i + 1 < total;
            let fragment = format!("Section {}: lorem ipsum dolor sit amet\n", i + 1);
            bytes += fragment.len() as u64;
            ctx.emit_result_chunk(&result_id, i, fragment, ResultChunkEncoding::Utf8, more)
                .await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(serde_json::json!({
            STREAMED_RESULT_SENTINEL: {
                "result_id": result_id,
                "result_size": bytes,
                "summary": format!("report with {total} chunks"),
            }
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("demo-token", "demo"),
        ))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(ReportBuilder))
                .build(),
        )
        .build()
        .await?;

    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("demo-token".into()),
        },
        client: ClientIdentity {
            kind: "result-chunk-demo".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await?;
    let accepted = client_t.recv().await?.ok_or("no session.accepted")?;
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        return Err("expected session.accepted".into());
    };
    let session_id = payload.session_id;
    println!("connected; session_id={session_id}");

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "report-builder",
        serde_json::json!({"chunks": 5}),
    )));
    invoke.session_id = Some(session_id);
    client_t.send(invoke).await?;

    let mut assembler = ResultChunkAssembler::new();
    let mut chunks = 0u32;
    loop {
        let env = tokio::time::timeout(Duration::from_secs(2), client_t.recv())
            .await??
            .ok_or("transport closed")?;
        match env.payload {
            MessageType::JobAccepted(p) => println!("job_id={}", p.job_id),
            MessageType::JobResultChunk(chunk) => {
                chunks += 1;
                println!(
                    "result_chunk seq={} more={} len={}B",
                    chunk.chunk_seq,
                    chunk.more,
                    chunk.data.len()
                );
                let _ = assembler.push(&chunk)?;
            }
            MessageType::JobCompleted(p) => {
                println!(
                    "job.completed result_id={:?} result_size={:?} summary={:?}",
                    p.result_id, p.result_size, p.summary
                );
                break;
            }
            _ => {}
        }
    }

    let assembled = assembler.finish_utf8()?;
    println!(
        "assembled {} chunks into {} bytes (head: {:?})",
        chunks,
        assembled.len(),
        &assembled[..assembled.len().min(40)],
    );

    Ok(())
}
