//! Integration tests for `job.result_chunk` streaming (ARCP v1.1 §8.4).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::similar_names
)]

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
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let result_id = "res_test_01";
        let chunks = ["Section 1: hello ", "Section 2: world ", "Section 3: end"];
        let mut total: u64 = 0;
        for (seq, fragment) in chunks.iter().enumerate() {
            let more = seq + 1 < chunks.len();
            total += fragment.len() as u64;
            ctx.emit_result_chunk(
                result_id,
                seq as u64,
                (*fragment).to_owned(),
                ResultChunkEncoding::Utf8,
                more,
            )
            .await?;
        }
        Ok(serde_json::json!({
            STREAMED_RESULT_SENTINEL: {
                "result_id": result_id,
                "result_size": total,
                "summary": format!("report with {} chunks", chunks.len()),
            }
        }))
    }
}

#[tokio::test]
async fn result_chunk_stream_and_completed_with_result_id() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(ReportBuilder))
                .build(),
        )
        .build()
        .await
        .expect("build");

    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "rc-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send");
    let accepted = client_t.recv().await.expect("recv").expect("envelope");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted");
    };
    let session_id = payload.session_id;

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "report-builder",
        serde_json::json!({}),
    )));
    invoke.session_id = Some(session_id.clone());
    client_t.send(invoke).await.expect("send");

    let mut assembler = ResultChunkAssembler::new();
    let mut got_completed = None;

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client_t.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("envelope");
        match env.payload {
            MessageType::JobResultChunk(chunk) => {
                let _ = assembler.push(&chunk).expect("assemble");
            }
            MessageType::JobCompleted(payload) => {
                got_completed = Some(payload);
                break;
            }
            _ => {}
        }
    }

    let completed = got_completed.expect("got job.completed");
    assert_eq!(completed.result_id.as_deref(), Some("res_test_01"));
    assert_eq!(
        completed.result_size,
        Some(
            (["Section 1: hello ", "Section 2: world ", "Section 3: end"])
                .iter()
                .map(|s| s.len() as u64)
                .sum()
        )
    );
    assert!(completed.value.is_none());
    assert!(assembler.is_finished());
    let assembled = assembler.finish_utf8().expect("assemble utf8");
    assert_eq!(
        assembled,
        "Section 1: hello Section 2: world Section 3: end"
    );
}

#[tokio::test]
async fn result_chunk_round_trips_through_serde() {
    let env = Envelope::new(MessageType::JobResultChunk(
        arcp::messages::JobResultChunkPayload {
            result_id: "r1".into(),
            chunk_seq: 5,
            data: "fragment".into(),
            encoding: ResultChunkEncoding::Utf8,
            more: true,
        },
    ));
    let json = serde_json::to_string(&env).expect("serialize");
    assert!(json.contains("\"type\":\"job.result_chunk\""));
    let back: Envelope = serde_json::from_str(&json).expect("deserialize");
    let MessageType::JobResultChunk(p) = back.payload else {
        panic!("expected JobResultChunk");
    };
    assert_eq!(p.chunk_seq, 5);
    assert_eq!(p.encoding, ResultChunkEncoding::Utf8);
}
