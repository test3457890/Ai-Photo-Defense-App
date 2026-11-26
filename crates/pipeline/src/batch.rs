//! Batch processing over user-selected files and folders.
//!
//! Safety rules: symbolic links and junctions are never followed; every file must resolve
//! inside the root it was found under; the output directory is excluded from collection;
//! traversal depth and file counts are bounded; originals are never overwritten.
use crate::{Prepared, conceal_findings, prepare, publish};
use anyhow::{Result, ensure};
use photo_core::ExportFormat;
use photo_edit::{EditStack, Operation};
use photo_privacy::{Finding, Profile};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "tif", "tiff"];

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub recursive: bool,
    pub max_depth: usize,
    pub max_files: usize,
    pub exclude: Vec<PathBuf>,
}
impl Default for CollectOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            max_depth: 16,
            max_files: 10_000,
            exclude: vec![],
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Collected {
    pub files: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, String)>,
}

pub fn collect(inputs: &[PathBuf], opts: &CollectOptions) -> Result<Collected> {
    let mut out = Collected::default();
    let exclude: Vec<PathBuf> = opts
        .exclude
        .iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();
    for input in inputs {
        let meta = match std::fs::symlink_metadata(input) {
            Ok(m) => m,
            Err(e) => {
                out.skipped.push((input.clone(), e.to_string()));
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            out.skipped
                .push((input.clone(), "symbolic link not followed".into()));
        } else if meta.is_file() {
            out.files.push(input.clone());
        } else if meta.is_dir() {
            let root = std::fs::canonicalize(input)?;
            walk(&root, &root, 0, opts, &exclude, &mut out)?;
        }
        ensure!(
            out.files.len() <= opts.max_files,
            "More than {} files selected",
            opts.max_files
        );
    }
    out.files.sort();
    out.files.dedup();
    Ok(out)
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    opts: &CollectOptions,
    exclude: &[PathBuf],
    out: &mut Collected,
) -> Result<()> {
    if exclude.iter().any(|e| dir.starts_with(e)) {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            out.skipped
                .push((path, "symbolic link or junction not followed".into()));
            continue;
        }
        // Defense in depth against reparse points that are not reported as links.
        match std::fs::canonicalize(&path) {
            Ok(real) if real.starts_with(root) => {}
            _ => {
                out.skipped
                    .push((path, "resolves outside the selected folder".into()));
                continue;
            }
        }
        if meta.is_dir() {
            if opts.recursive && depth < opts.max_depth {
                walk(root, &path, depth + 1, opts, exclude, out)?;
            } else if opts.recursive {
                out.skipped
                    .push((path, "maximum folder depth reached".into()));
            }
        } else if meta.is_file() {
            let ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
            if ok {
                out.files.push(path);
                ensure!(
                    out.files.len() <= opts.max_files,
                    "More than {} files selected",
                    opts.max_files
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BatchOptions {
    pub output_dir: PathBuf,
    pub format: ExportFormat,
    pub quality: u8,
    pub profile: Profile,
    /// Shared edits applied to every photo before redaction (crop, resize...).
    pub edits: Vec<Operation>,
    /// Automatically conceal every located finding. Off by default: findings go to review.
    pub auto_redact: bool,
    /// Publish even when final verification reports unresolved findings.
    pub accept_unresolved: bool,
    pub skip_duplicates: bool,
}

#[derive(Debug, Clone, Serialize)]
pub enum Outcome {
    Exported {
        image: PathBuf,
        verification_passed: bool,
    },
    /// Located or uncertain findings need a human decision before export.
    NeedsReview {
        findings: Vec<Finding>,
    },
    Duplicate {
        of: PathBuf,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub results: Vec<(PathBuf, Outcome)>,
    pub exported: usize,
    pub review: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub cancelled: bool,
}

pub struct Control {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            pause: AtomicBool::new(false),
        }
    }
}

/// Processes files one at a time; one failure never stops the batch.
pub fn run(
    files: &[PathBuf],
    opts: &BatchOptions,
    control: &Control,
    mut progress: impl FnMut(usize, &Path, &Outcome),
) -> Result<Summary> {
    ensure!(
        opts.output_dir.is_dir(),
        "Output directory must already exist"
    );
    let mut summary = Summary::default();
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    for (i, path) in files.iter().enumerate() {
        while control.pause.load(Ordering::Relaxed) && !control.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if control.cancel.load(Ordering::Relaxed) {
            summary.cancelled = true;
            summary.results.push((path.clone(), Outcome::Cancelled));
            continue;
        }
        let outcome = match process(path, opts, &mut seen) {
            Ok(o) => o,
            Err(e) => Outcome::Failed {
                error: format!("{e:#}"),
            },
        };
        match &outcome {
            Outcome::Exported { .. } => summary.exported += 1,
            Outcome::NeedsReview { .. } => summary.review += 1,
            Outcome::Duplicate { .. } => summary.duplicates += 1,
            Outcome::Failed { .. } => summary.failed += 1,
            Outcome::Cancelled => {}
        }
        progress(i, path, &outcome);
        summary.results.push((path.clone(), outcome));
    }
    Ok(summary)
}

fn process(
    path: &Path,
    opts: &BatchOptions,
    seen: &mut HashMap<String, PathBuf>,
) -> Result<Outcome> {
    let photo = photo_core::load(path)?;
    if opts.skip_duplicates {
        if let Some(of) = seen.get(&photo.fingerprint.blake3) {
            return Ok(Outcome::Duplicate { of: of.clone() });
        }
        seen.insert(photo.fingerprint.blake3.clone(), path.to_path_buf());
    }
    let mut edits = EditStack::from_operations(opts.edits.clone())?;
    // Findings are located on the edited geometry so their boxes match the output.
    let geometry = edits.render(&photo.pixels)?;
    let staged = photo_core::Photo {
        pixels: geometry,
        fingerprint: photo.fingerprint.clone(),
        format: photo.format,
        bytes: photo.bytes.clone(),
    };
    let scan = crate::scan_photo(&staged, &opts.profile);
    let pixel_findings: Vec<Finding> = scan
        .findings
        .iter()
        .filter(|f| f.region.is_some())
        .cloned()
        .collect();
    if !pixel_findings.is_empty() {
        if !opts.auto_redact {
            return Ok(Outcome::NeedsReview {
                findings: pixel_findings,
            });
        }
        if let Some(op) = conceal_findings(&pixel_findings, opts.profile.redaction, 4) {
            edits.push(op)?;
        }
    }
    let prepared: Prepared = prepare(&photo, &edits, opts.format, opts.quality, &opts.profile)?;
    let saved = publish(
        &prepared,
        path,
        &opts.output_dir,
        opts.format,
        opts.accept_unresolved,
    )?;
    Ok(Outcome::Exported {
        image: saved.image,
        verification_passed: saved.verification_passed,
    })
}

/// Polling folder monitor: returns files that are new or changed since the last call.
/// Polling is used instead of OS watch APIs to keep behaviour identical on every platform.
#[derive(Default)]
pub struct FolderMonitor {
    known: HashMap<PathBuf, (u64, std::time::SystemTime)>,
}
impl FolderMonitor {
    pub fn poll(&mut self, inputs: &[PathBuf], opts: &CollectOptions) -> Result<Vec<PathBuf>> {
        let c = collect(inputs, opts)?;
        let mut fresh = vec![];
        for f in c.files {
            let Ok(m) = std::fs::metadata(&f) else {
                continue;
            };
            let stamp = (m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH));
            if self.known.get(&f) != Some(&stamp) {
                self.known.insert(f.clone(), stamp);
                fresh.push(f);
            }
        }
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write_png(path: &Path, shade: u8) {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            16,
            16,
            image::Rgb([shade; 3]),
        ));
        std::fs::write(
            path,
            photo_core::encode(&img, ExportFormat::Png, 95).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn collects_recursively_excluding_output_and_bounds() {
        let d = tempfile::tempdir().unwrap();
        let sub = d.path().join("sub");
        let out = d.path().join("out");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        write_png(&d.path().join("a.png"), 1);
        write_png(&sub.join("b.PNG"), 2);
        write_png(&out.join("old.png"), 3);
        std::fs::write(d.path().join("notes.txt"), "x").unwrap();
        let flat = collect(
            &[d.path().to_path_buf()],
            &CollectOptions {
                exclude: vec![out.clone()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(flat.files.len(), 1);
        let deep = collect(
            &[d.path().to_path_buf()],
            &CollectOptions {
                recursive: true,
                exclude: vec![out.clone()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(deep.files.len(), 2);
        assert!(
            collect(
                &[d.path().to_path_buf()],
                &CollectOptions {
                    recursive: true,
                    max_files: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write_png(&outside.path().join("secret.png"), 9);
        std::os::unix::fs::symlink(outside.path(), d.path().join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.png"), d.path().join("f.png"))
            .unwrap();
        let c = collect(
            &[d.path().to_path_buf()],
            &CollectOptions {
                recursive: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(c.files.is_empty());
        assert_eq!(c.skipped.len(), 2);
    }
    #[test]
    fn batch_review_duplicates_errors_and_cancel() {
        let d = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        write_png(&d.path().join("a.png"), 10);
        write_png(&d.path().join("b.png"), 10);
        std::fs::write(d.path().join("c.png"), b"broken").unwrap();
        let code = crate::tests::qr_photo("WIFI:T:WPA;S:Example;P:example-pass;;");
        std::fs::write(d.path().join("d.png"), &code.bytes).unwrap();
        let files = collect(&[d.path().to_path_buf()], &CollectOptions::default())
            .unwrap()
            .files;
        let mut opts = BatchOptions {
            output_dir: out.path().to_path_buf(),
            format: ExportFormat::Png,
            quality: 95,
            profile: Profile::builtin("Home Privacy").unwrap(),
            edits: vec![],
            auto_redact: false,
            accept_unresolved: false,
            skip_duplicates: true,
        };
        let s = run(&files, &opts, &Control::default(), |_, _, _| {}).unwrap();
        assert_eq!(
            (s.exported, s.duplicates, s.failed, s.review),
            (1, 1, 1, 1),
            "{:?}",
            s.results
        );
        opts.auto_redact = true;
        opts.skip_duplicates = false;
        let s = run(&files[3..], &opts, &Control::default(), |_, _, _| {}).unwrap();
        assert_eq!(s.exported, 1, "{:?}", s.results);
        let control = Control::default();
        control.cancel.store(true, Ordering::Relaxed);
        let s = run(&files, &opts, &control, |_, _, _| {}).unwrap();
        assert!(s.cancelled && s.exported == 0);
        // Originals untouched.
        assert_eq!(std::fs::read(d.path().join("c.png")).unwrap(), b"broken");
    }
    #[test]
    fn monitor_reports_new_files_once() {
        let d = tempfile::tempdir().unwrap();
        let mut m = FolderMonitor::default();
        let roots = [d.path().to_path_buf()];
        assert!(
            m.poll(&roots, &CollectOptions::default())
                .unwrap()
                .is_empty()
        );
        write_png(&d.path().join("new.png"), 4);
        assert_eq!(m.poll(&roots, &CollectOptions::default()).unwrap().len(), 1);
        assert!(
            m.poll(&roots, &CollectOptions::default())
                .unwrap()
                .is_empty()
        );
    }
}
