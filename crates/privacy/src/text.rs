//! Sensitive-text and credential detection over untrusted strings (OCR lines, decoded codes,
//! metadata text). Text is treated as data only: nothing is executed, opened or verified online.
//!
//! Evidence levels: Confirmed = structural/checksum validated (Luhn, IBAN mod-97, ICAO MRZ
//! check digits, address parsing); Pattern = distinctive format match; Contextual = a keyword
//! plus a plausible value, which is uncertain.
use crate::{Category, Evidence, Finding, Rect, Source};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Longest text a single scan call will examine.
pub const MAX_TEXT_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Country {
    Us,
    Gb,
    Ca,
}
impl Country {
    pub const ALL: [Self; 3] = [Self::Us, Self::Gb, Self::Ca];
}

struct Rule {
    category: Category,
    evidence: Evidence,
    regex: Regex,
    country: Option<Country>,
    check: Option<fn(&str) -> Option<Evidence>>,
    description: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        use Category::*;
        use Evidence::*;
        let r = |category, evidence, pattern: &str, description| Rule {
            category,
            evidence,
            regex: Regex::new(pattern).expect("static regex"),
            country: None,
            check: None,
            description,
        };
        let generic = vec![
            r(AuthenticationCode, Confirmed, r"(?i)\botpauth(?:-migration)?://\S+", "One-time-password enrollment secret"),
            r(WifiCredentials, Confirmed, r"(?i)\bWIFI:(?:[^;]*;){1,6}", "Wi-Fi network credentials"),
            r(PrivateKey, Confirmed, r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----", "Private key block"),
            r(ApiKey, Pattern, r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", "AWS access key ID"),
            r(ApiKey, Contextual, r"(?i)aws_secret_access_key\s*[=:]\s*[A-Za-z0-9/+=]{40}", "AWS secret access key"),
            r(AccessToken, Pattern, r"\bgh[pousr]_[A-Za-z0-9]{36,255}\b", "GitHub token"),
            r(AccessToken, Pattern, r"\bgithub_pat_[A-Za-z0-9_]{22,255}\b", "GitHub fine-grained token"),
            r(AccessToken, Pattern, r"\bglpat-[A-Za-z0-9_-]{20,}\b", "GitLab token"),
            r(AccessToken, Pattern, r"\bxox[abposr]-[A-Za-z0-9-]{10,200}\b", "Slack token"),
            r(ApiKey, Pattern, r"\bAIza[0-9A-Za-z_-]{35}\b", "Google API key"),
            r(ApiKey, Pattern, r"\b[sr]k_(?:live|test)_[0-9A-Za-z]{16,247}\b", "Stripe key"),
            r(ApiKey, Pattern, r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{32,}\b", "AI service API key"),
            r(AccessToken, Pattern, r"\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b", "JSON Web Token"),
            r(ConnectionString, Pattern, r"(?i)\b(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|redis|rediss|amqps?|mssql|sqlserver|ftp|sftp)://[^\s:/@]+:[^\s@]+@[^\s]+", "Connection string with embedded password"),
            r(ConnectionString, Pattern, r"(?i)(?:server|data source|host)=[^;\n]+;[^\n]*(?:password|pwd)=[^;\n]+", "Connection string with password"),
            r(Password, Contextual, r#"(?i)\b(?:password|passwd|passwort|pwd|passcode|pass|pin|secret)\b\s*[:=]\s*[^\s"']{3,}"#, "Password assignment"),
            r(Credential, Pattern, r"(?m)^\s*(?:export\s+|set\s+)?[A-Z][A-Z0-9_]*(?:KEY|SECRET|TOKEN|PASSWORD|PASSWD|CREDENTIALS?)[A-Z0-9_]*\s*=\s*\S{4,}", "Secret environment variable"),
            r(SessionIdentifier, Pattern, r"(?i)\b(?:session[_-]?id|sid|phpsessid|jsessionid|asp\.net_sessionid|sessionToken)\b\s*[:=]\s*[A-Za-z0-9._%-]{16,}", "Session identifier"),
            r(RecoveryCode, Contextual, r"(?i)\b(?:recovery|backup|reset)\s+codes?\b", "Recovery or backup codes"),
            r(WifiCredentials, Contextual, r"(?i)\b(?:ssid|wi-?fi(?:\s+(?:name|network))?|network name)\b\s*[:=]\s*\S+", "Wi-Fi network name"),
            r(Email, Pattern, r"(?i)\b[a-z0-9._%+-]{1,64}@[a-z0-9-]{1,63}(?:\.[a-z0-9-]{1,63})*\.[a-z]{2,24}\b", "Email address"),
            r(Url, Pattern, r#"(?i)\b(?:https?|ftp|file)://[^\s<>"']{1,2048}"#, "URL"),
            r(InternalHostname, Pattern, r"(?i)\b[a-z0-9-]{1,63}(?:\.[a-z0-9-]{1,63})*\.(?:local|internal|intranet|corp|lan|localdomain|home\.arpa)\b", "Internal hostname"),
            r(MacAddress, Pattern, r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b", "MAC address"),
            r(FilePath, Pattern, r"\b[A-Za-z]:\\(?:[^\\/:*?<>|\r\n\s]+\\)+[^\\/:*?<>|\r\n\s]*", "Windows file path"),
            r(FilePath, Pattern, r"(?:^|\s)/(?:home|Users|etc|var|opt|root|srv|mnt)/[^\s]+", "Unix file path"),
            r(GpsLocation, Pattern, r"[-+]?\b\d{1,2}\.\d{4,}\s*,\s*[-+]?\d{1,3}\.\d{4,}\b", "Decimal GPS coordinates"),
            r(GpsLocation, Pattern, "\\b\\d{1,3}\u{b0}\\s?\\d{1,2}['\u{2032}]\\s?\\d{1,2}(?:\\.\\d+)?[\"\u{2033}]?\\s?[NSEW]\\b", "DMS GPS coordinates"),
            r(DateOfBirth, Contextual, r"(?i)\b(?:dob|d\.o\.b\.|date of birth|birth ?date|born)\b\s*[:\-]?\s*\d{1,4}[/.\-]\d{1,2}[/.\-]\d{1,4}", "Date of birth"),
            r(PassportNumber, Contextual, r"(?i)\bpassport\s*(?:no\.?|number|num|#)?\s*[:#]?\s*[A-Z0-9]{6,9}\b", "Passport number"),
            r(DriversLicense, Contextual, r"(?i)\b(?:driver'?s?\s+licen[cs]e|licen[cs]e\s+(?:no|number|#)|\bDL\b)\s*[:#]?\s*[A-Z0-9-]{5,16}\b", "Driver's licence number"),
            r(IdentificationNumber, Contextual, r"(?i)\b(?:employee|staff|student|member|badge|personnel)\s*(?:id|no\.?|number|#)\s*[:#]?\s*[A-Z0-9-]{3,16}\b", "Employee, student or member ID"),
            r(MedicalIdentifier, Contextual, r"(?i)\b(?:mrn|medical record(?: number)?|patient id|nhs (?:no|number)|health card)\s*[:#]?\s*[A-Z0-9-]{4,16}\b", "Medical record identifier"),
            r(InsuranceInformation, Contextual, r"(?i)\b(?:policy|group|subscriber|insurance)\s*(?:no\.?|number|id|#)\s*[:#]?\s*[A-Z0-9-]{4,20}\b", "Insurance identifier"),
            r(FinancialAccount, Contextual, r"(?i)\b(?:account|acct|a/c)\s*(?:no\.?|number|#)?\s*[:#]?\s*\d{6,17}\b", "Financial account number"),
            r(Username, Contextual, r"(?i)\b(?:user(?:name)?|login|user id|uid)\b\s*[:=]\s*[A-Za-z0-9._@-]{2,64}", "Username"),
            r(AircraftRegistration, Contextual, r"(?i)\b(?:tail|registration|reg\.?|aircraft)\s*(?:no\.?|number|#)?\s*[:#]?\s*N[1-9][0-9]{0,4}[A-HJ-NP-Z]{0,2}\b", "US aircraft registration"),
            r(AircraftRegistration, Pattern, r"\b(?:G|D|F|I|EC|PH|OO|OE|HB|VH|ZK|JA|HL|B|CS|SE|LN|OY|OH|EI|9H|VT|PK|A6|C|XA|PP|PR)-[A-Z]{3,4}\b", "Aircraft registration"),
            r(LicensePlate, Contextual, r"(?i)\b(?:licen[cs]e plate|plate (?:no|number)|registration plate|vrn)\b\s*[:#]?\s*[A-Z0-9 -]{4,10}\b", "Vehicle licence plate"),
            r(Signature, Contextual, r"(?i)\bsignature\s*:", "Signature field"),
            r(EmergencyContact, Contextual, r"(?i)\bemergency contact\b", "Emergency contact information"),
            r(FullName, Contextual, r"(?i)\b(?:name|full name|surname|given names?)\s*:\s*[A-Z][a-z'-]+(?:\s+[A-Z][a-z'-]+){0,3}", "Personal name field"),
            r(HomeAddress, Contextual, r"(?i)\b\d{1,6}\s+(?:[A-Z][a-z]+\s){1,4}(?:street|st|avenue|ave|road|rd|boulevard|blvd|lane|ln|drive|dr|court|ct|way|place|pl|terrace|close|crescent)\b\.?", "Street address"),
            r(HomeAddress, Contextual, r"(?i)\b(?:apt|apartment|suite|unit|room|flat)\s*#?\s*\d{1,5}[A-Z]?\b", "Apartment, room or unit number"),
            r(PhoneNumber, Pattern, r"(?:\+|\b)(?:\d{1,3}[\s.-]?)?\(?\d{3}\)?[\s.-]\d{3}[\s.-]\d{4}\b", "Telephone number"),
            r(PhoneNumber, Pattern, r"\+\d{1,3}(?:[\s.-]?\d{2,4}){2,5}\b", "International telephone number"),
        ];
        let mut v = Vec::new();
        let mut checked = |category, evidence, pattern: &str, check: fn(&str) -> Option<Evidence>, description, country| {
            v.push(Rule {
                category,
                evidence,
                regex: Regex::new(pattern).expect("static regex"),
                country,
                check: Some(check),
                description,
            })
        };
        checked(CreditCard, Confirmed, r"\b(?:\d[ -]?){12,18}\d\b", check_card, "Payment card number (Luhn valid)", None);
        checked(FinancialAccount, Confirmed, r"\b[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]{4}){2,7}(?:[ ]?[A-Z0-9]{1,4})?\b", check_iban, "IBAN (mod-97 valid)", None);
        checked(FinancialAccount, Confirmed, r"(?i)\b(?:routing|aba|rtn)\b[^0-9\n]{0,12}\d{9}\b", check_aba, "US routing number (checksum valid)", Some(Country::Us));
        checked(PassportNumber, Confirmed, r"P[A-Z<][A-Z<]{3}[A-Z<]{5,39}\s*\n?\s*[A-Z0-9<]{9}\d[A-Z<]{3}\d{6}\d[MFX<]\d{6}\d", check_mrz, "Passport machine-readable zone (check digits valid)", None);
        checked(IpAddress, Confirmed, r"\b(?:\d{1,3}\.){3}\d{1,3}\b", check_ipv4, "IPv4 address", None);
        checked(IpAddress, Confirmed, r"(?i)\b(?:[0-9a-f]{1,4}:){2,7}(?::?[0-9a-f]{1,4})?\b|(?i)\b[0-9a-f]{1,4}::(?:[0-9a-f]{1,4}:){0,5}[0-9a-f]{1,4}\b", check_ipv6, "IPv6 address", None);
        checked(NationalId, Pattern, r"\b(\d{3})-(\d{2})-(\d{4})\b", check_ssn, "US Social Security number", Some(Country::Us));
        checked(NationalId, Contextual, r"(?i)\b(?:ssn|social security(?: no\.?| number)?)\b\s*[:#]?\s*\d{9}\b", check_any, "US Social Security number (keyword)", Some(Country::Us));
        checked(NationalId, Pattern, r"(?i)\b[A-CEGHJ-PR-TW-Z][A-CEGHJ-NPR-TW-Z]\s?\d{2}\s?\d{2}\s?\d{2}\s?[A-D]\b", check_pattern, "UK National Insurance number", Some(Country::Gb));
        checked(NationalId, Confirmed, r"\b\d{3}[ -]\d{3}[ -]\d{3}\b", check_sin, "Canadian Social Insurance Number (Luhn valid)", Some(Country::Ca));
        // Generic rules after checksummed ones; the entropy heuristic always runs last.
        v.extend(generic);
        v.push(Rule {
            category: Credential,
            evidence: Contextual,
            regex: Regex::new(r"\b[A-Za-z0-9+/_=-]{32,}\b").expect("static regex"),
            country: None,
            check: Some(check_entropy),
            description: "High-entropy string (possible secret)",
        });
        v
    })
}

fn digits(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}
fn luhn(d: &[u32]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &x)| {
            if i % 2 == 1 {
                let y = x * 2;
                if y > 9 { y - 9 } else { y }
            } else {
                x
            }
        })
        .sum();
    !d.is_empty() && sum.is_multiple_of(10)
}
fn check_pattern(_: &str) -> Option<Evidence> {
    Some(Evidence::Pattern)
}
fn check_any(_: &str) -> Option<Evidence> {
    Some(Evidence::Contextual)
}
fn check_card(s: &str) -> Option<Evidence> {
    let d = digits(s);
    let prefix_ok = match d.first()? {
        4 => true,
        5 => matches!(d.get(1), Some(1..=5)),
        2 => matches!(d.get(1), Some(2..=7)),
        3 => matches!(d.get(1), Some(4 | 7 | 5 | 6 | 0 | 8)),
        6 => true,
        _ => false,
    };
    (matches!(d.len(), 13..=19) && prefix_ok && luhn(&d)).then_some(Evidence::Confirmed)
}
fn check_sin(s: &str) -> Option<Evidence> {
    let d = digits(s);
    (d.len() == 9 && d[0] != 8 && luhn(&d)).then_some(Evidence::Confirmed)
}
fn check_iban(s: &str) -> Option<Evidence> {
    let c: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !(15..=34).contains(&c.len()) {
        return None;
    }
    let rearranged = format!("{}{}", &c[4..], &c[..4]);
    let mut rem = 0u32;
    for ch in rearranged.chars() {
        let v = ch.to_digit(36)?;
        for digit in v.to_string().chars() {
            rem = (rem * 10 + digit.to_digit(10)?) % 97;
        }
    }
    (rem == 1).then_some(Evidence::Confirmed)
}
fn check_aba(s: &str) -> Option<Evidence> {
    let d = digits(s);
    let d = &d[d.len().checked_sub(9)?..];
    let sum = 3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + d[2] + d[5] + d[8];
    (sum.is_multiple_of(10) && sum > 0).then_some(Evidence::Confirmed)
}
/// ICAO 9303 check digit over the document number field of a TD3 passport MRZ line 2.
fn check_mrz(s: &str) -> Option<Evidence> {
    let line2: String = s.split_whitespace().last()?.chars().collect();
    let line2 = if line2.len() >= 28 {
        line2
    } else {
        return None;
    };
    let value = |c: char| match c {
        '0'..='9' => c.to_digit(10),
        'A'..='Z' => Some(c as u32 - 'A' as u32 + 10),
        '<' => Some(0),
        _ => None,
    };
    let check = |field: &str, digit: char| -> Option<bool> {
        let w = [7, 3, 1];
        let sum: u32 = field
            .chars()
            .enumerate()
            .map(|(i, c)| value(c).map(|v| v * w[i % 3]))
            .sum::<Option<u32>>()?;
        Some(sum % 10 == digit.to_digit(10)?)
    };
    let ch: Vec<char> = line2.chars().collect();
    let doc: String = ch[0..9].iter().collect();
    let dob: String = ch[13..19].iter().collect();
    (check(&doc, ch[9])? && check(&dob, ch[19])?).then_some(Evidence::Confirmed)
}
fn check_ipv4(s: &str) -> Option<Evidence> {
    let ip: std::net::Ipv4Addr = s.parse().ok()?;
    // Version-like strings such as 1.2.3.4 remain possible; dotted quads are reported anyway.
    (!ip.is_unspecified() && !ip.is_broadcast()).then_some(Evidence::Confirmed)
}
fn check_ipv6(s: &str) -> Option<Evidence> {
    let ip: std::net::Ipv6Addr = s.parse().ok()?;
    (s.matches(':').count() >= 2 && !ip.is_unspecified()).then_some(Evidence::Confirmed)
}
fn check_ssn(s: &str) -> Option<Evidence> {
    let d = digits(s);
    let area = d[0] * 100 + d[1] * 10 + d[2];
    let group = d[3] * 10 + d[4];
    let serial = d[5..].iter().fold(0, |a, x| a * 10 + x);
    (area != 0 && area != 666 && area < 900 && group != 0 && serial != 0)
        .then_some(Evidence::Pattern)
}
fn check_entropy(s: &str) -> Option<Evidence> {
    let classes = [
        s.chars().any(|c| c.is_ascii_lowercase()),
        s.chars().any(|c| c.is_ascii_uppercase()),
        s.chars().any(|c| c.is_ascii_digit()),
    ];
    if classes.iter().filter(|x| **x).count() < 2 {
        return None;
    }
    let mut counts = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let n = s.chars().count() as f64;
    let h: f64 = counts
        .values()
        .map(|&k| {
            let p = k as f64 / n;
            -p * p.log2()
        })
        .sum();
    (h >= 4.0).then_some(Evidence::Contextual)
}

/// Distinguishes private/public addresses without revealing the address itself.
fn ip_detail(value: &str) -> &'static str {
    if let Ok(ip) = value.parse::<std::net::Ipv4Addr>() {
        if ip.is_private()
            || ip.is_loopback()
            || ip.is_link_local()
            || ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1])
        {
            return "private/internal IPv4 address";
        }
        return "public IPv4 address";
    }
    if let Ok(ip) = value.parse::<std::net::Ipv6Addr>() {
        let s = ip.segments()[0];
        if ip.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80 {
            return "private/internal IPv6 address";
        }
        return "public IPv6 address";
    }
    "IP address"
}

