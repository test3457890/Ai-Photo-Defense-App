//! Offline, fail-closed model verification. No remote code execution or pickle support.
#[cfg(feature = "ask-ai")]
pub mod ai_assistant;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
pub const MAX_MODEL_BYTES: u64 = 1024 * 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub filename: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub source_url: String,
    pub code_license: String,
    pub weights_license: String,
    /// A reviewer confirmed the licence permits running these weights locally in this app.
    pub local_use_approved: bool,
    /// A reviewer confirmed the licence permits redistributing the file (bundling or mirroring).
    /// Not needed for local verification; required before any downloader or installer ships it.
    #[serde(default)]
    pub redistribution_approved: bool,
    #[serde(default)]
    pub commercial_use_permitted: bool,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        self.validate_with_ext(".safetensors")
    }

    pub fn validate_onnx(&self) -> Result<()> {
        self.validate_with_ext(".onnx")
    }

    fn validate_with_ext(&self, ext: &str) -> Result<()> {
        ensure!(
            self.schema_version == 2,
            "Unsupported manifest schema {} (version 2 separates local-use and redistribution approval)",
            self.schema_version
        );
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 100
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Invalid model ID"
        );
        ensure!(
            !self.filename.is_empty()
                && !self.filename.contains(['/', '\\', ':'])
                && self.filename.ends_with(ext)
                && self.filename.len() <= 160,
            "Only flat {ext} filenames are permitted"
        );
        ensure!(
            self.sha256.len() == 64 && self.sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "Invalid SHA-256"
        );
        ensure!(
            self.size_bytes > 0 && self.size_bytes <= MAX_MODEL_BYTES,
            "Invalid model size"
        );
        ensure!(
            self.source_url.starts_with("https://") && self.source_url.len() <= 2048,
            "Model source must use HTTPS"
        );
        ensure!(
            !self.code_license.trim().is_empty() && !self.weights_license.trim().is_empty(),
            "Separate code and weights license records required"
        );
        ensure!(
            self.local_use_approved,
            "Weights have not been approved for local use"
        );
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct VerifiedModel {
    pub id: String,
    pub sha256: String,
    pub tensors: usize,
    pub efficacy_verified: bool,
}
/// A checksum proves consistency with this manifest, not its authenticity.
/// Only install with manifests reviewed independently of the model distributor.
pub fn verify(path: &Path, manifest: &Manifest) -> Result<VerifiedModel> {
    manifest.validate()?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Model must be a regular non-symlink file"
    );
    ensure!(meta.len() == manifest.size_bytes, "Model size mismatch");
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(manifest.size_bytes + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == manifest.size_bytes,
        "Model changed or size mismatch"
    );
    let hash = format!("{:x}", Sha256::digest(&bytes));
    ensure!(
        hash.eq_ignore_ascii_case(&manifest.sha256),
        "Model integrity check failed"
    );
    let model =
        safetensors::SafeTensors::deserialize(&bytes).context("Invalid safetensors model")?;
    ensure!(!model.is_empty(), "Model contains no tensors");
    for (_, tensor) in model.tensors() {
        ensure!(tensor.shape().len() <= 8, "Tensor rank exceeds limit");
    }
    Ok(VerifiedModel {
        id: manifest.id.clone(),
        sha256: hash,
        tensors: model.len(),
        efficacy_verified: false,
    })
}

/// Verifies an ONNX model file against a schema-2 manifest before loading it in `tract-onnx`.
pub fn verify_onnx(path: &Path, manifest: &Manifest) -> Result<VerifiedModel> {
    manifest.validate_onnx()?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Model must be a regular non-symlink file"
    );
    ensure!(meta.len() == manifest.size_bytes, "Model size mismatch");
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(manifest.size_bytes + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == manifest.size_bytes,
        "Model changed or size mismatch"
    );
    let hash = format!("{:x}", Sha256::digest(&bytes));
    ensure!(
        hash.eq_ignore_ascii_case(&manifest.sha256),
        "Model integrity check failed"
    );
    Ok(VerifiedModel {
        id: manifest.id.clone(),
        sha256: hash,
        tensors: 0,
        efficacy_verified: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        Manifest {
            schema_version: 2,
            id: "example".into(),
            filename: "weights.safetensors".into(),
            sha256: "a".repeat(64),
            size_bytes: 128,
            source_url: "https://example.org/weights".into(),
            code_license: "MIT".into(),
            weights_license: "MIT".into(),
            local_use_approved: true,
            redistribution_approved: false,
            commercial_use_permitted: false,
        }
    }
    #[test]
    fn traversal_and_license_gate() {
        for name in [
            "../a.safetensors",
            "C:\\a.safetensors",
            "a.pkl",
            "/a.safetensors",
        ] {
            let mut m = manifest();
            m.filename = name.into();
            assert!(m.validate().is_err());
        }
        let mut m = manifest();
        assert!(
            m.validate().is_ok(),
            "redistribution is not needed for local use"
        );
        m.local_use_approved = false;
        assert!(m.validate().is_err());
        m.local_use_approved = true;
        m.schema_version = 1;
        assert!(
            m.validate().is_err(),
            "v1 manifests conflated approvals and are refused"
        );
    }
    #[test]
    fn integrity_before_parse() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("weights");
        std::fs::write(&p, [0; 128]).unwrap();
        assert!(
            verify(&p, &manifest())
                .unwrap_err()
                .to_string()
                .contains("integrity")
        );
    }
}
