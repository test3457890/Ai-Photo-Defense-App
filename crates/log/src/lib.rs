//! One structured logger for the whole application.
//!
//! Privacy boundary: callers cannot attach arbitrary key/value data. Every record has a fixed,
//! allowlisted schema (component, stable error code, durations, ids, recovery status) and one
//! free-text description that is always sanitized: control characters removed (no log
//! injection), file paths reduced to a file extension, long digit runs and high-entropy tokens
//! masked, URLs/e-mail addresses replaced, and length capped. Image pixels, OCR text, code
//! payloads and credentials have no field to travel in.
//!
//! Records are JSON lines in a local directory with size-based rotation and a total storage
//! cap. Nothing is ever transmitted. A bounded in-memory ring feeds the Diagnostics Center.
//!
//! `tracing` was evaluated: its open field model lets any library attach arbitrary values,
//! which conflicts with the allowlist requirement. A closed API is simpler to audit.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_DESCRIPTION: usize = 400;
const RING: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}
impl Level {
    pub const ALL: [Self; 6] = [
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warning,
        Self::Error,
        Self::Critical,
    ];
}
impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Component {
    App,
    Gui,
    Decode,
    Edit,
    Color,
    Raw,
    Privacy,
    Model,
    Export,
    Batch,
    Project,
    Catalog,
    Security,
    Performance,
    Recovery,
}
impl Component {
    pub const ALL: [Self; 15] = [
        Self::App,
        Self::Gui,
        Self::Decode,
        Self::Edit,
        Self::Color,
        Self::Raw,
        Self::Privacy,
        Self::Model,
        Self::Export,
        Self::Batch,
        Self::Project,
        Self::Catalog,
        Self::Security,
        Self::Performance,
        Self::Recovery,
    ];
}
impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable error codes. Never renumber; add new codes at the end of a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Code {
    Startup,
    Shutdown,
    DecodeFailed,
    UnsupportedFormat,
    ResourceLimit,
    EditFailed,
    ExportFailed,
    ExportBlocked,
    ExportVerified,
    MetadataFound,
    ScanFailed,
    ScanCompleted,
    ModelIntegrity,
    ModelUnavailable,
    ProjectSaved,
    ProjectSaveFailed,
    ProjectRecovered,
    ProjectCorrupt,
    UncleanShutdown,
    Panic,
    FileAccess,
    Database,
    SecurityEvent,
    SlowOperation,
    BatchItemFailed,
    GpuUnavailable,
    RawPreview,
    Other,
}
impl Code {
    pub fn id(self) -> &'static str {
        use Code::*;
        match self {
            Startup => "APD-0001",
            Shutdown => "APD-0002",
            DecodeFailed => "APD-1001",
            UnsupportedFormat => "APD-1002",
            ResourceLimit => "APD-1003",
            EditFailed => "APD-2001",
            ExportFailed => "APD-3001",
            ExportBlocked => "APD-3002",
            ExportVerified => "APD-3003",
            MetadataFound => "APD-4001",
            ScanFailed => "APD-4002",
            ScanCompleted => "APD-4003",
            ModelIntegrity => "APD-5001",
            ModelUnavailable => "APD-5002",
            ProjectSaved => "APD-6001",
            ProjectSaveFailed => "APD-6002",
            ProjectRecovered => "APD-6003",
            ProjectCorrupt => "APD-6004",
            UncleanShutdown => "APD-6005",
            Panic => "APD-6006",
            FileAccess => "APD-7001",
            Database => "APD-7002",
            SecurityEvent => "APD-8001",
            SlowOperation => "APD-9001",
            BatchItemFailed => "APD-3004",
            GpuUnavailable => "APD-9002",
            RawPreview => "APD-1004",
            Other => "APD-0000",
        }
    }
    pub fn explain(self) -> &'static str {
        use Code::*;
        match self {
            Startup | Shutdown => "Application lifecycle event.",
            DecodeFailed => {
                "An image could not be decoded. It may be corrupt, truncated or an unsupported variant."
            }
            UnsupportedFormat => "The file format or bit depth is not supported.",
            ResourceLimit => {
                "A safety limit (dimensions, pixels, memory, file size) stopped processing."
            }
            EditFailed => "An edit could not be applied; the original is unchanged.",
            ExportFailed => "Writing the exported file failed. No existing file was overwritten.",
            ExportBlocked => "Final verification found unresolved findings and export was blocked.",
            ExportVerified => "The exported file was reopened and passed the selected checks.",
            MetadataFound => "Source metadata was found (it is never copied to exports).",
            ScanFailed => "A privacy detector failed; results may be incomplete.",
            ScanCompleted => "A privacy scan finished.",
            ModelIntegrity => {
                "A model file failed hash or structure verification and was not loaded."
            }
            ModelUnavailable => "A feature needs a model that is not installed.",
            ProjectSaved => "Project saved atomically.",
            ProjectSaveFailed => "Saving failed; the previous valid project file was kept.",
            ProjectRecovered => "Work was restored from the autosave journal or backup.",
            ProjectCorrupt => "A project file was unreadable; the last valid copy was used.",
            UncleanShutdown => "The previous session did not exit normally.",
            Panic => "An internal error stopped an operation. No memory dump was collected.",
            FileAccess => "A file could not be read or written.",
            Database => "The local catalog database reported an error.",
            SecurityEvent => {
                "A safety rule rejected input (symbolic link, path, size, untrusted model)."
            }
            SlowOperation => "An operation exceeded its expected duration.",
            BatchItemFailed => "One batch item failed; the rest of the batch continued.",
            GpuUnavailable => "GPU acceleration is not available; the CPU path is used.",
            RawPreview => "A RAW file was opened through its embedded camera preview.",
            Other => "Uncategorised event.",
        }
    }
    pub fn lookup(id: &str) -> Option<Self> {
        use Code::*;
        [
            Startup,
            Shutdown,
            DecodeFailed,
            UnsupportedFormat,
            ResourceLimit,
            EditFailed,
            ExportFailed,
            ExportBlocked,
            ExportVerified,
            MetadataFound,
            ScanFailed,
            ScanCompleted,
            ModelIntegrity,
            ModelUnavailable,
            ProjectSaved,
            ProjectSaveFailed,
            ProjectRecovered,
            ProjectCorrupt,
            UncleanShutdown,
            Panic,
            FileAccess,
            Database,
            SecurityEvent,
            SlowOperation,
            BatchItemFailed,
            GpuUnavailable,
            RawPreview,
            Other,
        ]
        .into_iter()
        .find(|c| c.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub ts_ms: u64,
    pub level: Level,
    pub version: String,
    pub os: String,
    pub component: Component,
    pub code: String,
    pub session: String,
    pub job: Option<u64>,
    pub correlation: Option<u64>,
    pub description: String,
    pub duration_ms: Option<f64>,
    pub recovered: Option<bool>,
}

/// Masks anything that could carry private content. Applied to every description.
pub fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_DESCRIPTION));
    for raw in input.split_whitespace() {
        if out.len() >= MAX_DESCRIPTION {
            break;
        }
        // Strip control and bidi characters (log injection, terminal escapes).
        let word: String = raw
            .chars()
            .filter(|c| {
                !c.is_control()
                    && !('\u{202a}'..='\u{202e}').contains(c)
                    && !('\u{2066}'..='\u{2069}').contains(c)
            })
            .collect();
        if word.is_empty() {
            continue;
        }
        let lower = word.to_ascii_lowercase();
        let masked = if word.contains('\\')
            || word.contains('/') && (word.matches('/').count() >= 2 || word.starts_with('/'))
        {
            match Path::new(word.trim_matches(|c: char| "\"'(),:;".contains(c)))
                .extension()
                .and_then(|e| e.to_str())
            {
                Some(ext) if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => {
                    format!("<path .{}>", ext.to_ascii_lowercase())
                }
                _ => "<path>".into(),
            }
        } else if lower.contains("://") || lower.starts_with("www.") {
            "<url>".into()
        } else if word.contains('@') && word.contains('.') {
            "<email>".into()
        } else if word.chars().filter(|c| c.is_ascii_digit()).count() >= 6 {
            "<number>".into()
        } else if word.len() >= 20 && looks_random(&word) {
            "<token>".into()
        } else if [
            "password", "passwd", "secret", "token", "apikey", "api_key", "otpauth", "wifi:",
        ]
        .iter()
        .any(|k| lower.contains(k))
            && (word.contains('=') || word.contains(':'))
        {
            "<credential>".into()
        } else {
            word.chars().take(64).collect()
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&masked);
    }
    out.chars().take(MAX_DESCRIPTION).collect()
}
fn looks_random(s: &str) -> bool {
    let alnum = s.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    let digits = s.chars().filter(|c| c.is_ascii_digit()).count();
    let upper = s.chars().filter(|c| c.is_ascii_uppercase()).count();
    alnum * 10 >= s.len() * 8
        && (digits > 0 || upper > 2)
        && !s.chars().all(|c| c.is_ascii_lowercase())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub directory: Option<PathBuf>,
    pub min_level: Level,
    /// Rotate the active file at this size.
    pub max_file_bytes: u64,
    /// Total bytes kept across rotated files.
    pub max_total_bytes: u64,
    pub retention_days: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            directory: None,
            min_level: Level::Info,
            max_file_bytes: 1 << 20,
            max_total_bytes: 8 << 20,
            retention_days: 14,
        }
    }
}