/// A line of text with an optional location in image coordinates (from OCR or a code).
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub region: Option<Rect>,
    pub source: Source,
}

/// Scans text and returns findings that never contain the matched value itself.
pub fn scan(spans: &[TextSpan], countries: &[Country]) -> Vec<Finding> {
    let mut out: Vec<Finding> = vec![];
    for span in spans {
        let text = truncate(&span.text);
        let mut claimed: Vec<(usize, usize)> = vec![];
        for rule in rules() {
            if rule.country.is_some_and(|c| !countries.contains(&c)) {
                continue;
            }
            for m in rule.regex.find_iter(text).take(256) {
                // A more specific earlier rule already owns this text.
                if claimed.iter().any(|(a, b)| m.start() < *b && *a < m.end()) {
                    continue;
                }
                let evidence = match rule.check {
                    Some(check) => match check(m.as_str()) {
                        Some(e) => e,
                        None => continue,
                    },
                    None => rule.evidence,
                };
                let mut description = rule.description.to_string();
                if rule.category == Category::IpAddress {
                    description = ip_detail(m.as_str()).into();
                } else if rule.category == Category::Url {
                    description = url_detail(m.as_str());
                }
                claimed.push((m.start(), m.end()));
                out.push(Finding::new(
                    if rule.category == Category::Url && description.starts_with("Internal") {
                        Category::InternalUrl
                    } else {
                        rule.category
                    },
                    evidence,
                    span.source,
                    span.region,
                    description,
                    Some(m.as_str().chars().count()),
                ));
            }
        }
    }
    out
}
fn truncate(s: &str) -> &str {
    if s.len() <= MAX_TEXT_BYTES {
        return s;
    }
    let mut end = MAX_TEXT_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
fn url_detail(u: &str) -> String {
    let host = u
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let internal = host == "localhost"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_private() || ip.is_loopback())
        || [
            ".local",
            ".internal",
            ".intranet",
            ".corp",
            ".lan",
            ".localdomain",
        ]
        .iter()
        .any(|s| host.ends_with(s));
    let creds = u
        .split("://")
        .nth(1)
        .is_some_and(|r| r.split('/').next().unwrap_or("").contains('@'));
    match (internal, creds) {
        (true, _) => "Internal application URL (not opened)".into(),
        (_, true) => "URL with embedded credentials (not opened)".into(),
        _ => "URL (not opened)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn find(t: &str) -> Vec<Finding> {
        scan(
            &[TextSpan {
                text: t.into(),
                region: None,
                source: Source::Text,
            }],
            &Country::ALL,
        )
    }
    fn has(t: &str, c: Category, e: Evidence) -> bool {
        find(t).iter().any(|f| f.category == c && f.evidence == e)
    }
    // All values below are synthetic, published test numbers or deliberately invalid.
    #[test]
    fn checksummed_identifiers() {
        assert!(has(
            "card 4111 1111 1111 1111 exp",
            Category::CreditCard,
            Evidence::Confirmed
        ));
        assert!(!has(
            "4111 1111 1111 1112",
            Category::CreditCard,
            Evidence::Confirmed
        ));
        assert!(has(
            "IBAN GB82 WEST 1234 5698 7654 32",
            Category::FinancialAccount,
            Evidence::Confirmed
        ));
        assert!(!has(
            "GB82 WEST 1234 5698 7654 33",
            Category::FinancialAccount,
            Evidence::Confirmed
        ));
        assert!(has(
            "routing 011000015",
            Category::FinancialAccount,
            Evidence::Confirmed
        ));
        assert!(has(
            "SIN 046 454 286",
            Category::NationalId,
            Evidence::Confirmed
        ));
        assert!(has(
            "SSN 123-45-6789",
            Category::NationalId,
            Evidence::Pattern
        ));
        assert!(!has("000-12-3456", Category::NationalId, Evidence::Pattern));
        // ICAO 9303 specimen MRZ.
        assert!(has(
            "P<UTOERIKSSON<<ANNA<MARIA<<<<<<<<<<<<<<<<<<<\nL898902C36UTO7408122F1204159ZE184226B<<<<<10",
            Category::PassportNumber,
            Evidence::Confirmed
        ));
    }
    #[test]
    fn credentials_and_network() {
        assert!(has(
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            Category::PrivateKey,
            Evidence::Confirmed
        ));
        assert!(has(
            "key AKIAIOSFODNN7EXAMPLE",
            Category::ApiKey,
            Evidence::Pattern
        ));
        assert!(has(
            "postgres://admin:example-pass@db.internal:5432/app",
            Category::ConnectionString,
            Evidence::Pattern
        ));
        assert!(has(
            "password: correct-horse",
            Category::Password,
            Evidence::Contextual
        ));
        assert!(has(
            "export DATABASE_PASSWORD=abcdef123",
            Category::Credential,
            Evidence::Pattern
        ));
        assert!(has(
            "otpauth://totp/Example:a@b.c?secret=JBSWY3DPEHPK3PXP",
            Category::AuthenticationCode,
            Evidence::Confirmed
        ));
        assert!(has(
            "WIFI:T:WPA;S:ExampleNet;P:example-pass;;",
            Category::WifiCredentials,
            Evidence::Confirmed
        ));
        let ip = find("gateway 192.168.1.1 and 8.8.8.8");
        assert_eq!(
            ip.iter()
                .filter(|f| f.category == Category::IpAddress)
                .count(),
            2
        );
        assert!(
            ip.iter().any(|f| f.description.starts_with("private"))
                && ip.iter().any(|f| f.description.starts_with("public"))
        );
        assert!(has(
            "fe80::1ff:fe23:4567:890a",
            Category::IpAddress,
            Evidence::Confirmed
        ));
        assert!(has(
            "aa:bb:cc:dd:ee:ff",
            Category::MacAddress,
            Evidence::Pattern
        ));
        assert!(has(
            "http://wiki.corp/page",
            Category::InternalUrl,
            Evidence::Pattern
        ));
        assert!(has(
            r"C:\Users\someone\Documents\notes.txt",
            Category::FilePath,
            Evidence::Pattern
        ));
    }
    #[test]
    fn personal_and_country_rules() {
        assert!(has(
            "mail someone@example.com",
            Category::Email,
            Evidence::Pattern
        ));
        assert!(has(
            "call (555) 123-4567",
            Category::PhoneNumber,
            Evidence::Pattern
        ));
        assert!(has(
            "DOB: 01/02/1990",
            Category::DateOfBirth,
            Evidence::Contextual
        ));
        assert!(has(
            "40.7128, -74.0060",
            Category::GpsLocation,
            Evidence::Pattern
        ));
        assert!(has(
            "aircraft G-ABCD",
            Category::AircraftRegistration,
            Evidence::Pattern
        ));
        let us_only = scan(
            &[TextSpan {
                text: "SIN 046 454 286".into(),
                region: None,
                source: Source::Text,
            }],
            &[Country::Us],
        );
        assert!(!us_only.iter().any(|f| f.category == Category::NationalId));
    }
    #[test]
    fn findings_never_contain_values() {
        let f = find("password: correct-horse 4111111111111111 someone@example.com");
        let json = serde_json::to_string(&f).unwrap();
        for secret in ["correct-horse", "4111111111111111", "someone@example.com"] {
            assert!(!json.contains(secret));
        }
        assert!(f.iter().all(|x| x.value_length.is_some()));
    }
    #[test]
    fn large_and_odd_input() {
        let big = "a".repeat(MAX_TEXT_BYTES + 10);
        let _ = find(&big);
        let _ = find("\u{0}\u{ffff}é".repeat(1000).as_str());
    }
}
