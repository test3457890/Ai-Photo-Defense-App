//! Offline, fail-closed model verification. No remote code execution or pickle support.
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
    pub redistribution_approved: bool,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "Unknown manifest schema");
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
                && self.filename.ends_with(".safetensors")
                && self.filename.len() <= 160,
            "Only flat .safetensors filenames are permitted"
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
            self.redistribution_approved,
            "Weights have not been approved for installation"
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
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            id: "example".into(),
            filename: "weights.safetensors".into(),
            sha256: "a".repeat(64),
            size_bytes: 128,
            source_url: "https://example.org/weights".into(),
            code_license: "MIT".into(),
            weights_license: "MIT".into(),
            redistribution_approved: true,
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
        m.redistribution_approved = false;
        assert!(m.validate().is_err());
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
