//! Optional offline Ask AI assistant: hardware detection, approved open-weight model
//! registry, localhost-only Ollama client, and fail-closed user-approval gate.
//!
//! The core privacy and protection engine operates 100% independently of this module.
//! No remote cloud endpoints are ever contacted, and no arbitrary commands can be executed.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfile {
    pub cpu_threads: usize,
    pub total_ram_mib: u64,
    pub available_ram_mib: u64,
    pub gpu_backend: Option<String>,
    pub execution_mode: &'static str,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let cpu_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let (total_ram_mib, available_ram_mib) = if let Ok(mut sys) = System::new() {
            sys.refresh_memory();
            (
                sys.total_memory() / (1024 * 1024),
                sys.available_memory() / (1024 * 1024),
            )
        } else {
            (0, 0)
        };
        Self {
            cpu_threads,
            total_ram_mib,
            available_ram_mib,
            gpu_backend: None,
            execution_mode: "CPU fallback (GPU acceleration not enabled in this build)",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ApprovedAiModel {
    pub id: &'static str,
    pub source: &'static str,
    pub license: &'static str,
    pub min_ram_mib: u64,
}

pub const APPROVED_MODELS: &[ApprovedAiModel] = &[
    ApprovedAiModel {
        id: "qwen2.5:3b",
        source: "Ollama / Qwen/Qwen2.5-3B-Instruct",
        license: "Apache-2.0",
        min_ram_mib: 4096,
    },
    ApprovedAiModel {
        id: "phi3.5:mini",
        source: "Ollama / microsoft/Phi-3.5-mini-instruct",
        license: "MIT",
        min_ram_mib: 4096,
    },
    ApprovedAiModel {
        id: "mistral:7b-instruct",
        source: "Ollama / mistralai/Mistral-7B-Instruct-v0.3",
        license: "Apache-2.0",
        min_ram_mib: 8192,
    },
    ApprovedAiModel {
        id: "llama3.2:3b",
        source: "Ollama / meta-llama/Llama-3.2-3B-Instruct",
        license: "Llama-3.2-Community",
        min_ram_mib: 4096,
    },
];

pub fn find_approved_model(id: &str) -> Option<&'static ApprovedAiModel> {
    APPROVED_MODELS.iter().find(|m| m.id == id)
}

/// Verifies a local `.gguf` or `.safetensors` model file against an expected SHA-256 hex digest.
/// Refuses pickle/PyTorch executable checkpoints.
pub fn verify_local_model_file(path: &Path, expected_sha256: &str) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    ensure!(
        matches!(ext.as_str(), "gguf" | "safetensors"),
        "Refusing unsupported model format '.{ext}'; only .gguf and .safetensors are allowed"
    );
    let mut f = std::fs::File::open(path).with_context(|| format!("Open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = format!("{:x}", hasher.finalize());
    ensure!(
        got == expected_sha256.to_ascii_lowercase(),
        "SHA-256 mismatch for {}: expected {expected_sha256}, got {got}",
        path.display()
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposedAction {
    SuggestRedactCategory(String),
    SuggestStripMetadata,
    SuggestExportProfile(String),
}

/// Fail-closed user-approval gate for any consequential AI suggestion.
pub fn execute_if_approved(action: &ProposedAction, approved_by_user: bool) -> Result<String> {
    ensure!(
        approved_by_user,
        "Action {:?} requires explicit user approval before execution",
        action
    );
    Ok(match action {
        ProposedAction::SuggestRedactCategory(cat) => {
            format!("Approved: apply redaction for category '{cat}'")
        }
        ProposedAction::SuggestStripMetadata => "Approved: strip container metadata".into(),
        ProposedAction::SuggestExportProfile(p) => {
            format!("Approved: switch export profile to '{p}'")
        }
    })
}

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "127.0.0.1:11434".into(),
            model: "qwen2.5:3b".into(),
        }
    }
}

fn validate_loopback_endpoint(endpoint: &str) -> Result<SocketAddr> {
    let trimmed = endpoint
        .trim()
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let addr: SocketAddr = if let Some(port) = trimmed.strip_prefix("localhost:") {
        format!("127.0.0.1:{port}")
            .parse()
            .context("Invalid localhost port")?
    } else {
        trimmed
            .parse()
            .context("Endpoint must be an explicit loopback IP:port (e.g. 127.0.0.1:11434)")?
    };
    ensure!(
        addr.ip().is_loopback(),
        "Refusing non-loopback endpoint '{}': Ask AI is strictly offline/local-only",
        addr
    );
    Ok(addr)
}

/// Queries a local Ollama instance (`127.0.0.1:11434`) only when `cfg.enabled == true` and
/// the selected model is in `APPROVED_MODELS`. Never contacts remote hosts.
pub fn ask_ollama_local(cfg: &OllamaConfig, prompt: &str) -> Result<String> {
    ensure!(
        cfg.enabled,
        "Ask AI is disabled by default; enable local Ollama explicitly first"
    );
    ensure!(
        find_approved_model(&cfg.model).is_some(),
        "Model '{}' is not in the licence-approved registry",
        cfg.model
    );
    let addr = validate_loopback_endpoint(&cfg.endpoint)?;
    let body = serde_json::json!({
        "model": cfg.model,
        "prompt": prompt,
        "stream": false
    })
    .to_string();

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .with_context(|| format!("Local Ollama not reachable at {addr}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let req = format!(
        "POST /api/generate HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes())?;
    // A local server is still untrusted input: cap the response at 4 MiB.
    let mut raw = String::new();
    (&mut stream)
        .take(4 * 1024 * 1024)
        .read_to_string(&mut raw)?;
    let json_part = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .context("Malformed HTTP response from local Ollama")?;
    let val: serde_json::Value = serde_json::from_str(json_part)?;
    Ok(val
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_when_disabled_and_refuses_remote_hosts() {
        let cfg = OllamaConfig::default();
        assert!(ask_ollama_local(&cfg, "hello").is_err());

        let remote = OllamaConfig {
            enabled: true,
            endpoint: "8.8.8.8:11434".into(),
            model: "qwen2.5:3b".into(),
        };
        let err = ask_ollama_local(&remote, "hello").unwrap_err().to_string();
        assert!(err.contains("non-loopback"));

        let unapproved = OllamaConfig {
            enabled: true,
            endpoint: "127.0.0.1:11434".into(),
            model: "unlisted-model:latest".into(),
        };
        assert!(ask_ollama_local(&unapproved, "hello").is_err());
    }

    #[test]
    fn approved_model_registry_and_local_sha256_gate() {
        assert!(find_approved_model("qwen2.5:3b").is_some());
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("weights.bin");
        std::fs::write(&bad, b"pickle").unwrap();
        assert!(verify_local_model_file(&bad, "0000").is_err());

        let gguf = dir.path().join("model.gguf");
        std::fs::write(&gguf, b"gguf-bytes").unwrap();
        let expected = format!("{:x}", Sha256::digest(b"gguf-bytes"));
        verify_local_model_file(&gguf, &expected).unwrap();
    }

    #[test]
    fn consequential_action_requires_explicit_user_approval() {
        let act = ProposedAction::SuggestStripMetadata;
        assert!(execute_if_approved(&act, false).is_err());
        assert!(execute_if_approved(&act, true).is_ok());
    }

    #[test]
    fn hardware_profile_reports_cpu_and_ram() {
        let hw = HardwareProfile::detect();
        assert!(hw.cpu_threads >= 1);
        assert!(hw.total_ram_mib > 0);
    }
}
