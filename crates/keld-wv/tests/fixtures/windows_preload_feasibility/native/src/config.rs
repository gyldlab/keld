//! Strict fixture configuration, not a product URL or permission parser.
use crate::digest;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) raw: Value,
    pub(crate) mode: String,
    pub(crate) variant: String,
    pub(crate) url: String,
    pub(crate) successor: Option<String>,
    pub(crate) bundle: Option<String>,
    pub(crate) expected_bundle_digest: Option<String>,
    pub(crate) bundle_digest: Option<String>,
    pub(crate) out_dir: PathBuf,
    pub(crate) timeout: Duration,
    pub(crate) cleanup_timeout: Duration,
    pub(crate) visible: bool,
    pub(crate) busy_ms: u64,
    pub(crate) burst: u64,
    pub(crate) observe_ms: u64,
}

impl Config {
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let raw: Value = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("read config: {e}"))?,
        )
        .map_err(|e| format!("parse config: {e}"))?;
        let object = raw.as_object().ok_or("config must be a JSON object")?;
        for key in object.keys() {
            if ![
                "mode",
                "variant",
                "url",
                "successorUrl",
                "bundlePath",
                "bundleSha256",
                "outDir",
                "timeoutMs",
                "cleanupTimeoutMs",
                "visible",
                "busyMs",
                "burst",
                "observeMs",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unknown config field: {key}"));
            }
        }
        let required = |key: &str| -> Result<String, String> {
            raw[key]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must be a nonempty string"))
        };
        let number = |key: &str, default: u64, max: u64| -> Result<u64, String> {
            let n = if raw.get(key).is_none() {
                default
            } else {
                raw[key]
                    .as_u64()
                    .ok_or_else(|| format!("{key} must be an unsigned integer"))?
            };
            if n == 0 || n > max {
                return Err(format!("{key} must be in 1..={max}"));
            }
            Ok(n)
        };
        let mode = required("mode")?;
        let variant = required("variant")?;
        match (mode.as_str(), variant.as_str()) {
            ("order", "document-created" | "dom-content-loaded")
            | ("stale-delivery", "guarded" | "unfenced")
            | ("contentloading-race", "busy" | "pagehide" | "unload" | "no-force") => {}
            _ => return Err("unknown mode/variant combination".to_owned()),
        }
        let url = required("url")?;
        let successor = if mode == "order" {
            None
        } else {
            Some(required("successorUrl")?)
        };
        if successor.as_ref() == Some(&url) {
            return Err(
                "use distinct fixture query tags for A and successor observations".to_owned(),
            );
        }
        let bundle = match raw.get("bundlePath") {
            None => None,
            Some(Value::String(p)) => {
                Some(std::fs::read_to_string(p).map_err(|e| format!("read bundlePath: {e}"))?)
            }
            _ => return Err("bundlePath must be a string".to_owned()),
        };
        if mode == "order" && bundle.is_none() {
            return Err("order mode requires bundlePath".to_owned());
        }
        let expected_bundle_digest = if bundle.is_some() {
            let expected = required("bundleSha256")?.to_ascii_lowercase();
            if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("bundleSha256 must be exactly 64 hexadecimal digits".to_owned());
            }
            Some(expected)
        } else {
            None
        };
        let bundle_digest = bundle
            .as_ref()
            .map(|source| digest::sha256(source.as_bytes()));
        let out_dir = PathBuf::from(required("outDir")?);
        if !out_dir.is_absolute() {
            return Err("outDir must be absolute".to_owned());
        }
        let timeout = Duration::from_millis(number("timeoutMs", 15_000, 60_000)?);
        let cleanup_timeout = Duration::from_millis(number("cleanupTimeoutMs", 20_000, 60_000)?);
        let busy_ms = number("busyMs", 200, 1000)?;
        if ![50, 200, 1000].contains(&busy_ms) {
            return Err("busyMs must be 50, 200, or 1000".to_owned());
        }
        let burst = number("burst", 256, 4096)?;
        let observe_ms = number("observeMs", 1500, 5000)?;
        let visible = match raw.get("visible") {
            None => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err("visible must be Boolean".to_owned()),
        };
        Ok(Self {
            raw,
            mode,
            variant,
            url,
            successor,
            bundle,
            expected_bundle_digest,
            bundle_digest,
            out_dir,
            timeout,
            cleanup_timeout,
            visible,
            busy_ms,
            burst,
            observe_ms,
        })
    }
}
