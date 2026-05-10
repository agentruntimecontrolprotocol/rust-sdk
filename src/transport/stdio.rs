//! stdio transport (RFC §22).
//!
//! Newline-delimited JSON over an `AsyncRead` + `AsyncWrite` pair. Process
//! stdin/stdout is the primary use case ([`StdioTransport::process`]); for
//! testing, [`StdioTransport::from_streams`] accepts arbitrary
//! [`AsyncRead`]/[`AsyncWrite`] (e.g. `tokio::io::duplex`).
//!
//! Sidecar binary frames (RFC §11.3) are not supported on stdio per spec —
//! base64 in-envelope only.

#![allow(clippy::significant_drop_tightening)]

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use super::Transport;
use crate::envelope::Envelope;
use crate::error::ARCPError;

/// stdio transport over arbitrary reader/writer halves.
pub struct StdioTransport {
    reader: Mutex<BufReader<Box<dyn AsyncRead + Unpin + Send>>>,
    writer: Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
}

impl std::fmt::Debug for StdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioTransport").finish_non_exhaustive()
    }
}

impl StdioTransport {
    /// Construct from arbitrary streams. Useful for tests via
    /// `tokio::io::duplex` or for adapting to non-process pipes.
    pub fn from_streams<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Self {
            reader: Mutex::new(BufReader::new(Box::new(reader))),
            writer: Mutex::new(Box::new(writer)),
        }
    }

    /// Construct from the current process's stdin / stdout.
    #[must_use]
    pub fn process() -> Self {
        Self::from_streams(tokio::io::stdin(), tokio::io::stdout())
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send(&self, envelope: Envelope) -> Result<(), ARCPError> {
        let line = serde_json::to_string(&envelope)?;
        let mut w = self.writer.lock().await;
        w.write_all(line.as_bytes())
            .await
            .map_err(|e| ARCPError::Internal {
                detail: format!("stdio write failed: {e}"),
            })?;
        w.write_all(b"\n").await.map_err(|e| ARCPError::Internal {
            detail: format!("stdio write newline failed: {e}"),
        })?;
        w.flush().await.map_err(|e| ARCPError::Internal {
            detail: format!("stdio flush failed: {e}"),
        })?;
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Envelope>, ARCPError> {
        let mut buf = String::new();
        let n = self
            .reader
            .lock()
            .await
            .read_line(&mut buf)
            .await
            .map_err(|e| ARCPError::Internal {
                detail: format!("stdio read failed: {e}"),
            })?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = buf.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return Ok(None);
        }
        let env: Envelope = serde_json::from_str(trimmed)?;
        Ok(Some(env))
    }

    async fn close(&self) -> Result<(), ARCPError> {
        let mut w = self.writer.lock().await;
        w.shutdown().await.ok();
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
    async fn duplex_paired_transports_round_trip() {
        // Wire two StdioTransports together with `tokio::io::duplex` so
        // a write on one shows up as a read on the other.
        let (a_r, b_w) = tokio::io::duplex(8192);
        let (b_r, a_w) = tokio::io::duplex(8192);
        let a = StdioTransport::from_streams(a_r, a_w);
        let b = StdioTransport::from_streams(b_r, b_w);

        let env = Envelope::new(MessageType::Ping(PingPayload::default()));
        let id = env.id.clone();
        a.send(env).await.expect("send");
        let received = b.recv().await.expect("recv").expect("present");
        assert_eq!(received.id, id);
    }

    #[tokio::test]
    async fn closed_writer_makes_recv_return_none() {
        let (a_r, b_w) = tokio::io::duplex(8);
        let (_b_r, a_w) = tokio::io::duplex(8);
        let a = StdioTransport::from_streams(a_r, a_w);
        drop(b_w); // close peer's writer; a's read sees EOF
        let received = a.recv().await.expect("recv");
        assert!(received.is_none());
    }
}
