//! SDR domain via custom `arcpx.sdr.*.v1` extension messages.
//!
//! Tune to 145.500 MHz (2 m FM calling), capture 5 s of IQ at 2.048 MS/s,
//! NBFM-demodulate to 48 kHz PCM. Exercises §21 naming, capability
//! advertisement, and unknown-message handling.

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

use arcp::error::ARCPError;
use arcp::messages::Capabilities;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, ErrorCode};
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

const EXT_TUNE: &str = "arcpx.sdr.tune.v1";
const EXT_GAIN: &str = "arcpx.sdr.gain.v1";
const EXT_CAPTURE: &str = "arcpx.sdr.capture.v1";
const EXT_DEMODULATE: &str = "arcpx.sdr.demodulate.v1";
const ALL_EXTENSIONS: &[&str] = &[EXT_TUNE, EXT_GAIN, EXT_CAPTURE, EXT_DEMODULATE];

fn advertised(_caps: &Capabilities) -> Vec<String> {
    // Capabilities.extensions on session.accepted (RFC §7 / §21.2).
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // capabilities.extensions=ALL_EXTENSIONS on the open call.
    let client: Client = todo!();
    let negotiated: Capabilities = todo!(); // client.negotiated_capabilities()
    let adv = advertised(&negotiated);
    if !ALL_EXTENSIONS.iter().all(|e| adv.iter().any(|a| a == e)) {
        return Err(ARCPError::Unimplemented {
            section: "21.2",
            detail: format!("runtime missing SDR extensions: advertised={adv:?}"),
        }
        .into());
    }

    let handle = "<uuid-hex-8>";

    // Tune (synchronous request/response).
    let _tune_payload = json!({
        "center_freq_hz": 145_500_000.0_f64,
        "sample_rate_hz": 2_048_000.0_f64,
        "ppm_correction": 1,
    });
    // client.request(envelope(EXT_TUNE, payload=_tune_payload), timeout=10s)

    // Set gain.
    let _gain_payload = json!({
        "stages": [{"name": "TUNER", "value_db": 28.0_f64}],
    });
    // client.request(envelope(EXT_GAIN, payload=_gain_payload), timeout=10s)

    // Capture returns an artifact.ref; IQ never travels inline.
    let _capture_payload = json!({
        "seconds": 5.0_f64,
        "capture_handle": handle,
        "decimate": 1,
    });
    let iq_artifact: String = todo!(); // cap.payload["artifact_id"]
    println!("captured IQ -> {iq_artifact}");

    let _demod_payload = json!({
        "iq_artifact_id": iq_artifact,
        "mode": "NBFM",
        "audio_rate_hz": 48_000,
    });
    let audio_artifact: String = todo!();
    println!("demod  PCM -> {audio_artifact}");

    // §21.3 demonstration: unadvertised extension marked optional. Runtime
    // SHOULD ack (silent drop) rather than nack.
    let _optional_payload = json!({"velocity_mps": 7.4_f64});
    // client.request(envelope("arcpx.sdr.experimental_doppler.v1",
    //   extensions={"optional": true}, payload=_optional_payload), timeout=5s)
    println!("optional unknown -> ack");

    Ok(())
}
