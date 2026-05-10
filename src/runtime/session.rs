//! Per-session bookkeeping owned by the runtime.

use crate::ids::SessionId;
use crate::messages::Capabilities;

/// Phase of the four-step handshake (RFC §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandshakePhase {
    /// `session.open` received; waiting for credential validation outcome.
    Opened,
    /// `session.challenge` sent; awaiting `session.authenticate`.
    Challenged,
    /// `session.accepted` sent; protocol traffic permitted.
    Accepted,
    /// Terminal: rejected or evicted.
    Closed,
}

/// Server-side bookkeeping for one session.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Session identifier.
    pub session_id: SessionId,
    /// Authenticated principal (set after `session.accepted`).
    pub principal: Option<String>,
    /// Negotiated capability set.
    pub capabilities: Capabilities,
    /// Current handshake phase.
    pub phase: HandshakePhase,
    /// Active challenge nonce (set during `Challenged`).
    pub active_challenge: Option<String>,
}

impl SessionState {
    /// Construct a new session in [`HandshakePhase::Opened`].
    #[must_use]
    pub const fn new(session_id: SessionId, capabilities: Capabilities) -> Self {
        Self {
            session_id,
            principal: None,
            capabilities,
            phase: HandshakePhase::Opened,
            active_challenge: None,
        }
    }

    /// True when the session has completed the handshake and may carry
    /// non-handshake protocol traffic.
    #[must_use]
    pub const fn is_accepted(&self) -> bool {
        matches!(self.phase, HandshakePhase::Accepted)
    }
}
