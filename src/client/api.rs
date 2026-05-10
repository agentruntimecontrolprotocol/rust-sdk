//! `ARCPClient` and the type-state [`Session<S>`] (RFC §4.6, §8).

use std::marker::PhantomData;
use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::ids::SessionId;
use crate::messages::{
    Capabilities, ClientIdentity, Credentials, MessageType, SessionAcceptedPayload,
    SessionOpenPayload,
};
use crate::transport::Transport;

/// Marker trait sealed inside this module — only [`Unauthenticated`] and
/// [`Authenticated`] satisfy it.
mod sealed {
    pub trait State {}
    impl State for super::Unauthenticated {}
    impl State for super::Authenticated {}
}

/// Type-state marker: the session has not yet completed `session.accepted`.
#[derive(Debug)]
pub struct Unauthenticated;

/// Type-state marker: the session has completed `session.accepted`.
#[derive(Debug)]
pub struct Authenticated;

/// Type-state session handle.
///
/// `Session<Unauthenticated>` exposes only [`Session::authenticate`].
/// `Session<Authenticated>` exposes the rest of the protocol surface
/// (Phase 3+ adds `invoke`, `subscribe`, etc.).
pub struct Session<S: sealed::State, T: Transport + 'static> {
    inner: Arc<SessionInner<T>>,
    _state: PhantomData<S>,
}

struct SessionInner<T: Transport + 'static> {
    transport: T,
    session_id: tokio::sync::Mutex<Option<SessionId>>,
    capabilities: tokio::sync::Mutex<Capabilities>,
}

impl<S: sealed::State, T: Transport + 'static> std::fmt::Debug for Session<S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("state", &std::any::type_name::<S>())
            .finish_non_exhaustive()
    }
}

impl<T: Transport + 'static> Session<Unauthenticated, T> {
    /// Drive the four-step handshake (RFC §8.1) and, on success, return a
    /// [`Session<Authenticated>`].
    ///
    /// `client` is the attestation block sent in `session.open`. `caps` is
    /// the capability set the client offers; the runtime will negotiate
    /// down.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unauthenticated`] if the runtime emits
    /// `session.rejected` or `session.unauthenticated`,
    /// [`ARCPError::Unavailable`] if the transport closes mid-handshake,
    /// [`ARCPError::Internal`] for protocol violations.
    pub async fn authenticate(
        self,
        creds: Credentials,
        client: ClientIdentity,
        caps: Capabilities,
    ) -> Result<Session<Authenticated, T>, ARCPError> {
        let open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
            auth: creds.clone(),
            client,
            capabilities: caps,
        }));
        let open_id = open.id.clone();
        self.inner.transport.send(open).await?;

        let env = self
            .inner
            .transport
            .recv()
            .await?
            .ok_or_else(|| ARCPError::Unavailable {
                detail: "transport closed during handshake".into(),
            })?;
        match env.payload {
            MessageType::SessionAccepted(SessionAcceptedPayload {
                session_id,
                capabilities,
                ..
            }) => {
                *self.inner.session_id.lock().await = Some(session_id);
                *self.inner.capabilities.lock().await = capabilities;
                Ok(Session {
                    inner: self.inner.clone(),
                    _state: PhantomData,
                })
            }
            MessageType::SessionRejected(p) => Err(ARCPError::Unauthenticated {
                detail: format!("session.rejected ({}): {}", p.code, p.message),
            }),
            MessageType::SessionUnauthenticated(p) => Err(ARCPError::Unauthenticated {
                detail: format!("session.unauthenticated ({}): {}", p.code, p.message),
            }),
            MessageType::SessionChallenge(p) => Err(ARCPError::Unauthenticated {
                detail: format!(
                    "runtime issued a challenge (\"{}\") but Phase 2 client cannot respond; \
                     correlation_id={}",
                    p.challenge, open_id
                ),
            }),
            other => Err(ARCPError::Internal {
                detail: format!("unexpected handshake response: type={}", other.type_name()),
            }),
        }
    }
}

impl<T: Transport + 'static> Session<Authenticated, T> {
    /// Return the negotiated session id.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Internal`] if called on a session that somehow
    /// reached the `Authenticated` state without an id (cannot happen in
    /// well-formed code).
    pub async fn id(&self) -> Result<SessionId, ARCPError> {
        self.inner
            .session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| ARCPError::Internal {
                detail: "authenticated session missing id".into(),
            })
    }

    /// Return the negotiated capability set.
    pub async fn capabilities(&self) -> Capabilities {
        self.inner.capabilities.lock().await.clone()
    }

    /// Phase 3+ surface placeholder. Returns
    /// [`ARCPError::Unimplemented`] so the public type already names the
    /// future API surface.
    ///
    /// # Errors
    ///
    /// Always returns [`ARCPError::Unimplemented`] in Phase 2.
    pub fn invoke(&self, _tool: &str, _arguments: serde_json::Value) -> Result<(), ARCPError> {
        Err(ARCPError::Unimplemented {
            section: "10",
            detail: "Session::invoke lands in Phase 3".into(),
        })
    }
}

/// Client-side entry point.
pub struct ARCPClient<T: Transport + 'static> {
    transport: Option<T>,
}

impl<T: Transport + 'static> std::fmt::Debug for ARCPClient<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ARCPClient")
            .field("attached", &self.transport.is_some())
            .finish()
    }
}

impl<T: Transport + 'static> ARCPClient<T> {
    /// Construct over an attached transport.
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self {
            transport: Some(transport),
        }
    }

    /// Open an unauthenticated session.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] with code [`ErrorCode::FailedPrecondition`] if
    /// the client has already opened its session (the underlying transport
    /// is consumed at that point).
    pub fn open(mut self) -> Result<Session<Unauthenticated, T>, ARCPError> {
        let transport = self
            .transport
            .take()
            .ok_or_else(|| ARCPError::FailedPrecondition {
                detail: "client transport has already been consumed".into(),
            })?;
        // Discard ErrorCode usage to silence dead-code warnings on the import.
        let _ = ErrorCode::FailedPrecondition;
        Ok(Session {
            inner: Arc::new(SessionInner {
                transport,
                session_id: tokio::sync::Mutex::new(None),
                capabilities: tokio::sync::Mutex::new(Capabilities::default()),
            }),
            _state: PhantomData,
        })
    }
}
