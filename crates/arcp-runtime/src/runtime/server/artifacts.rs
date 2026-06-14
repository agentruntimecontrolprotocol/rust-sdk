//! Artifact put/fetch dispatch (split, #74).

#[allow(clippy::wildcard_imports)]
use super::*;

impl ARCPRuntime {
    pub(crate) async fn handle_artifact_put(
        out: &mpsc::Sender<Envelope>,
        store: &ArtifactStore,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: ArtifactPutPayload,
    ) {
        let ArtifactPutPayload {
            media_type,
            data,
            sha256,
            retain_seconds,
        } = payload;
        let mut env = match store.put(media_type, &data, retain_seconds, sha256) {
            Ok(reference) => Envelope::new(MessageType::ArtifactRef(ArtifactRefPayload {
                artifact: reference,
            })),
            Err(e) => Envelope::new(MessageType::Nack(NackPayload {
                code: e.code(),
                message: e.to_string(),
                details: None,
            })),
        };
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id);
        let _ = out.send(env).await;
    }
    pub(crate) async fn handle_artifact_fetch(
        out: &mpsc::Sender<Envelope>,
        store: &ArtifactStore,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: ArtifactFetchPayload,
    ) {
        let ArtifactFetchPayload { artifact_id } = payload;
        let mut env = match store.fetch(&artifact_id) {
            Ok((data, media_type)) => Envelope::new(MessageType::ArtifactPut(ArtifactPutPayload {
                media_type,
                data,
                sha256: None,
                retain_seconds: None,
            })),
            Err(e) => Envelope::new(MessageType::Nack(NackPayload {
                code: e.code(),
                message: e.to_string(),
                details: None,
            })),
        };
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id);
        let _ = out.send(env).await;
    }
}
