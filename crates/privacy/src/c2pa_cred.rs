//! C2PA Content Credentials verification and optional export signing (`c2pa` crate,
//! `MIT OR Apache-2.0`, `rust_native_crypto` + `file_io`, zero network access).

use anyhow::{Context, Result};
use c2pa::{Builder, Reader, SigningAlg, ValidationState, create_signer};
use serde::Serialize;
use std::io::Cursor;

#[derive(Debug, Clone, Serialize)]
pub struct C2paSummary {
    pub present: bool,
    pub validation_state: String,
    pub claim_generator: Option<String>,
    pub issuer: Option<String>,
    pub actions: Vec<String>,
}

/// Infers MIME type from magic bytes (never from file extension).
pub fn mime_from_bytes(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        Some("image/tiff")
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" {
        Some("video/mp4")
    } else {
        None
    }
}

/// Cryptographically inspects and validates any embedded C2PA manifest in `bytes`.
pub fn inspect_bytes(bytes: &[u8]) -> C2paSummary {
    let Some(mime) = mime_from_bytes(bytes) else {
        return C2paSummary {
            present: false,
            validation_state: "None".into(),
            claim_generator: None,
            issuer: None,
            actions: vec![],
        };
    };
    let Ok(reader) =
        Reader::from_context(c2pa::Context::default()).with_stream(mime, Cursor::new(bytes))
    else {
        return C2paSummary {
            present: false,
            validation_state: "None".into(),
            claim_generator: None,
            issuer: None,
            actions: vec![],
        };
    };
    let state = format!("{:?}", reader.validation_state());
    let (claim_generator, issuer, actions) = if let Some(m) = reader.active_manifest() {
        let cg = m
            .claim_generator_info
            .as_deref()
            .and_then(|v| v.first())
            .map(|g| g.name.clone())
            .or_else(|| m.claim_generator().map(str::to_string));
        let iss = m.signature_info().and_then(|s| s.issuer.clone());
        let acts: Vec<String> = m
            .assertions()
            .iter()
            .map(|a| a.label().to_string())
            .collect();
        (cg, iss, acts)
    } else {
        (None, None, vec![])
    };
    C2paSummary {
        present: true,
        validation_state: state,
        claim_generator,
        issuer,
        actions,
    }
}

/// Human-readable C2PA status for `forensics::ForensicReport::c2pa`.
pub fn inspect_status(bytes: &[u8]) -> String {
    let s = inspect_bytes(bytes);
    if !s.present {
        return "No C2PA manifest found".into();
    }
    let gen_str = s.claim_generator.as_deref().unwrap_or("unknown");
    let iss_str = s.issuer.as_deref().unwrap_or("untrusted/unknown");
    if s.validation_state == format!("{:?}", ValidationState::Invalid) {
        format!(
            "C2PA manifest INVALID (cryptographic check failed; generator: {gen_str}, issuer: {iss_str})"
        )
    } else {
        format!(
            "C2PA manifest verified ({}; generator: {gen_str}, issuer: {iss_str})",
            s.validation_state
        )
    }
}

/// Signs `bytes` (format inferred from magic bytes) with a user-supplied PEM certificate chain and
/// private key, recording `c2pa.actions` without any private details.
pub fn sign_bytes(
    bytes: &[u8],
    cert_pem: &[u8],
    key_pem: &[u8],
    actions: &[&str],
) -> Result<Vec<u8>> {
    let mime = mime_from_bytes(bytes).context("Unsupported format for C2PA signing")?;
    let mut action_objs = Vec::with_capacity(actions.len() + 1);
    if !matches!(
        actions.first().copied(),
        Some("c2pa.created" | "c2pa.opened")
    ) {
        action_objs.push(serde_json::json!({
            "action": "c2pa.created",
            "digitalSourceType": "http://cv.iptc.org/newscodes/digitalsourcetype/digitalCapture"
        }));
    }
    action_objs.extend(actions.iter().map(|a| serde_json::json!({ "action": a })));
    let manifest_json = serde_json::json!({
        "claim_generator_info": [{
            "name": "AI Photo Defense",
            "version": env!("CARGO_PKG_VERSION"),
        }],
        "assertions": [{
            "label": "c2pa.actions",
            "data": {
                "actions": action_objs
            }
        }]
    })
    .to_string();

    let mut builder = Builder::from_context(c2pa::Context::default())
        .with_definition(&manifest_json)
        .context("Invalid C2PA manifest JSON")?;
    let signer = [
        SigningAlg::Es256,
        SigningAlg::Es384,
        SigningAlg::Ed25519,
        SigningAlg::Ps256,
    ]
    .into_iter()
    .find_map(|alg| create_signer::from_keys(cert_pem, key_pem, alg, None).ok())
    .context("Failed to initialize C2PA signer from PEM certificate and key")?;

    let mut out = Cursor::new(Vec::new());
    builder
        .sign(signer.as_ref(), mime, &mut Cursor::new(bytes), &mut out)
        .context("C2PA signing failed")?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_with_untrusted_self_signed_cert() {
        // Generate an ephemeral self-signed CA + ES256 end-entity chain in the test only (untrusted).
        let mut ca_params = rcgen::CertificateParams::new(vec!["Test Root CA".into()]).unwrap();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        let ca_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();
        let ca_issuer = rcgen::Issuer::new(ca_params, ca_key);

        let mut ee_params = rcgen::CertificateParams::new(vec!["test.local".into()]).unwrap();
        ee_params.is_ca = rcgen::IsCa::NoCa;
        ee_params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
        ee_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::EmailProtection];
        ee_params.use_authority_key_identifier_extension = true;
        let ee_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let ee_cert = ee_params.signed_by(&ee_key, &ca_issuer).unwrap();

        let cert_pem = format!("{}\n{}", ee_cert.pem(), ca_cert.pem());
        let key_pem = ee_key.serialize_pem();

        // Create a minimal valid JPEG in memory.
        let img = image::DynamicImage::new_rgb8(16, 16);
        let mut jpeg = Cursor::new(Vec::new());
        img.write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
        let raw_jpeg = jpeg.into_inner();

        assert!(!inspect_bytes(&raw_jpeg).present);

        let signed = sign_bytes(
            &raw_jpeg,
            cert_pem.as_bytes(),
            key_pem.as_bytes(),
            &["c2pa.redacted", "c2pa.metadata_removed"],
        )
        .unwrap();

        let summary = inspect_bytes(&signed);
        assert!(summary.present);
        assert_ne!(summary.validation_state, "Invalid");
        assert!(
            summary
                .actions
                .iter()
                .any(|a| a.starts_with("c2pa.actions"))
        );

        let status = inspect_status(&signed);
        assert!(status.contains("C2PA manifest verified"));
    }
}