struct State {
    config: Config,
    session: String,
    ring: VecDeque<Record>,
    next_job: u64,
}
fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        let now = now_ms();
        let seed = format!("{now}-{}", std::process::id());
        Mutex::new(State {
            config: Config::default(),
            session: blake3::hash(seed.as_bytes()).to_hex()[..12].to_string(),
            ring: VecDeque::with_capacity(RING),
            next_job: 1,
        })
    })
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|p| p.into_inner())
}

pub fn configure(config: Config) {
    if let Some(d) = &config.directory {
        let _ = std::fs::create_dir_all(d);
        restrict_dir(d);
    }
    lock().config = config;
    prune();
}
pub fn config() -> Config {
    lock().config.clone()
}
pub fn session_id() -> String {
    lock().session.clone()
}
pub fn new_job() -> u64 {
    let mut s = lock();
    s.next_job += 1;
    s.next_job
}

#[cfg(unix)]
fn restrict_dir(d: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700));
}
#[cfg(not(unix))]
fn restrict_dir(_d: &Path) {
    // Per-user application data directories on Windows are already private to the account.
}

/// Builder for one event. Only allowlisted fields exist.
pub struct Event {
    r: Record,
}
pub fn event(level: Level, component: Component, code: Code, description: &str) -> Event {
    let s = lock();
    Event {
        r: Record {
            ts_ms: now_ms(),
            level,
            version: env!("CARGO_PKG_VERSION").into(),
            os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            component,
            code: code.id().into(),
            session: s.session.clone(),
            job: None,
            correlation: None,
            description: sanitize(description),
            duration_ms: None,
            recovered: None,
        },
    }
}
impl Event {
    pub fn job(mut self, id: u64) -> Self {
        self.r.job = Some(id);
        self
    }
    pub fn correlation(mut self, id: u64) -> Self {
        self.r.correlation = Some(id);
        self
    }
    pub fn duration(mut self, d: std::time::Duration) -> Self {
        self.r.duration_ms = Some((d.as_secs_f64() * 1e6).round() / 1e3);
        self
    }
    pub fn recovered(mut self, ok: bool) -> Self {
        self.r.recovered = Some(ok);
        self
    }
    pub fn emit(self) {
        let mut s = lock();
        if self.r.level < s.config.min_level {
            return;
        }
        if s.ring.len() >= RING {
            s.ring.pop_front();
        }
        s.ring.push_back(self.r.clone());
        if let Some(dir) = s.config.directory.clone() {
            let max = s.config.max_file_bytes;
            drop(s);
            write_line(&dir, &self.r, max);
        }
    }
}
pub fn info(c: Component, code: Code, d: &str) {
    event(Level::Info, c, code, d).emit()
}
pub fn warn(c: Component, code: Code, d: &str) {
    event(Level::Warning, c, code, d).emit()
}
pub fn error(c: Component, code: Code, d: &str) {
    event(Level::Error, c, code, d).emit()
}

