//! WebSocket transport (ARCP v1.1 §4: WebSocket is mandatory for network
//! deployments).
//!
//! Backed by `tokio_tungstenite`. Each ARCP envelope rides as a single
//! `Text` WebSocket frame containing the envelope's JSON. Sidecar binary
//! frames are not implemented; binary stream chunks travel in the
//! in-envelope base64 form (see ARCP v1.1 §8.4 `result_chunk` encoding).
//!
//! The connection helpers are intentionally thin:
//!
//! - [`WebSocketTransport::dial`] — outbound connect to a `ws://` URL.
//! - [`WebSocketTransport::accept_stream`] — wrap an already-accepted
//!   `WebSocketStream` (the caller usually built it with
//!   `tokio_tungstenite::accept_async`).
//! - [`WebSocketTransport::serve_loopback`] — convenience for tests:
//!   binds an ephemeral localhost port and yields paired
//!   `(server_transport, client_transport)`.
//!
//! Reconnection with exponential backoff is documented as a follow-up.

use async_trait::async_trait;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::Transport;
use crate::envelope::Envelope;
use crate::error::ARCPError;

/// WebSocket transport over an established `WebSocketStream`.
pub struct WebSocketTransport {
    sink: Mutex<Box<dyn WsSink>>,
    stream: Mutex<Box<dyn WsStream>>,
}

trait WsSink: Send + Unpin {
    fn send_text<'a>(
        &'a mut self,
        text: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ARCPError>> + Send + 'a>>;
}

trait WsStream: Send + Unpin {
    fn next_message<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Message>, ARCPError>> + Send + 'a>,
    >;
}

impl<S> WsSink for SplitSink<WebSocketStream<S>, Message>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    fn send_text<'a>(
        &'a mut self,
        text: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ARCPError>> + Send + 'a>>
    {
        Box::pin(async move {
            #[allow(clippy::useless_conversion)] // Utf8Bytes accepts String via From
            let msg = Message::Text(text.into());
            self.send(msg).await.map_err(|e| ARCPError::Internal {
                detail: format!("ws send: {e}"),
            })
        })
    }
}

impl<S> WsStream for SplitStream<WebSocketStream<S>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    fn next_message<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Message>, ARCPError>> + Send + 'a>,
    > {
        Box::pin(async move {
            match self.next().await {
                Some(Ok(m)) => Ok(Some(m)),
                Some(Err(e)) => Err(ARCPError::Internal {
                    detail: format!("ws recv: {e}"),
                }),
                None => Ok(None),
            }
        })
    }
}

impl std::fmt::Debug for WebSocketTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketTransport").finish_non_exhaustive()
    }
}

impl WebSocketTransport {
    /// Wrap an already-established `WebSocketStream` (server side, after
    /// `tokio_tungstenite::accept_async`).
    pub fn accept_stream<S>(ws: WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (sink, stream) = ws.split();
        Self {
            sink: Mutex::new(Box::new(sink)),
            stream: Mutex::new(Box::new(stream)),
        }
    }

    /// Dial `url`. Suitable for `ws://localhost:7777`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the connect fails.
    pub async fn dial(url: &str) -> Result<Self, ARCPError> {
        let (ws, _resp) =
            tokio_tungstenite::connect_async(url)
                .await
                .map_err(|e| ARCPError::Unavailable {
                    detail: format!("ws dial {url}: {e}"),
                })?;
        Ok(Self::wrap_maybe_tls(ws))
    }

    fn wrap_maybe_tls(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        let (sink, stream) = ws.split();
        Self {
            sink: Mutex::new(Box::new(sink)),
            stream: Mutex::new(Box::new(stream)),
        }
    }

    /// Convenience for tests: bind an ephemeral localhost port, accept the
    /// first inbound connection, and return `(server_transport,
    /// client_transport)`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] for any underlying I/O failure.
    pub async fn serve_loopback() -> Result<(Self, Self), ARCPError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| ARCPError::Unavailable {
                detail: format!("ws bind: {e}"),
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| ARCPError::Unavailable {
                detail: format!("ws addr: {e}"),
            })?
            .port();
        let url = format!("ws://127.0.0.1:{port}/");

        let accept_handle = tokio::spawn(async move {
            let (sock, _) = listener
                .accept()
                .await
                .map_err(|e| ARCPError::Unavailable {
                    detail: format!("ws accept: {e}"),
                })?;
            let ws = tokio_tungstenite::accept_async(sock).await.map_err(|e| {
                ARCPError::Unavailable {
                    detail: format!("ws handshake: {e}"),
                }
            })?;
            Ok::<_, ARCPError>(Self::accept_stream(ws))
        });

        let client = Self::dial(&url).await?;
        let server = accept_handle.await.map_err(|e| ARCPError::Internal {
            detail: format!("ws accept join: {e}"),
        })??;
        Ok((server, client))
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn send(&self, envelope: Envelope) -> Result<(), ARCPError> {
        let text = serde_json::to_string(&envelope)?;
        self.sink.lock().await.send_text(text).await
    }

    async fn recv(&self) -> Result<Option<Envelope>, ARCPError> {
        loop {
            let msg = self.stream.lock().await.next_message().await?;
            match msg {
                Some(Message::Text(text)) => {
                    let env: Envelope = serde_json::from_str(text.as_str())?;
                    return Ok(Some(env));
                }
                // The SDK ignores sidecar binary frames (not part of v1.1);
                // tungstenite handles control frames internally.
                Some(
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_),
                ) => {}
                Some(Message::Close(_)) | None => return Ok(None),
            }
        }
    }

    async fn close(&self) -> Result<(), ARCPError> {
        // SinkExt::close requires &mut Self, which the trait object can't
        // give us through the wrapper without an extra method on the trait.
        // For v0.1, dropping the transport tears the connection down.
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;
    use crate::messages::{MessageType, PingPayload};

    #[tokio::test]
    async fn loopback_round_trip() {
        let (server, client) = WebSocketTransport::serve_loopback()
            .await
            .expect("loopback");
        let env = Envelope::new(MessageType::Ping(PingPayload::default()));
        let id = env.id.clone();
        client.send(env).await.expect("send");
        let received = server.recv().await.expect("recv").expect("present");
        assert_eq!(received.id, id);
    }
}
