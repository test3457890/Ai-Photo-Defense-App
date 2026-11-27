//! Projects hold the reversible state (photo references, edit stacks, ratings, tags, settings).
//! Original photographs are never stored or modified here.
//!
//! Durability rules:
//! - Every write goes to a private temporary file in the same directory, is fsynced, then
//!   atomically renamed. A crash mid-write leaves the previous file intact.
//! - Before replacing a project, the previous *valid* version is copied to `.bak` and to a
//!   bounded set of timestamped checkpoints. Invalid data never replaces valid data.
//! - The autosave journal is a separate file; it is only offered for restore when it parses.
//! - A session lock detects unexpected shutdown on the next start.
use anyhow::{Context, Result, bail, ensure};
use photo_edit::EditStack;
use photo_log::{Code, Component};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "apdproj";
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectEntry {
    pub path: PathBuf,
    pub edits: EditStack,
    #[serde(default)]
    pub rating: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub flagged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub profile: String,
    pub format: String,
    pub quality: u8,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_model_dir: Option<String>,
    pub face_model_path: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: "Social Media".into(),
            format: "png".into(),
            quality: 92,
            mode: "Professional".into(),
            voice_model_dir: None,
            face_model_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    pub entries: Vec<ProjectEntry>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub saved_ms: u64,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            name: "Untitled".into(),
            entries: vec![],
            settings: Settings::default(),
            saved_ms: 0,
        }
    }
}
impl Project {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA,
            "Unsupported project version {}",
            self.schema_version
        );
        ensure!(self.name.len() <= 200, "Project name too long");
        ensure!(self.entries.len() <= 100_000, "Too many entries");
        for e in &self.entries {
            ensure!(e.rating <= 5, "Rating out of range");
            ensure!(
                e.tags.len() <= 64 && e.tags.iter().all(|t| t.len() <= 64),
                "Too many or too long tags"
            );
            ensure!(
                e.edits.active().len() <= photo_edit::MAX_OPERATIONS,
                "Edit history too long"
            );
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
    }
    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        ensure!(
            b.len() as u64 <= MAX_PROJECT_BYTES,
            "Project file too large"
        );
        let p: Project = serde_json::from_slice(b).context("Project file is not valid")?;
        p.validate()?;
        Ok(p)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Project path must be a regular file"
    );
    ensure!(meta.len() <= MAX_PROJECT_BYTES, "Project file too large");
    let mut b = vec![];
    std::fs::File::open(path)?
        .take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut b)?;
    Ok(b)
}
pub fn backup_path(p: &Path) -> PathBuf {
    p.with_extension(format!("{EXTENSION}.bak"))
}
pub fn autosave_path(p: &Path) -> PathBuf {
    p.with_extension(format!("{EXTENSION}.autosave"))
}

/// Writes bytes atomically: temp file in the same directory, fsync, rename over target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(dir.is_dir(), "Project folder must exist");
    if let Ok(m) = std::fs::symlink_metadata(path) {
        ensure!(
            !m.file_type().is_symlink(),
            "Refusing to write through a symbolic link"
        );
    }
    let mut t = tempfile::NamedTempFile::new_in(dir)?;
    t.write_all(bytes)?;
    t.as_file().sync_all()?;
    t.persist(path)
        .map_err(|e| anyhow::anyhow!("Atomic save failed: {}", e.error))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SaveOptions {
    pub checkpoints: usize,
}
impl Default for SaveOptions {
    fn default() -> Self {
        Self { checkpoints: 5 }
    }
}

