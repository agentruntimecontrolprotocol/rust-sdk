# Phase 5 — Host Adapters

The TS reference ships six middleware packages
(`typescript-sdk/packages/middleware/{node,express,fastify,hono,bun,otel}`).
Five of those are thin re-exports of one Node-side WS upgrade helper
(`@arcp/node`'s `attachArcpUpgrade`, `typescript-sdk/packages/middleware/node/src/index.ts:36`);
only `bun` and `otel` have non-trivial code paths. The Rust set therefore
collapses to **four crates**: one tower-based universal adapter, one bare
hyper adapter for the no-framework case, one client-side WS shim, and one
OTel `Transport` decorator. Two server-side `Transport` impls live in
adapter crates because each is bound to its host's WS stream type.

## §1. Required adapter crates

| Crate                    | Pulls in                                                                                        | `Transport` impl exposed                                              | WS upgrade attach point                                  |
| ------------------------ | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------- |
| `arcp-axum`              | `axum = "0.7"`, `tower`, `tower-http`, `tokio-tungstenite` (for the upgraded socket's frame codec via `axum::extract::ws`) | `AxumWsTransport` — wraps `axum::extract::ws::WebSocket`              | `WebSocketUpgrade::on_upgrade(|ws| async move { ... })`  |
| `arcp-hyper`             | `hyper = "1"`, `hyper-util`, `tokio-tungstenite`, `http`, `http-body-util`                      | `HyperWsTransport` — wraps `tokio_tungstenite::WebSocketStream<TokioIo<Upgraded>>` | `hyper::upgrade::on(req).await` → `WebSocketStream::from_raw_socket` |
| `arcp-tokio-tungstenite` | `tokio-tungstenite = "0.24"`, `tokio`, `url`                                                    | `TungsteniteTransport` — wraps `tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>` | (client-only) `tokio_tungstenite::connect_async(url)` returns the stream |
| `arcp-otel`              | `opentelemetry = "0.24"`, `opentelemetry-semantic-conventions`, `tracing`, `tracing-opentelemetry` | (decorator) `TracedTransport<T: Transport>` — pass-through wrapper, mints spans, propagates W3C trace context | n/a — composes onto any other `Transport`                |

The four cover every host the TS set covers because every Rust HTTP host
worth shipping either runs on `hyper` directly or accepts a
`tower::Service`. `arcp-axum` is the universal adapter; `arcp-hyper` is
the fallback for users who refuse the tower dependency; the other two are
not host adapters but ship alongside.

### 1.1 `arcp-axum`

Signature of the attach point — the user writes a handler that takes
`WebSocketUpgrade` and the SDK does the rest:

```rust
async fn arcp_handler(
    ws: WebSocketUpgrade,
    State(rt): State<Arc<ArcpRuntime>>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        let transport = AxumWsTransport::new(socket);
        rt.serve(transport).await;
    })
}
```

`WebSocketUpgrade::on_upgrade` signature:
`fn on_upgrade<F, Fut>(self, callback: F) -> Response where F: FnOnce(WebSocket) -> Fut + Send + 'static, Fut: Future<Output = ()> + Send + 'static`
(`axum::extract::ws`). The adapter provides one helper —
`arcp_axum::router(rt)` — returning an `axum::Router` with `GET /arcp`
already wired and the layers from §3 attached.

### 1.2 `arcp-hyper`

For the no-framework case. The user owns the `hyper::server::conn::http1::Builder`
serve loop and dispatches per-request:

```rust
async fn dispatch(
    mut req: Request<Incoming>,
    rt: Arc<ArcpRuntime>,
) -> Result<Response<Empty<Bytes>>, hyper::Error> {
    if is_websocket_upgrade(&req) && req.uri().path() == "/arcp" {
        let (response, fut) = arcp_hyper::upgrade(&mut req)?; // wraps hyper::upgrade::on
        tokio::spawn(async move {
            let ws = fut.await?;
            rt.serve(HyperWsTransport::new(ws)).await;
        });
        Ok(response)
    } else {
        Ok(Response::builder().status(404).body(Empty::new()).unwrap())
    }
}
```

`hyper::upgrade::on(&mut req)` signature:
`fn on<T: HasUpgraded>(msg: T) -> OnUpgrade` returning `OnUpgrade: Future<Output = Result<Upgraded, hyper::Error>>`.
The adapter handles the `Sec-WebSocket-Accept` handshake response, RFC 6455
key derivation, and frame codec via `tokio-tungstenite`'s
`WebSocketStream::from_raw_socket(TokioIo<Upgraded>, Role::Server, None)`.

### 1.3 `arcp-tokio-tungstenite`

Client-side only. The TS set has no equivalent because the TS client uses
the browser `WebSocket` API or `ws` directly; in Rust, the connect dance
is enough boilerplate to warrant a crate. Exposes one function:

```rust
pub async fn connect(
    url: &str,
    headers: HeaderMap,
) -> Result<TungsteniteTransport, ConnectError>;
```

Internally calls `tokio_tungstenite::connect_async_with_config` and wraps
the returned `WebSocketStream` in the SDK's `Transport`. Nothing host-side
lives here — this is purely a `arcp-client` convenience.

### 1.4 `arcp-otel`

Detail in §4.

## §2. Defensible adds — accept / reject

| Candidate         | Decision | One-line reason                                                                                                              |
| ----------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `arcp-actix-web`  | Reject   | `actix-web` 4.x runs on its own actor system, but `actix-web::web::Payload` plus `actix-ws::handle` produce a compatible `Stream + Sink` that the user can hand to `HyperWsTransport`'s constructor; the actix-specific code is ~30 lines of glue best lived in an `examples/actix.rs`, not its own crate. |
| `arcp-warp`       | Reject   | Detailed in §6.                                                                                                              |
| `arcp-poem`       | Reject   | `poem` 3.x is a tower-compatible router; `poem::web::websocket::WebSocket::on_upgrade(|ws| ...)` returns `Pin<Box<dyn Future>>` and yields `poem::web::websocket::WebSocketStream` which implements `Stream<Item = Result<Message, Error>> + Sink<Message>` — identical shape to axum's. Users instantiate `HyperWsTransport` directly or paste the 20-line glue. |
| `arcp-rocket`     | Reject   | Rocket 0.5's WS support is via the `rocket_ws` external crate, still beta; it doesn't expose a stable `tower::Service` and traffic is low. Carry an `examples/rocket.rs` if requested. |
| `arcp-tide`       | Reject   | `tide` 0.16 is unmaintained (last release 2021); `tide-websockets` is unmaintained too. Out.                                 |

The pattern: every Rust HTTP host's WS upgrade hands back a
`Stream<Item = Result<Message, _>> + Sink<Message>`. A single adapter
(`HyperWsTransport`) parameterized over that pair covers all of them. Per
the audit, the existing `src/transport/websocket.rs` already targets
`tokio_tungstenite::WebSocketStream`; the same code generalizes via a
`AsyncRead + AsyncWrite + Unpin` bound on the inner socket.

## §3. Adapter responsibilities (per accepted adapter)

### 3.1 `arcp-axum`

- **WS upgrade attach point.** `axum::extract::ws::WebSocketUpgrade::on_upgrade`. Signature above.
- **`tower::Layer` wrapping.** Adapter ships `arcp_axum::layers()` returning a `ServiceBuilder` pre-stacked with:
  - `tower_http::trace::TraceLayer::new_for_http()` — request/response spans named `arcp.http`.
  - `tower_http::set_header::SetResponseHeaderLayer` for `Strict-Transport-Security` (default `max-age=63072000`).
  - **Host-allowlist layer** (custom — `arcp_axum::HostAllowlistLayer`) per §3.3.
  - **No** `CompressionLayer` — WS frames already carry per-message compression in the WS layer (RFC 7692 / `permessage-deflate`); HTTP compression on the upgrade response is a footgun.
  - **No** `tower::limit::RateLimitLayer` — rate-limit is per-deployment; the user composes it.
- **Host-header / DNS-rebind protection.** Per §3.3 — enforced by default.

### 3.2 `arcp-hyper`

- **WS upgrade attach point.** `hyper::upgrade::on(&mut req)` returning `OnUpgrade: Future<Output = Result<Upgraded, hyper::Error>>`. Adapter wraps this so the user does not write the `Sec-WebSocket-Accept` SHA-1 handshake.
- **`tower::Layer` wrapping.** Not applicable — `hyper` 1.x exposes raw connection handling. The adapter exposes a free function `arcp_hyper::handshake(&mut req) -> (Response, OnUpgrade)` plus a `HostGuard::check(&req, &allowlist)` helper. Composition is the user's job.
- **Host-header / DNS-rebind protection.** Per §3.3 — adapter refuses to mint a `Response` for the upgrade unless `HostGuard::check` returns `Ok`. Default policy below.

### 3.3 Host-allowlist defaults

Spec §14 ("Host-header / DNS-rebind protection" is implied by the security
posture for the WS upgrade) plus the TS contract at
`typescript-sdk/packages/middleware/node/src/index.ts:81-91` (function
`hostHeaderAllowed`). The TS adapter treats `allowedHosts === undefined`
as "allow all" — a documented opt-in.

The Rust adapters MUST invert that: **deny-all unless listed**, with one
escape hatch.

| Configuration                                              | Behaviour                                                       |
| ---------------------------------------------------------- | --------------------------------------------------------------- |
| Default (`HostAllowlist::default()`)                       | Accept only `localhost`, `127.0.0.1`, `[::1]`; reject all else with HTTP 403. |
| `HostAllowlist::strict([..hosts])`                         | Accept listed hosts; reject all else with HTTP 403.             |
| `HostAllowlist::permissive_for_dev()`                      | Accept any `Host`. Tagged `#[doc(hidden = false)]` with a `tracing::warn!` on every accepted upgrade. Intended only for ephemeral dev environments. |
| `HostAllowlist::trusting_proxy_hosts([..forwarded_hosts])` | For deployments behind a reverse proxy that rewrites `Host`. Validates `X-Forwarded-Host` against the list; the upstream proxy is responsible for the raw `Host`. |

Port is stripped before comparison (matching the TS `raw.split(":", 1)[0]`
at `node/src/index.ts:89`).

The deny-all default is the inversion of the TS posture and is the right
call because the Rust SDK is server-side-only at first ship and DNS
rebinding is the live threat for any developer running `cargo run` with
the upgrade attached.

## §4. `arcp-otel` adapter

A `Transport` decorator, not a host adapter. Mirrors the TS contract at
`typescript-sdk/packages/middleware/otel/src/index.ts:withTracing`.

Public API:

```rust
pub fn with_tracing<T: Transport>(inner: T, tracer: opentelemetry::trace::Tracer) -> TracedTransport<T>;
```

`TracedTransport<T>` implements `Transport` (per `arcp-core::transport::Transport`)
and forwards `send` / `recv` / `close` to the wrapped transport, instrumenting
each call.

### 4.1 Trace context on connect

The host adapter passes the HTTP upgrade request's headers to the runtime
when minting a session. `arcp-otel` exposes
`extract_session_span(headers: &http::HeaderMap, tracer: &Tracer) -> Span`:

1. Read `traceparent` and `tracestate` headers per W3C Trace Context.
2. If present, build an `opentelemetry::Context` and use it as parent.
3. Mint a span named `arcp.session` (kind = `SERVER`) with attributes
   from §4.3.
4. Attach via `tracing::Span::current()` so the runtime's per-session
   task inherits the context across `await` points.

The W3C extraction uses `opentelemetry::propagation::TraceContextPropagator`.

### 4.2 Span per envelope

For every envelope flowing through the wrapped `Transport`:

- Inbound: extract `extensions["x-vendor.opentelemetry.tracecontext"]` from
  the envelope (matching the TS contract at
  `typescript-sdk/packages/middleware/otel/src/index.ts:48` — the
  constant `OTEL_EXTENSION_NAME = "x-vendor.opentelemetry.tracecontext"`).
  Mint a `CONSUMER` span as a child.
- Outbound: mint a `PRODUCER` span; inject the active context into
  `extensions["x-vendor.opentelemetry.tracecontext"]` before calling
  `inner.send(envelope)`.
- Span name: `arcp.{type}` where `type` is the envelope's `type` field
  (e.g., `arcp.job.submit`, `arcp.job.event`, `arcp.session.ack`).

### 4.3 Attribute name parity

The TS attribute set comes from
`typescript-sdk/packages/middleware/otel/src/index.ts:extractAttributes`
(lines 139–184). The Rust adapter MUST commit to the same names so
trace queries written against either SDK work.

| TS attribute                | Source field on envelope            | Rust adapter sets it? | Notes                                                                 |
| --------------------------- | ----------------------------------- | --------------------- | --------------------------------------------------------------------- |
| `arcp.direction`            | `"in" \| "out"`                     | Yes                   |                                                                       |
| `arcp.type`                 | `envelope.type`                     | Yes                   |                                                                       |
| `arcp.id`                   | `envelope.id`                       | Yes                   |                                                                       |
| `arcp.session_id`           | `envelope.session_id`               | Yes                   |                                                                       |
| `arcp.job_id`               | `envelope.job_id`                   | Yes                   |                                                                       |
| `arcp.trace_id`             | `envelope.trace_id`                 | Yes                   | 32-hex W3C trace id, distinct from the local span's trace id.         |
| `arcp.event_seq`            | `envelope.event_seq`                | Yes                   | Encoded as `i64`.                                                     |
| `arcp.agent`                | `payload.agent`                     | Yes                   | `name@version` after resolution per §7.5.                             |
| `arcp.lease.capabilities`   | comma-joined keys of `payload.lease` or `payload.lease_request` | Yes | Matches TS line 164.                                              |
| `arcp.lease.expires_at`     | `payload.lease_constraints.expires_at` | Yes (v1.1)            | Per spec §11 addition; TS lines 167–171.                              |
| `arcp.budget.remaining`     | `payload.budget` (initial) or `payload.body.value` for `cost.budget.remaining` metric | Yes (v1.1) | **Encode as JSON string** of the per-currency map (e.g., `{"USD":1.42}`). Matches TS lines 174–181: `JSON.stringify(budget)`. |

The JSON-string encoding for `arcp.budget.remaining` is wire-format-aware:
OTel attribute values are scalars or arrays-of-scalars; per-currency map
flattening to one attribute per currency would multiply cardinality. The
TS code's `JSON.stringify` is the pragma; Rust uses `serde_json::to_string`
on the same map.

## §5. SDK ships enough without an adapter

Confirmed. The Rust SDK's `arcp-runtime` exposes a `serve(transport)`
entry point that accepts any `Transport`:

```rust
impl ArcpRuntime {
    pub async fn serve<T: Transport>(self: Arc<Self>, transport: T) -> Result<(), ARCPError>;
}
```

An end user with no host can run ARCP over `StdioTransport` (`arcp serve --stdio`)
or `MemoryTransport` (in-process tests) without touching `arcp-axum` or
`arcp-hyper`. The current `src/bin/arcp.rs` is the precedent; the
post-rewrite binary keeps that shape.

**`Transport` impl ownership:**

| Crate                    | `Transport` impls shipped                                   |
| ------------------------ | ----------------------------------------------------------- |
| `arcp-core`              | `MemoryTransport` (`src/transport/memory.rs`), `StdioTransport` (`src/transport/stdio.rs`), `WebSocketTransport` (client-side over `tokio_tungstenite::WebSocketStream`, refactored from `src/transport/websocket.rs`). |
| `arcp-axum`              | `AxumWsTransport` — wraps `axum::extract::ws::WebSocket`.   |
| `arcp-hyper`             | `HyperWsTransport` — wraps `WebSocketStream<TokioIo<Upgraded>>`. |
| `arcp-tokio-tungstenite` | `TungsteniteTransport` — client-side alias for `arcp-core`'s `WebSocketTransport` with a `connect()` helper. |
| `arcp-otel`              | (no new impl) — `TracedTransport<T>` decorator.             |

The split keeps `arcp-core` host-free: pulling `arcp-core` into a test
crate or a build that already has its own WS plumbing costs only the
`tokio_tungstenite` client surface plus stdio.

## §6. Rejected: `arcp-warp`

`warp` 0.4 is a 6-year-old project layered on `hyper` + `tower`. Its WS
attach point is `warp::ws::Ws::on_upgrade(|websocket| async move { ... })`
where `websocket: warp::filters::ws::WebSocket` is itself a thin wrapper
around `tokio_tungstenite::WebSocketStream`. The whole `warp` filter
combinator surface adds nothing over an `axum::Router` for an ARCP user
— the upgraded socket is the same `Stream + Sink` of WS messages, and
the host-allowlist + tracing layers wanted are written for `tower`, not
for the `warp::Filter` trait. A warp user can paste 12 lines of glue:
take `warp::filters::ws::WebSocket`, wrap with the same `HyperWsTransport`
construction the no-framework user gets, hand to `rt.serve(...)`. No
crate.
