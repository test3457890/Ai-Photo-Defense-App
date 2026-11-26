//! Privacy profiles: which categories to look for, how certain a finding must be, and the
//! preferred concealment. Profiles are plain JSON so users can import, edit and export them.
use crate::{Category, Country, Detector, Evidence, Finding};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PROFILE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionPreference {
    OpaqueBlack,
    OpaqueWhite,
    Blur,
    Pixelate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub name: String,
    pub description: String,
    pub categories: BTreeSet<Category>,
    pub countries: Vec<Country>,
    /// Least certain evidence level to report (Contextual reports everything).
    pub min_evidence: Evidence,
    pub redaction: RedactionPreference,
}

pub const BUILTIN: [&str; 12] = [
    "Social Media",
    "Cybersecurity",
    "Corporate",
    "Aviation",
    "Public Safety",
    "Education",
    "Healthcare",
    "Financial",
    "Home Privacy",
    "Journalism",
    "Travel",
    "Maximum Privacy",
];

const METADATA: &[Category] = &[
    Category::GpsLocation,
    Category::ExifMetadata,
    Category::XmpMetadata,
    Category::IptcMetadata,
    Category::IccProfile,
    Category::TextMetadata,
    Category::Comment,
    Category::DeviceInformation,
    Category::Timestamp,
    Category::EmbeddedThumbnail,
    Category::EmbeddedImage,
    Category::HiddenLayers,
    Category::OtherMetadata,
    Category::UnexpectedData,
    Category::UnexpectedTransparency,
];

impl Profile {
    pub fn builtin(name: &str) -> Option<Self> {
        use Category::*;
        let (description, cats): (&str, Vec<Category>) = match name {
            "Social Media" => (
                "Faces, people, plates, addresses, notifications and location",
                vec![
                    Face,
                    Person,
                    Reflection,
                    LicensePlate,
                    Vehicle,
                    HomeAddress,
                    StreetSign,
                    Screen,
                    Phone,
                    QrCode,
                    Email,
                    PhoneNumber,
                    FullName,
                ],
            ),
            "Cybersecurity" => (
                "Credentials, network details, screens, code and technical documents",
                vec![
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
                    Username,
                    Screen,
                    ComputerEquipment,
                    NetworkEquipment,
                    SecurityCamera,
                    Whiteboard,
                    Document,
                    QrCode,
                    Barcode,
                    Email,
                ],
            ),
            "Corporate" => (
                "Internal documents, employee information, equipment, whiteboards",
                vec![
                    Document,
                    Whiteboard,
                    Screen,
                    ComputerEquipment,
                    Logo,
                    NameTag,
                    IdentificationNumber,
                    FullName,
                    Email,
                    PhoneNumber,
                    InternalHostname,
                    InternalUrl,
                    Username,
                    Password,
                    ApiKey,
                    AccessToken,
                    QrCode,
                    Barcode,
                    Face,
                    Person,
                ],
            ),
            "Aviation" => (
                "Aircraft registrations, crew identification, operational documents",
                vec![
                    AircraftRegistration,
                    Aircraft,
                    NameTag,
                    IdentificationNumber,
                    IdentityDocument,
                    Document,
                    Screen,
                    Map,
                    Face,
                    Person,
                    Barcode,
                    QrCode,
                    FullName,
                ],
            ),
            "Public Safety" => (
                "Faces, personnel identification, plates, maps, communication equipment",
                vec![
                    Face,
                    Person,
                    NameTag,
                    IdentificationNumber,
                    LicensePlate,
                    Vehicle,
                    Map,
                    Screen,
                    NetworkEquipment,
                    Phone,
                    Document,
                    StreetSign,
                    HomeAddress,
                    FullName,
                    PhoneNumber,
                ],
            ),
            "Education" => (
                "Student identification, records, classroom documents, screens",
                vec![
                    IdentificationNumber,
                    FullName,
                    Document,
                    Screen,
                    Whiteboard,
                    Face,
                    Person,
                    NameTag,
                    Email,
                    DateOfBirth,
                    Barcode,
                    QrCode,
                ],
            ),
            "Healthcare" => (
                "Patient information, prescriptions, records, bracelets, displays",
                vec![
                    MedicalIdentifier,
                    InsuranceInformation,
                    FullName,
                    DateOfBirth,
                    Document,
                    Screen,
                    Barcode,
                    QrCode,
                    Face,
                    Person,
                    NameTag,
                    PhoneNumber,
                    HomeAddress,
                ],
            ),
            "Financial" => (
                "Statements, account numbers, cards, tax documents",
                vec![
                    CreditCard,
                    PaymentCard,
                    FinancialAccount,
                    NationalId,
                    Document,
                    PaymentCode,
                    FullName,
                    HomeAddress,
                    Signature,
                    Barcode,
                    QrCode,
                ],
            ),
            "Home Privacy" => (
                "Addresses, family photographs, personal documents, electronics, location",
                vec![
                    HomeAddress,
                    StreetSign,
                    Document,
                    Screen,
                    ComputerEquipment,
                    Phone,
                    Face,
                    Person,
                    Reflection,
                    WifiCredentials,
                    QrCode,
                    Barcode,
                    FullName,
                    PhoneNumber,
                    Email,
                ],
            ),
            "Journalism" => (
                "Source identities, identifying surroundings, documents, reflections",
                vec![
                    Face,
                    Person,
                    Reflection,
                    Tattoo,
                    NameTag,
                    Document,
                    Screen,
                    StreetSign,
                    LicensePlate,
                    Vehicle,
                    Logo,
                    Map,
                    HomeAddress,
                    FullName,
                    PhoneNumber,
                    Email,
                    QrCode,
                    Barcode,
                ],
            ),
            "Travel" => (
                "Passports, boarding passes, room numbers, tickets and itineraries",
                vec![
                    PassportNumber,
                    IdentityDocument,
                    Document,
                    Barcode,
                    QrCode,
                    HomeAddress,
                    FullName,
                    DateOfBirth,
                    NationalId,
                    LicensePlate,
                    Face,
                ],
            ),
            "Maximum Privacy" => ("Every available detection category", all_categories()),
            _ => return None,
        };
        let mut categories: BTreeSet<Category> = cats.into_iter().collect();
        categories.extend(METADATA.iter().copied());
        Some(Self {
            schema_version: 1,
            name: name.into(),
            description: description.into(),
            categories,
            countries: Country::ALL.to_vec(),
            min_evidence: Evidence::Contextual,
            redaction: RedactionPreference::OpaqueBlack,
        })
    }
    pub fn all() -> Vec<Self> {
        BUILTIN.iter().filter_map(|n| Self::builtin(n)).collect()
    }
    pub fn wants(&self, d: Detector) -> bool {
        self.categories.iter().any(|c| c.detector() == d)
    }
    /// A located machine-readable code is always reported when the profile looks for codes,
    /// whatever its payload was classified as.
    pub fn includes(&self, f: &Finding) -> bool {
        let code = f.source == crate::Source::Barcode
            && (self.categories.contains(&Category::QrCode)
                || self.categories.contains(&Category::Barcode));
        (self.categories.contains(&f.category) || code) && f.evidence <= self.min_evidence
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_PROFILE_BYTES, "Profile file too large");
        let p: Self = serde_json::from_slice(bytes)?;
        p.validate()?;
        Ok(p)
    }
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "Unknown profile schema");
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 80,
            "Profile name must be 1..80 characters"
        );
        ensure!(
            self.description.len() <= 500,
            "Profile description too long"
        );
        ensure!(
            !self.categories.is_empty(),
            "Profile needs at least one category"
        );
        Ok(())
    }
}

