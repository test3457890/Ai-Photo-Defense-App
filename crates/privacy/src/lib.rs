//! Local Privacy Scan. Combines independent detectors and reports what each one could and
//! could not examine. Automated scanning is never a guarantee of complete privacy.
//!
//! Detectors available without any model download:
//! - container metadata (EXIF/GPS, XMP, IPTC, ICC, comments, thumbnails, trailing data)
//! - QR, Data Matrix, PDF417, Aztec and 1-D barcodes (rxing, decoded locally, never opened)
//! - sensitive text and credential rules over decoded codes, metadata text and supplied text
//!
//! Face, person, object, document-layout and OCR detectors need verified local models; until
//! one is installed they are reported as unavailable and nothing is claimed for them.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[cfg(feature = "c2pa")]
pub mod c2pa_cred;
pub mod codes;
#[cfg(feature = "faces")]
pub mod faces;
pub mod forensics;
pub mod metadata;
pub mod profiles;
pub mod text;

pub use profiles::Profile;
pub use text::{Country, TextSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Category {
    // Identity (model-based detectors)
    Face,
    Person,
    Reflection,
    NameTag,
    Tattoo,
    // Objects and documents (model-based detectors)
    Screen,
    ComputerEquipment,
    Phone,
    SecurityCamera,
    NetworkEquipment,
    Document,
    IdentityDocument,
    PaymentCard,
    Whiteboard,
    HandwrittenNote,
    Vehicle,
    LicensePlate,
    Aircraft,
    Logo,
    Map,
    StreetSign,
    // Personal information (text)
    FullName,
    HomeAddress,
    Email,
    PhoneNumber,
    DateOfBirth,
    NationalId,
    PassportNumber,
    DriversLicense,
    IdentificationNumber,
    FinancialAccount,
    CreditCard,
    InsuranceInformation,
    MedicalIdentifier,
    Username,
    Signature,
    EmergencyContact,
    AircraftRegistration,
    // Cybersecurity (text)
    Password,
    ApiKey,
    AccessToken,
    PrivateKey,
    RecoveryCode,
    SessionIdentifier,
    ConnectionString,
    Credential,
    IpAddress,
    MacAddress,
    WifiCredentials,
    InternalHostname,
    InternalUrl,
    Url,
    FilePath,
    AuthenticationCode,
    // Machine-readable
    QrCode,
    Barcode,
    PaymentCode,
    // Location
    GpsLocation,
    // File-level metadata and structure
    ExifMetadata,
    XmpMetadata,
    IptcMetadata,
    IccProfile,
    TextMetadata,
    Comment,
    DeviceInformation,
    Timestamp,
    EmbeddedThumbnail,
    EmbeddedImage,
    HiddenLayers,
    OtherMetadata,
    UnexpectedData,
    UnexpectedTransparency,
    Manual,
}
impl Category {
    /// Which detector family is responsible for a category.
    pub fn detector(self) -> Detector {
        use Category::*;
        match self {
            Face | Person | Reflection | NameTag | Tattoo => Detector::FaceAndPerson,
            Screen | ComputerEquipment | Phone | SecurityCamera | NetworkEquipment | Document
            | IdentityDocument | PaymentCard | Whiteboard | HandwrittenNote | Vehicle
            | LicensePlate | Aircraft | Logo | Map | StreetSign => Detector::Objects,
            QrCode | Barcode | PaymentCode => Detector::MachineReadable,
            ExifMetadata
            | XmpMetadata
            | IptcMetadata
            | IccProfile
            | TextMetadata
            | Comment
            | DeviceInformation
            | Timestamp
            | EmbeddedThumbnail
            | EmbeddedImage
            | HiddenLayers
            | OtherMetadata
            | UnexpectedData
            | UnexpectedTransparency => Detector::Metadata,
            GpsLocation => Detector::Metadata,
            Manual => Detector::Manual,
            _ => Detector::Text,
        }
    }
    pub fn is_credential(self) -> bool {
        use Category::*;
        matches!(
            self,
            Password
                | ApiKey
                | AccessToken
                | PrivateKey
                | RecoveryCode
                | SessionIdentifier
                | ConnectionString
                | Credential
                | WifiCredentials
                | AuthenticationCode
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Detector {
    Metadata,
    MachineReadable,
    /// Pattern rules over text that has been made available (codes, metadata, OCR when installed).
    Text,
    /// Requires an installed OCR model to read text from pixels.
    Ocr,
    FaceAndPerson,
    Objects,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Evidence {
    /// Structural or checksum validation passed.
    Confirmed,
    /// A distinctive format matched.
    Pattern,
    /// A keyword with a plausible value; uncertain.
    Contextual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Metadata,
    Barcode,
    Ocr,
    Text,
    Model,
    Manual,
    Pixels,
}

/// One privacy finding. It never stores the sensitive value, only its category, location,
/// evidence level and length, so findings are safe to log or include in ordinary reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub category: Category,
    pub evidence: Evidence,
    pub source: Source,
    pub region: Option<Rect>,
    pub description: String,
    pub value_length: Option<usize>,
}
impl Finding {
    pub fn new(
        category: Category,
        evidence: Evidence,
        source: Source,
        region: Option<Rect>,
        description: String,
        value_length: Option<usize>,
    ) -> Self {
        Self {
            category,
            evidence,
            source,
            region,
            description,
            value_length,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorStatus {
    pub detector: Detector,
    pub available: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub schema_version: u32,
    pub profile: String,
    pub container: String,
    pub findings: Vec<Finding>,
    pub detectors: Vec<DetectorStatus>,
    /// Profile categories that no available detector can examine.
    pub not_examined: Vec<Category>,
    pub limitations: Vec<String>,
}
impl ScanReport {
    pub fn count_by_category(&self) -> Vec<(Category, usize)> {
        let mut m = std::collections::BTreeMap::new();
        for f in &self.findings {
            *m.entry(f.category).or_insert(0) += 1;
        }
        m.into_iter().collect()
    }
}

/// Inputs for a scan. `ocr_text` is supplied by an installed OCR engine, if any.
pub struct ScanInput<'a> {
    pub bytes: &'a [u8],
    pub pixels: &'a image::DynamicImage,
    pub ocr_text: Option<Vec<TextSpan>>,
    pub extra_text: Vec<TextSpan>,
}

pub fn scan(input: ScanInput<'_>, profile: &Profile) -> ScanReport {
    scan_with_faces(input, profile, None)
}

pub fn scan_with_faces(
    input: ScanInput<'_>,
    profile: &Profile,
    face_regions: Option<&[Rect]>,
) -> ScanReport {
    let mut findings = vec![];
    let mut spans = input.extra_text;
    let mut container = String::new();
    let has_faces = face_regions.is_some();
    if let Some(regions) = face_regions {
        for &r in regions {
            findings.push(Finding::new(
                Category::Face,
                Evidence::Contextual,
                Source::Model,
                Some(r),
                "Detected face".into(),
                None,
            ));
        }
    }
    if profile.wants(Detector::Metadata) {
        let m = metadata::inspect(input.bytes);
        container = m.container.clone();
        findings.extend(m.findings);
        spans.extend(m.text_values.into_iter().map(|t| TextSpan {
            text: t,
            region: None,
            source: Source::Metadata,
        }));
    }
    let mut code_note = "Not requested by profile".to_string();
    if profile.wants(Detector::MachineReadable) || profile.wants(Detector::Text) {
        match codes::detect(input.pixels) {
            Ok(found) => {
                code_note = format!("Scanned {} tile(s) at full resolution", found.tiles);
                for c in found.codes {
                    findings.push(c.finding);
                    spans.push(TextSpan {
                        text: c.payload,
                        region: c.region,
                        source: Source::Barcode,
                    });
                }
            }
            Err(e) => code_note = format!("Code detection failed: {e}"),
        }
    }
    let has_ocr = input.ocr_text.is_some();
    if let Some(o) = input.ocr_text {
        spans.extend(o);
    }
    if profile.wants(Detector::Text) {
        findings.extend(text::scan(&spans, &profile.countries));
    }
    findings.retain(|f| profile.includes(f));
    // A decoded code and the text rule over its payload describe the same object once.
    let mut seen = std::collections::HashSet::new();
    findings.retain(|f| {
        f.region.is_none()
            || seen.insert((f.category, f.region.map(|r| (r.x, r.y, r.width, r.height))))
    });
    let detectors = vec![
        DetectorStatus { detector: Detector::Metadata, available: true, note: "Container structure and EXIF fields".into() },
        DetectorStatus { detector: Detector::MachineReadable, available: true, note: code_note },
        DetectorStatus { detector: Detector::Text, available: true, note: "Pattern rules over decoded codes, metadata text and OCR output".into() },
        DetectorStatus {
            detector: Detector::Ocr,
            available: has_ocr,
            note: if has_ocr { "OCR text supplied".into() } else { "No OCR model installed: text visible in pixels was NOT read".into() },
        },
        DetectorStatus {
            detector: Detector::FaceAndPerson,
            available: has_faces,
            note: if has_faces {
                "Local UltraFace ONNX model ran (faces only; person/reflection/name-tag/tattoo not examined)".into()
            } else {
                "No verified face/person model installed: faces and people were NOT detected".into()
            },
        },
        DetectorStatus { detector: Detector::Objects, available: false, note: "No verified object/document model installed: screens, documents, plates and objects were NOT detected".into() },
    ];
    let available: BTreeSet<Detector> = detectors
        .iter()
        .filter(|d| d.available)
        .map(|d| d.detector)
        .collect();
    let not_examined: Vec<Category> = profile
        .categories
        .iter()
        .copied()
        .filter(|c| {
            let d = c.detector();
            if d == Detector::FaceAndPerson {
                return !(has_faces && *c == Category::Face);
            }
            // Text categories visible only in pixels need OCR; codes and metadata alone are partial.
            !(available.contains(&d) && (d != Detector::Text || has_ocr)) && d != Detector::Manual
        })
        .collect();
    let mut limitations = vec![
        "Automated scanning cannot guarantee that a photograph contains no private information."
            .to_string(),
        "Review the photograph visually before sharing.".to_string(),
    ];
    if !has_ocr {
        limitations.push("Text in the image pixels was only examined inside decoded QR/barcodes; install an OCR model to read visible text.".into());
    }
    ScanReport {
        schema_version: 1,
        profile: profile.name.clone(),
        container,
        findings,
        detectors,
        not_examined,
        limitations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_reports_unavailable_detectors_honestly() {
        let img = image::DynamicImage::new_rgb8(32, 32);
        let mut c = std::io::Cursor::new(vec![]);
        img.write_to(&mut c, image::ImageFormat::Png).unwrap();
        let bytes = c.into_inner();
        let r = scan(
            ScanInput {
                bytes: &bytes,
                pixels: &img,
                ocr_text: None,
                extra_text: vec![],
            },
            &Profile::builtin("Social Media").unwrap(),
        );
        assert!(r.findings.is_empty());
        assert!(r.not_examined.contains(&Category::Face));
        assert!(
            r.detectors
                .iter()
                .any(|d| d.detector == Detector::FaceAndPerson && !d.available)
        );
    }
}

#[cfg(test)]
mod profile_code_tests {
    use super::*;
    use rxing::Writer;
    #[test]
    fn located_codes_reported_by_any_profile_that_wants_codes() {
        let m = rxing::MultiFormatWriter
            .encode(
                "WIFI:T:WPA;S:Example;P:example-pass;;",
                &rxing::BarcodeFormat::QR_CODE,
                200,
                200,
            )
            .unwrap();
        let img = image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(
            m.getWidth(),
            m.getHeight(),
            |x, y| image::Luma([if m.get(x, y) { 0 } else { 255 }]),
        ));
        for name in ["Social Media", "Home Privacy", "Maximum Privacy"] {
            let r = scan(
                ScanInput {
                    bytes: &[],
                    pixels: &img,
                    ocr_text: None,
                    extra_text: vec![],
                },
                &Profile::builtin(name).unwrap(),
            );
            assert_eq!(
                r.findings.iter().filter(|f| f.region.is_some()).count(),
                1,
                "{name}: {:?}",
                r.findings
            );
        }
    }
}