fn active(dir: &Path) -> PathBuf {
    dir.join("photo-defense.log")
}
fn write_line(dir: &Path, r: &Record, max: u64) {
    let path = active(dir);
    if std::fs::metadata(&path)
        .map(|m| m.len() >= max)
        .unwrap_or(false)
    {
        let rotated = dir.join(format!("photo-defense-{}.log", r.ts_ms));
        let _ = std::fs::rename(&path, rotated);
        prune();
    }
    if let Ok(line) = serde_json::to_string(r)
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Applies retention age and total-size limits to rotated files.
pub fn prune() {
    let c = config();
    let Some(dir) = c.directory else { return };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("photo-defense")
                && e.file_name().to_string_lossy().ends_with(".log")
        })
        .filter_map(|e| {
            let m = std::fs::symlink_metadata(e.path()).ok()?;
            (m.is_file()).then(|| (e.path(), m.len(), m.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.2));
    let max_age = std::time::Duration::from_secs(u64::from(c.retention_days) * 86_400);
    let mut total = 0;
    for (p, len, modified) in files {
        total += len;
        let old = modified.elapsed().map(|e| e > max_age).unwrap_or(false);
        if (total > c.max_total_bytes || old) && p != active(&dir) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Deletes all log files and the in-memory history.
pub fn clear() -> usize {
    let mut n = 0;
    lock().ring.clear();
    if let Some(dir) = config().directory
        && let Ok(rd) = std::fs::read_dir(&dir)
    {
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("photo-defense")
                && name.ends_with(".log")
                && std::fs::remove_file(e.path()).is_ok()
            {
                n += 1;
            }
        }
    }
    n
}

pub fn recent(min: Level, component: Option<Component>, search: &str) -> Vec<Record> {
    let s = lock();
    let q = search.to_ascii_lowercase();
    s.ring
        .iter()
        .filter(|r| r.level >= min && component.is_none_or(|c| r.component == c))
        .filter(|r| {
            q.is_empty()
                || r.description.to_ascii_lowercase().contains(&q)
                || r.code.to_ascii_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Sanitized diagnostic report for the user to preview before choosing to share it.
pub fn report(extra: &[(&str, String)]) -> String {
    let records = recent(Level::Warning, None, "");
    let mut s = format!(
        "AI Photo Defense diagnostic report\nversion: {}\nos: {}-{}\nsession: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        session_id()
    );
    for (k, v) in extra {
        s.push_str(&format!("{}: {}\n", sanitize(k), sanitize(v)));
    }
    s.push_str(&format!("\nwarnings and errors ({}):\n", records.len()));
    for r in records.iter().rev().take(200) {
        s.push_str(&format!(
            "{} {:?} {:?} {} {}\n",
            r.ts_ms, r.level, r.component, r.code, r.description
        ));
    }
    s.push_str("\nThis report contains no images, paths, OCR text, code payloads or credentials. Review it before sharing.\n");
    s
}

/// Installs a panic hook that records a sanitized critical event and a crash marker.
/// No memory dump is collected.
pub fn install_panic_hook(marker: Option<PathBuf>) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| {
                format!(
                    "{}:{}",
                    l.file().rsplit(['/', '\\']).next().unwrap_or(""),
                    l.line()
                )
            })
            .unwrap_or_default();
        event(
            Level::Critical,
            Component::App,
            Code::Panic,
            &format!("internal error at {location}"),
        )
        .emit();
        if let Some(m) = &marker {
            let _ = std::fs::write(m, format!("{}\n", now_ms()));
        }
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic sensitive values only.
    #[test]
    fn sanitizer_removes_private_values_and_injection() {
        let cases = [
            (
                "failed to open C:\\Users\\alice\\Pictures\\passport.jpg",
                "alice",
            ),
            ("failed /home/bob/secret-trip/IMG_0001.CR2", "bob"),
            ("card 4111111111111111 rejected", "4111111111111111"),
            (
                "token ghp_AbCdEf0123456789AbCdEf0123456789AbCd leaked",
                "ghp_AbCd",
            ),
            ("password=hunter2-example in qr", "hunter2"),
            ("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP", "JBSWY3DP"),
            ("contact someone@example.com", "someone@example.com"),
            ("WIFI:T:WPA;S:Net;P:example-pass;;", "example-pass"),
        ];
        for (input, secret) in cases {
            let out = sanitize(input);
            assert!(!out.contains(secret), "{input} -> {out}");
        }
        let injected = sanitize("ok\n{\"level\":\"Critical\"}\r\x1b[31mred\u{202e}");
        assert!(
            !injected.contains('\n')
                && !injected.contains('\x1b')
                && !injected.contains('\u{202e}')
        );
        assert!(sanitize(&"x".repeat(5000)).len() <= MAX_DESCRIPTION);
        assert_eq!(
            sanitize("Decoder rejected header"),
            "Decoder rejected header"
        );
        assert_eq!(sanitize("bad file a/b/c.png"), "bad file <path .png>");
    }
    #[test]
    fn file_logging_rotation_ring_and_clear() {
        let d = tempfile::tempdir().unwrap();
        configure(Config {
            directory: Some(d.path().into()),
            min_level: Level::Debug,
            max_file_bytes: 600,
            max_total_bytes: 2000,
            retention_days: 1,
        });
        for i in 0..40 {
            event(
                Level::Warning,
                Component::Decode,
                Code::DecodeFailed,
                &format!("attempt {i} for /home/carol/x.jpg"),
            )
            .job(new_job())
            .duration(std::time::Duration::from_millis(3))
            .emit();
        }
        event(Level::Trace, Component::App, Code::Other, "below threshold").emit();
        let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
        assert!(files.len() >= 2, "rotated");
        let total: u64 = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        assert!(total <= 2000 + 600);
        let all: String = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        assert!(!all.contains("carol") && !all.contains("below threshold"));
        for line in all.lines() {
            let r: Record = serde_json::from_str(line).unwrap();
            assert_eq!(r.code, "APD-1001");
        }
        assert!(!recent(Level::Warning, Some(Component::Decode), "attempt").is_empty());
        let rep = report(&[("cache", "C:\\Users\\dave\\cache".into())]);
        assert!(!rep.contains("dave") && !rep.contains("carol"));
        assert!(clear() >= 1);
        assert!(recent(Level::Trace, None, "").is_empty());
        assert_eq!(Code::lookup("apd-1001"), Some(Code::DecodeFailed));
        configure(Config::default());
    }
}