/// Saves a project. The previous valid file becomes `.bak` and a checkpoint.
pub fn save(path: &Path, project: &Project, opts: &SaveOptions) -> Result<()> {
    let mut p = project.clone();
    p.saved_ms = now_ms();
    let bytes = p.to_bytes()?;
    if let Ok(old) = read_bounded(path)
        && Project::from_bytes(&old).is_ok()
    {
        write_atomic(&backup_path(path), &old)?;
        if opts.checkpoints > 0 {
            let cp = path.with_extension(format!("{}.{EXTENSION}", p.saved_ms));
            let cp = checkpoint_dir(path)
                .map(|d| d.join(cp.file_name().unwrap_or_default()))
                .unwrap_or(cp);
            if let Some(d) = cp.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            write_atomic(&cp, &old)?;
            prune_checkpoints(path, opts.checkpoints);
        }
    }
    match write_atomic(path, &bytes) {
        Ok(()) => {
            let _ = std::fs::remove_file(autosave_path(path));
            photo_log::info(Component::Project, Code::ProjectSaved, "project saved");
            Ok(())
        }
        Err(e) => {
            photo_log::error(
                Component::Project,
                Code::ProjectSaveFailed,
                &format!("{e:#}"),
            );
            Err(e)
        }
    }
}
fn checkpoint_dir(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    Some(path.parent()?.join(format!(".{stem}-checkpoints")))
}
pub fn checkpoints(path: &Path) -> Vec<PathBuf> {
    let Some(d) = checkpoint_dir(path) else {
        return vec![];
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(d)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}
fn prune_checkpoints(path: &Path, keep: usize) {
    let v = checkpoints(path);
    if v.len() > keep {
        for p in &v[..v.len() - keep] {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Writes the autosave journal without touching the saved project.
pub fn autosave(path: &Path, project: &Project) -> Result<()> {
    let mut p = project.clone();
    p.saved_ms = now_ms();
    write_atomic(&autosave_path(path), &p.to_bytes()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Project,
    Backup,
    Autosave,
}
#[derive(Debug)]
pub struct Loaded {
    pub project: Project,
    pub source: Source,
    /// A newer valid autosave exists and can be offered to the user.
    pub autosave_available: Option<Project>,
    pub warnings: Vec<String>,
}

/// Opens a project, falling back to the last valid backup if the main file is corrupt.
pub fn open(path: &Path) -> Result<Loaded> {
    let mut warnings = vec![];
    let main = read_bounded(path).and_then(|b| Project::from_bytes(&b));
    let (project, source) = match main {
        Ok(p) => (p, Source::Project),
        Err(e) => {
            photo_log::warn(Component::Project, Code::ProjectCorrupt, &format!("{e:#}"));
            warnings.push(format!(
                "Project file unreadable ({e}); using last valid backup."
            ));
            match read_bounded(&backup_path(path)).and_then(|b| Project::from_bytes(&b)) {
                Ok(p) => {
                    photo_log::event(
                        photo_log::Level::Warning,
                        Component::Recovery,
                        Code::ProjectRecovered,
                        "restored from backup",
                    )
                    .recovered(true)
                    .emit();
                    (p, Source::Backup)
                }
                Err(_) => {
                    match read_bounded(&autosave_path(path)).and_then(|b| Project::from_bytes(&b)) {
                        Ok(p) => (p, Source::Autosave),
                        Err(_) => bail!("No valid project, backup or autosave could be read"),
                    }
                }
            }
        }
    };
    let autosave_available = read_bounded(&autosave_path(path))
        .ok()
        .and_then(|b| Project::from_bytes(&b).ok())
        .filter(|a| a.saved_ms > project.saved_ms && source != Source::Autosave);
    Ok(Loaded {
        project,
        source,
        autosave_available,
        warnings,
    })
}

/// Marks a running session. If a previous lock is still present at start, the last session
/// ended unexpectedly. Dropping the guard (normal shutdown) removes the lock.
pub struct SessionGuard {
    lock: PathBuf,
    pub previous_unclean: bool,
}
impl SessionGuard {
    pub fn acquire(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let lock = dir.join("session.lock");
        let previous_unclean = lock.exists();
        if previous_unclean {
            photo_log::event(
                photo_log::Level::Warning,
                Component::Recovery,
                Code::UncleanShutdown,
                "previous session ended unexpectedly",
            )
            .emit();
        }
        write_atomic(&lock, format!("{}\n", std::process::id()).as_bytes())?;
        Ok(Self {
            lock,
            previous_unclean,
        })
    }
    pub fn crash_marker(&self) -> PathBuf {
        self.lock.with_file_name("last-crash")
    }
}
impl Drop for SessionGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock);
    }
}

/// Removes orphaned temporary files left by interrupted atomic writes.
pub fn clean_temp_files(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp"))
        .filter(|e| std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_file()))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_edit::Operation;
    fn sample(n: usize) -> Project {
        let mut p = Project::default();
        for i in 0..n {
            let mut edits = EditStack::new();
            edits.push(Operation::Rotate90).unwrap();
            p.entries.push(ProjectEntry {
                path: format!("photo{i}.png").into(),
                edits,
                rating: 3,
                tags: vec!["trip".into()],
                flagged: false,
            });
        }
        p
    }
    #[test]
    fn save_reopen_backup_and_checkpoints() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join(format!("a.{EXTENSION}"));
        save(&path, &sample(1), &SaveOptions { checkpoints: 2 }).unwrap();
        for n in 2..6 {
            std::thread::sleep(std::time::Duration::from_millis(3));
            save(&path, &sample(n), &SaveOptions { checkpoints: 2 }).unwrap();
        }
        let l = open(&path).unwrap();
        assert_eq!(l.project.entries.len(), 5);
        assert_eq!(l.source, Source::Project);
        assert_eq!(checkpoints(&path).len(), 2);
        assert_eq!(
            Project::from_bytes(&std::fs::read(backup_path(&path)).unwrap())
                .unwrap()
                .entries
                .len(),
            4
        );
    }
    #[test]
    fn corrupted_project_recovers_from_backup_and_is_never_overwritten_by_garbage() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join(format!("b.{EXTENSION}"));
        save(&path, &sample(1), &SaveOptions::default()).unwrap();
        save(&path, &sample(2), &SaveOptions::default()).unwrap();
        // Simulate an interrupted non-atomic write by another tool: truncated JSON.
        let full = std::fs::read(&path).unwrap();
        std::fs::write(&path, &full[..full.len() / 2]).unwrap();
        let l = open(&path).unwrap();
        assert_eq!(l.source, Source::Backup);
        assert_eq!(l.project.entries.len(), 1);
        // Saving again must not copy the corrupt file over the valid backup.
        save(&path, &sample(3), &SaveOptions::default()).unwrap();
        assert_eq!(
            Project::from_bytes(&std::fs::read(backup_path(&path)).unwrap())
                .unwrap()
                .entries
                .len(),
            1
        );
        // Hostile content is rejected.
        assert!(Project::from_bytes(br#"{"schema_version":99,"name":"x","entries":[]}"#).is_err());
        assert!(Project::from_bytes(b"\xff\xfe garbage").is_err());
    }
    #[test]
    fn autosave_offered_only_when_newer_and_valid() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join(format!("c.{EXTENSION}"));
        save(&path, &sample(1), &SaveOptions::default()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        autosave(&path, &sample(4)).unwrap();
        let l = open(&path).unwrap();
        assert_eq!(l.autosave_available.unwrap().entries.len(), 4);
        std::fs::write(autosave_path(&path), b"{broken").unwrap();
        assert!(open(&path).unwrap().autosave_available.is_none());
        save(&path, &sample(2), &SaveOptions::default()).unwrap();
        assert!(
            !autosave_path(&path).exists(),
            "explicit save clears the journal"
        );
    }
    #[test]
    fn session_lock_detects_unclean_shutdown_and_temp_cleanup() {
        let d = tempfile::tempdir().unwrap();
        {
            let g = SessionGuard::acquire(d.path()).unwrap();
            assert!(!g.previous_unclean);
        }
        let g = SessionGuard::acquire(d.path()).unwrap();
        assert!(!g.previous_unclean, "clean exit removes the lock");
        std::mem::forget(g); // simulate forced termination: Drop never runs
        let g2 = SessionGuard::acquire(d.path()).unwrap();
        assert!(g2.previous_unclean);
        std::fs::write(d.path().join(".tmpABC123"), b"partial").unwrap();
        assert_eq!(clean_temp_files(d.path()), 1);
    }
}
