//! Paired in-memory transport for tests.
//!
//! Two endpoints exchange envelopes through bounded mpsc channels. Cloning
//! is free; closing one side propagates to the other (the next `recv`
//! returns `Ok(None)`).

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use super::Transport;
use crate::envelope::Envelope;
use crate::error::ARCPError;

const CHANNEL_CAPACITY: usize = 256;

/// One half of a paired in-memory transport.
#[derive(Clone)]
pub struct MemoryTransport {
    tx: mpsc::Sender<Envelope>,
    rx: Arc<Mutex<mpsc::Receiver<Envelope>>>,
    label: &'static str,
}

impl std::fmt::Debug for MemoryTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // mpsc Sender / Receiver don't have meaningful Debug output and the
        // channel state is uninspectable from outside; finish_non_exhaustive
        // signals the intentional omission.
        f.debug_struct("MemoryTransport")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Construct a paired (`a`, `b`) where `a.send` arrives at `b.recv` and
/// vice versa.
#[must_use]
pub fn paired() -> (MemoryTransport, MemoryTransport) {
    let (tx_a, rx_a) = mpsc::channel(CHANNEL_CAPACITY);
    let (tx_b, rx_b) = mpsc::channel(CHANNEL_CAPACITY);
    let a = MemoryTransport {
        tx: tx_b,
        rx: Arc::new(Mutex::new(rx_a)),
        label: "a",
    };
    let b = MemoryTransport {
        tx: tx_a,
        rx: Arc::new(Mutex::new(rx_b)),
        label: "b",
    };
    (a, b)
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn send(&self, envelope: Envelope) -> Result<(), ARCPError> {
        self.tx
            .send(envelope)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: format!("memory transport ({}): peer closed", self.label),
            })
    }

    async fn recv(&self) -> Result<Option<Envelope>, ARCPError> {
        let mut guard = self.rx.lock().await;
        Ok(guard.recv().await)
    }

    async fn close(&self) -> Result<(), ARCPError> {
        // Closing the send side signals the peer's recv to terminate. mpsc
        // closes when all senders are dropped; we can't drop self.tx here,
        // so closing is implicit on transport drop. This is a best-effort
        // signal: spawn-tasks holding clones will keep the channel alive.
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
    async fn paired_transports_exchange_envelopes() {
        let (a, b) = paired();
        let env = Envelope::new(MessageType::Ping(PingPayload::default()));
        let env_id = env.id.clone();
        a.send(env).await.expect("send");
        let received = b.recv().await.expect("recv").expect("present");
        assert_eq!(received.id, env_id);
    }

    #[tokio::test]
    async fn closed_peer_makes_recv_return_none() {
        let (a, b) = paired();
        drop(a);
        let received = b.recv().await.expect("recv");
        assert!(received.is_none());
    }
}