fn all_categories() -> Vec<Category> {
    // Round-trips every variant through serde names so new categories are not forgotten.
    use Category::*;
    vec![
        Face,
        Person,
        Reflection,
        NameTag,
        Tattoo,
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
        QrCode,
        Barcode,
        PaymentCode,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_roundtrip_and_validate() {
        assert_eq!(Profile::all().len(), BUILTIN.len());
        for p in Profile::all() {
            let back = Profile::from_json(&p.to_json().unwrap()).unwrap();
            assert_eq!(back, p);
            assert!(p.categories.contains(&Category::GpsLocation));
        }
        assert!(Profile::from_json(br#"{"schema_version":1,"name":"x","description":"","categories":[],"countries":[],"min_evidence":"Pattern","redaction":"Blur"}"#).is_err());
        assert!(Profile::from_json(br#"{"schema_version":1,"name":"x","description":"","categories":["Email"],"countries":[],"min_evidence":"Pattern","redaction":"Blur","run":"rm -rf"}"#).is_err());
        assert!(Profile::from_json(&vec![b' '; MAX_PROFILE_BYTES + 1]).is_err());
    }
    #[test]
    fn evidence_threshold() {
        let mut p = Profile::builtin("Cybersecurity").unwrap();
        p.min_evidence = Evidence::Pattern;
        let f = |e| {
            Finding::new(
                Category::Password,
                e,
                crate::Source::Text,
                None,
                String::new(),
                None,
            )
        };
        assert!(p.includes(&f(Evidence::Confirmed)) && p.includes(&f(Evidence::Pattern)));
        assert!(!p.includes(&f(Evidence::Contextual)));
    }
}
