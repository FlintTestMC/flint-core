//! Clickable links from a failing flint run to FlintVisualizer.
//!
//! When `flint-steel` (or any other adapter) sees a failing test, it can build
//! a [`FailurePayload`] from the spec + result and turn it into a URL pointing
//! at a flint-viz instance. Clicking the URL opens the test with the failing
//! tick auto-seeked and the expected/actual blocks overlaid in 3D.
//!
//! The encoded payload lives in the URL **fragment** (`#data=...`) so it never
//! reaches the server in HTTP requests — flint-viz decodes it client-side
//! (which round-trips through `POST /api/failure/decode`, server-side, to
//! reuse this crate's `TestSpec` deserializer).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

use crate::results::AssertFailure;
use crate::test_spec::TestSpec;

/// Schema version of [`FailurePayload`]. Bumped on breaking changes so flint-viz
/// can refuse mismatched payloads with a friendly error rather than crash.
pub const PAYLOAD_VERSION: u8 = 1;

/// Self-contained description of a failing test run, encodable into a URL
/// fragment for sharing between flint-steel and flint-viz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePayload {
    pub version: u8,
    /// Inline copy of the test spec — self-contained for CI / no-checkout viewing.
    pub spec: TestSpec,
    /// Optional relative path of the test file, for live-edit-on-disk in local dev.
    /// flint-viz tries this first; on 404 it falls back to `spec`.
    pub source_path: Option<PathBuf>,
    /// Failing assertions from the run. Today's runner stops at the first
    /// failure (so `len() == 1`); the type is a `Vec` so a future runner that
    /// collects every assert at the failing tick is forward-compatible without
    /// a schema bump.
    pub failures: Vec<AssertFailure>,
    /// Total ticks executed before the run ended.
    pub total_ticks: u32,
}

impl FailurePayload {
    pub fn new(
        spec: TestSpec,
        source_path: Option<PathBuf>,
        failures: Vec<AssertFailure>,
        total_ticks: u32,
    ) -> Self {
        Self {
            version: PAYLOAD_VERSION,
            spec,
            source_path,
            failures,
            total_ticks,
        }
    }
}

/// Encode a payload as a URL-safe base64 string of gzipped JSON.
pub fn encode(payload: &FailurePayload) -> Result<String> {
    let json = serde_json::to_vec(payload).context("serialize FailurePayload")?;
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(&json).context("gzip write")?;
    let compressed = gz.finish().context("gzip finish")?;
    Ok(URL_SAFE_NO_PAD.encode(compressed))
}

/// Decode a URL-safe base64 + gzipped JSON blob back into a payload.
///
/// Returns an error on malformed base64, malformed gzip, malformed JSON, or
/// schema-version mismatch.
pub fn decode(encoded: &str) -> Result<FailurePayload> {
    let compressed = URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .context("base64 decode")?;
    let mut gz = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    gz.read_to_end(&mut json).context("gzip read")?;
    let payload: FailurePayload =
        serde_json::from_slice(&json).context("deserialize FailurePayload")?;
    if payload.version != PAYLOAD_VERSION {
        return Err(anyhow!(
            "FailurePayload version mismatch: got {}, expected {}",
            payload.version,
            PAYLOAD_VERSION
        ));
    }
    Ok(payload)
}

/// Build the clickable URL for a payload. `base` is the flint-viz host root
/// (e.g. `http://localhost:7878`); the fragment carries the encoded payload so
/// the server never sees it in request URLs / access logs.
pub fn failure_url(payload: &FailurePayload, base: &str) -> Result<String> {
    let encoded = encode(payload)?;
    let trimmed = base.trim_end_matches('/');
    Ok(format!("{trimmed}/failure#data={encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{AssertPosition, InfoType};
    use crate::test_spec::Block;

    fn sample_failure() -> AssertFailure {
        AssertFailure {
            tick: 7,
            error_message: "expected stone, got air".to_string(),
            position: AssertPosition::from_array([1, 2, 3]),
            execution_time_ms: Some(2),
            expected: InfoType::Block(Block::new("minecraft:stone")),
            actual: InfoType::Block(Block::new("minecraft:air")),
        }
    }

    fn sample_payload() -> FailurePayload {
        FailurePayload::new(
            TestSpec {
                flint_version: None,
                name: "round-trip".to_string(),
                description: None,
                tags: Vec::new(),
                minecraft_ids: Vec::new(),
                dependencies: Vec::new(),
                setup: None,
                timeline: Vec::new(),
                breakpoints: Vec::new(),
            },
            Some(PathBuf::from("tests/round-trip.flint.json")),
            vec![sample_failure()],
            42,
        )
    }

    #[test]
    fn round_trips() {
        let payload = sample_payload();
        let encoded = encode(&payload).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.version, PAYLOAD_VERSION);
        assert_eq!(decoded.spec.name, "round-trip");
        assert_eq!(decoded.failures.len(), 1);
        assert_eq!(decoded.failures[0].tick, 7);
        assert_eq!(decoded.total_ticks, 42);
        assert_eq!(
            decoded.source_path.as_deref(),
            Some(std::path::Path::new("tests/round-trip.flint.json"))
        );
    }

    #[test]
    fn encoded_blob_is_url_safe() {
        let encoded = encode(&sample_payload()).unwrap();
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "encoded blob must be URL-safe (got: {encoded})",
        );
    }

    #[test]
    fn rejects_wrong_version() {
        let mut payload = sample_payload();
        payload.version = 99;
        let encoded = encode(&payload).unwrap();
        let err = decode(&encoded).unwrap_err();
        assert!(
            err.to_string().contains("version mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("not base64!!!").is_err());
        assert!(decode("aGVsbG8").is_err()); // valid base64, bad gzip
    }

    #[test]
    fn failure_url_uses_fragment_and_trims_trailing_slash() {
        let url = failure_url(&sample_payload(), "http://localhost:7878/").unwrap();
        assert!(url.starts_with("http://localhost:7878/failure#data="));
        assert!(!url.contains("?data="));
    }

    #[test]
    fn supports_zero_failures_for_forward_compat() {
        // No failures (e.g. flint-core could later use the payload format for
        // passing-test snapshots). Decoding shouldn't blow up.
        let mut payload = sample_payload();
        payload.failures.clear();
        let encoded = encode(&payload).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert!(decoded.failures.is_empty());
    }
}
