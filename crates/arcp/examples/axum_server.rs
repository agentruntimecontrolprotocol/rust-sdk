//! ARCP v1.1 — embedding a runtime in an Axum HTTP server.
//!
//! Mounts an ARCP WebSocket endpoint alongside regular HTTP routes in an
//! Axum application.  This mirrors the Fastify/Express patterns in the
//! TypeScript SDK: one port serves both REST and ARCP traffic.
//!
//! Route layout:
//!   GET  /health       → `{"status":"ok","arcp":"v1.1"}`
//!   GET  /arcp         → WebSocket upgrade → ARCP session
//!
//! The runtime is shared via `Arc` so all WebSocket handlers share the
//! same job queue, lease manager, and event bus.
//!
//! Run with:
//!     `cargo run --example axum_server`
//!
//! Prerequisites in Cargo.toml (illustrative):
//! ```toml
//! axum            = { version = "0.7", features = ["ws"] }
//! tower           = "0.4"
//! tower-http      = { version = "0.5", features = ["trace"] }
//! tokio           = { version = "1", features = ["full"] }
//! ```

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

use std::net::SocketAddr;
use std::sync::Arc;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::{json, Value};

// Illustrative type aliases — swap in real axum + arcp-runtime types.
type SharedRuntime = Arc<()>; // Arc<ARCPRuntime> in production
type Client = ARCPClient<MemoryTransport>;

/// Build the Axum router with the ARCP WebSocket endpoint and a health
/// check route.
///
/// ```
/// use axum::{routing::get, Router};
///
/// fn build_router(runtime: SharedRuntime) -> Router {
///     Router::new()
///         .route("/health", get(health_handler))
///         .route("/arcp",   get(arcp_ws_handler))
///         .layer(tower_http::trace::TraceLayer::new_for_http())
///         .with_state(runtime)
/// }
/// ```
fn build_router(_runtime: SharedRuntime) {
    // Pseudocode shown in doc-comment above.
    todo!()
}

/// `GET /health` handler.
///
/// ```json
/// {"status":"ok","arcp":"v1.1","req_id":"<uuid>"}
/// ```
async fn health_handler() -> Value {
    // axum::Json(json!({
    //   "status": "ok",
    //   "arcp":   "v1.1",
    //   "req_id": uuid::Uuid::new_v4().to_string(),
    // }))
    todo!()
}

/// `GET /arcp` WebSocket upgrade handler.
///
/// Upgrades the HTTP connection, wraps it in `WebSocketTransport`, and
/// hands it off to the runtime's session manager.
///
/// ```rust
/// async fn arcp_ws_handler(
///     ws: WebSocketUpgrade,
///     State(runtime): State<SharedRuntime>,
/// ) -> impl IntoResponse {
///     ws.on_upgrade(move |socket| async move {
///         let transport = WebSocketTransport::from_socket(socket);
///         runtime.accept(transport).await;
///     })
/// }
/// ```
async fn arcp_ws_handler() {
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialise the ARCP runtime with a bearer-token authenticator and
    // register the tool handlers.
    let runtime: SharedRuntime = Arc::new(()); // placeholder

    // In production:
    //   let app = build_router(runtime);
    //   let addr = SocketAddr::from(([127, 0, 0, 1], 7897));
    //   println!("listening on http://{addr}  (ARCP WebSocket: ws://{addr}/arcp)");
    //   axum::Server::bind(&addr).serve(app.into_make_service()).await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], 7897_u16));
    println!("server would listen on http://{addr}");
    println!("ARCP WebSocket endpoint: ws://{addr}/arcp");
    println!("(stub — implement build_router to actually start)");
    Ok(())
}
