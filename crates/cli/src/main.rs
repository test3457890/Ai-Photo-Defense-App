use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use photo_core::{ExportFormat, encode, load, quality, safe_stem, save_new};
use std::path::{Path, PathBuf};
#[derive(Parser)]
#[command(
    version,
    about = "Offline photo privacy utility and research lab. AI defenses are experimental."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Png,
    Jpeg,
    Webp,
    Tiff,
}
impl From<Format> for ExportFormat {
    fn from(x: Format) -> Self {
        match x {
            Format::Png => Self::Png,
            Format::Jpeg => Self::Jpeg,
            Format::Webp => Self::WebP,
            Format::Tiff => Self::Tiff,
        }
    }
}
#[derive(Subcommand)]
enum Command {
    /// Decode safely and inspect dimensions and cryptographic hashes.
    Inspect { input: PathBuf },
    /// Experimental CLIP feature suppression, without downstream efficacy validation.
    #[cfg(feature = "experimental-clip")]
    ExperimentalClip {
        input: PathBuf,
        model: PathBuf,
        manifest: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 4)]
        epsilon: u8,
        #[arg(long, default_value_t = 10)]
        steps: u32,
    },
    /// Re-encode pixels without source EXIF/GPS/XMP/ICC. Never overwrites files.
    Export {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "png")]
        format: Format,
        #[arg(long,default_value_t=95,value_parser=clap::value_parser!(u8).range(1..=100))]
        quality: u8,
    },
    /// Compare decoded images. Metrics do not measure AI protection.
    Compare {
        original: PathBuf,
        candidate: PathBuf,
        #[arg(long)]
        difference: Option<PathBuf>,
    },
    /// Test perturbation survival through transformations, with no AI efficacy claim.
    Lab {
        original: PathBuf,
        candidate: PathBuf,
        #[arg(short, long)]
        report: PathBuf,
    },
    /// Verify a local safetensors file against an independently reviewed manifest.
    VerifyModel { model: PathBuf, manifest: PathBuf },
    /// Refuses to claim protection until a validated model adapter is installed.
    Protect { input: PathBuf },
    /// Local Privacy Scan: metadata, decoded codes and sensitive text. Prints JSON without values.
    Scan {
        input: PathBuf,
        #[command(flatten)]
        profile: ProfileArgs,
        /// Additional text to scan (for example output of an external OCR tool you trust).
        #[arg(long)]
        text: Vec<String>,
    },
    /// Edit, redact, verify and export one photograph through the secure pipeline.
    Process {
        input: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[command(flatten)]
        edit: EditArgs,
        #[command(flatten)]
        profile: ProfileArgs,
        #[arg(long, value_enum, default_value = "png")]
        format: Format,
        #[arg(long,default_value_t=92,value_parser=clap::value_parser!(u8).range(1..=100))]
        quality: u8,
        /// Opaquely redact every located finding (decoded QR codes and barcodes).
        #[arg(long)]
        auto_redact: bool,
        /// Publish even if final verification reports unresolved findings.
        #[arg(long)]
        accept_unresolved: bool,
    },
    /// Process files or folders. Photos with located findings go to a review queue unless --auto-redact.
    Batch {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(long)]
        recursive: bool,
        #[command(flatten)]
        edit: EditArgs,
        #[command(flatten)]
        profile: ProfileArgs,
        #[arg(long, value_enum, default_value = "png")]
        format: Format,
        #[arg(long,default_value_t=92,value_parser=clap::value_parser!(u8).range(1..=100))]
        quality: u8,
        #[arg(long)]
        auto_redact: bool,
        #[arg(long)]
        accept_unresolved: bool,
        #[arg(long)]
        keep_duplicates: bool,
        /// Keep watching the inputs, polling every N seconds for new files (Ctrl+C to stop).
        #[arg(long)]
        watch: Option<u64>,
        /// Write the batch summary JSON (new file only).
        #[arg(long)]
        summary: Option<PathBuf>,
    },
    /// List built-in privacy profiles, or print one as JSON for editing and re-import.
    Profiles { name: Option<String> },
    /// Read-only forensic inspection (hashes, EXIF, JPEG tables, C2PA presence). Output is sensitive.
    Forensics { input: PathBuf },
    /// Extract the embedded camera preview from a RAW file as a new JPEG (no RAW development).
    RawPreview { input: PathBuf, output: PathBuf },
    /// Embed an invisible ownership watermark (48-bit payload). The passphrase is read from the
    /// environment variable named by --key-env so it does not appear in shell history.
    /// Survives JPEG quality 70+; does NOT survive resizing or cropping and is not AI protection.
    WatermarkEmbed {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Payload number to embed (0 to 2^48-1), for example an owner or licence ID.
        #[arg(long)]
        payload: u64,
        #[arg(long, default_value = "APD_WATERMARK_KEY")]
        key_env: String,
        #[arg(long, value_enum, default_value_t = Format::Png)]
        format: Format,
        #[arg(long, default_value_t = 92)]
        quality: u8,
        #[arg(long, default_value_t = 4.0)]
        strength: f32,
    },
    /// Look for a watermark made with the passphrase in --key-env.
    WatermarkDetect {
        input: PathBuf,
        #[arg(long, default_value = "APD_WATERMARK_KEY")]
        key_env: String,
    },
    /// Identify any media file by content and list its tracks, codecs and privacy metadata.
    MediaInspect { input: PathBuf },
    /// Remove privacy metadata from MP4/MOV/M4A/3GP without re-encoding; verifies every chunk.
    StripVideo {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Loudness (EBU R128), true peak, clipping, silence and spectrum summary of an audio file.
    AudioAnalyze { input: PathBuf },
    /// Compare two recordings sample by sample (SNR, segmental SNR, length).
    AudioCompare {
        original: PathBuf,
        processed: PathBuf,
    },
    /// Export audio as metadata-free WAV, optionally muting or bleeping time ranges (seconds).
    AudioExport {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Range to mute, e.g. 12.5-14 (repeatable).
        #[arg(long)]
        mute: Vec<String>,
        /// Range to bleep, e.g. 30-31.2 (repeatable).
        #[arg(long)]
        bleep: Vec<String>,
    },
    /// Adversarial voice protection. Refuses without a local ECAPA model directory.
    AudioProtect {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Local directory containing model.safetensors (and optional manifest.json).
        #[arg(long)]
        model: Option<PathBuf>,
        #[arg(long, default_value = "Balanced Protection")]
        profile: String,
    },
    /// Decode H.264 MP4/MOV, burn a time-ranged rectangle redaction, and re-encode cleanly.
    #[cfg(feature = "video")]
    VideoRedact {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Redaction rectangle x,y,w,h (pixels).
        #[arg(long)]
        rect: String,
        #[arg(long)]
        from: f64,
        #[arg(long)]
        to: f64,
        /// Apply strong blur instead of opaque black fill.
        #[arg(long)]
        blur: bool,
    },
    /// Track an object forward/backward from a keyframe in an H.264 MP4/MOV, redact, and verify every frame.
    #[cfg(feature = "video")]
    VideoTrack {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Initial bounding box x,y,w,h (pixels).
        #[arg(long)]
        rect: String,
        /// Keyframe timestamp in seconds.
        #[arg(long, default_value_t = 0.0)]
        at: f64,
        /// Apply strong blur instead of opaque black fill.
        #[arg(long)]
        blur: bool,
        /// Allow an export despite uncertain tracking; every frame then needs manual review.
        #[arg(long)]
        accept_tracking_gaps: bool,
    },
    /// Transcribe speech locally with Whisper, detect spoken private data, and optionally bleep it.
    #[cfg(feature = "stt")]
    #[command(name = "audio-transcribe")]
    AudioTranscribe {
        input: PathBuf,
        /// Local directory containing model.safetensors, config.json, and tokenizer.json.
        #[arg(long)]
        model: PathBuf,
        /// Optional output WAV path with detected private speech ranges bleeped.
        #[arg(long)]
        redact_to: Option<PathBuf>,
    },
    /// Detect assistant wake phrases (Siri, Google, Alexa, Meta, Bixby, custom) with local Whisper and mute/suppress them.
    #[cfg(feature = "stt")]
    #[command(name = "voice-safe-playback")]
    VoiceSafePlayback {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        model: PathBuf,
        /// Suppression mode: mute, bleep, or experimental-band.
        #[arg(long, default_value = "mute")]
        mode: String,
        /// Comma-separated custom wake phrases.
        #[arg(long, default_value = "")]
        custom: String,
    },
    /// Scan SRT/WebVTT subtitles for private information; optionally write a redacted copy.
    Subtitles {
        input: PathBuf,
        #[arg(long)]
        redact_to: Option<PathBuf>,
    },
}
#[derive(clap::Args, Clone)]
struct ProfileArgs {
    /// Built-in privacy profile name.
    #[arg(long, default_value = "Maximum Privacy")]
    profile: String,
    /// Custom profile JSON file (overrides --profile).
    #[arg(long)]
    profile_file: Option<PathBuf>,
}
impl ProfileArgs {
    fn load(&self) -> Result<photo_privacy::Profile> {
        if let Some(f) = &self.profile_file {
            use std::io::Read;
            let mut b = vec![];
            std::fs::File::open(f)?
                .take(photo_privacy::profiles::MAX_PROFILE_BYTES as u64 + 1)
                .read_to_end(&mut b)?;
            return photo_privacy::Profile::from_json(&b);
        }
        photo_privacy::Profile::builtin(&self.profile).with_context(|| {
            format!(
                "Unknown profile. Built-in: {}",
                photo_privacy::profiles::BUILTIN.join(", ")
            )
        })
    }
}
#[derive(clap::Args, Clone, Default)]
struct EditArgs {
    /// Edit recipe JSON (a list of operations), applied before the flags below.
    #[arg(long)]
    recipe: Option<PathBuf>,
    /// Centered crop to an aspect ratio: original, 1:1, 4:3, 3:2, 16:9, 9:16, 4:5 or W:H.
    #[arg(long)]
    aspect: Option<String>,
    /// Crop rectangle x,y,width,height.
    #[arg(long)]
    crop: Option<String>,
    /// Rotate clockwise by 90, 180 or 270 degrees.
    #[arg(long)]
    rotate: Option<u16>,
    /// Straighten by a small angle in degrees (-45..45).
    #[arg(long, allow_hyphen_values = true)]
    straighten: Option<f32>,
    #[arg(long)]
    flip_horizontal: bool,
    #[arg(long)]
    flip_vertical: bool,
    /// Fit within WIDTHxHEIGHT without upscaling, e.g. 1080x1350.
    #[arg(long)]
    fit: Option<String>,
    /// Opaque redaction rectangle x,y,width,height (repeatable).
    #[arg(long)]
    redact: Vec<String>,
    /// Opaque redaction color: black, white or #rrggbb.
    #[arg(long, default_value = "black")]
    redact_color: String,
    /// Gaussian blur rectangle x,y,width,height (visual concealment only; repeatable).
    #[arg(long)]
    blur: Vec<String>,
    /// Pixelate rectangle x,y,width,height (visual concealment only; repeatable).
    #[arg(long)]
    pixelate: Vec<String>,
}
fn rect(s: &str) -> Result<photo_edit::Rect> {
    let v: Vec<u32> = s
        .split(',')
        .map(|x| x.trim().parse())
        .collect::<Result<_, _>>()
        .context("Rectangle must be x,y,width,height")?;
    ensure!(
        v.len() == 4 && v[2] > 0 && v[3] > 0,
        "Rectangle must be x,y,width,height with non-zero size"
    );
    Ok(photo_edit::Rect::new(v[0], v[1], v[2], v[3]))
}
fn color(s: &str) -> Result<[u8; 3]> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        h if h.len() == 7 && h.starts_with('#') && h.is_ascii() => {
            let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16);
            [p(1)?, p(3)?, p(5)?]
        }
        _ => anyhow::bail!("Color must be black, white or #rrggbb"),
    })
}
impl EditArgs {
    fn operations(&self) -> Result<Vec<photo_edit::Operation>> {
        use photo_edit::{AspectRatio, Concealment, Operation, Selection};
        let mut ops = vec![];
        if let Some(r) = &self.recipe {
            use std::io::Read;
            let mut b = vec![];
            std::fs::File::open(r)?.take(1 << 20).read_to_end(&mut b)?;
            ensure!(b.len() < 1 << 20, "Recipe too large");
            ops = serde_json::from_slice(&b).context("Recipe must be a JSON list of operations")?;
        }
        if let Some(r) = &self.crop {
            ops.push(Operation::Crop(rect(r)?));
        }
        if let Some(a) = &self.aspect {
            let ratio = match a.as_str() {
                "original" => AspectRatio::Original,
                "1:1" => AspectRatio::Square,
                "4:3" => AspectRatio::R4x3,
                "3:2" => AspectRatio::R3x2,
                "16:9" => AspectRatio::R16x9,
                "9:16" => AspectRatio::R9x16,
                "4:5" => AspectRatio::R4x5,
                other => {
                    let (w, h) = other.split_once(':').context("Aspect must be W:H")?;
                    AspectRatio::Custom {
                        width: w.parse()?,
                        height: h.parse()?,
                    }
                }
            };
            ops.push(Operation::CropAspect(ratio));
        }
        match self.rotate {
            None | Some(0) => {}
            Some(90) => ops.push(Operation::Rotate90),
            Some(180) => ops.push(Operation::Rotate180),
            Some(270) => ops.push(Operation::Rotate270),
            Some(_) => anyhow::bail!("Rotate must be 90, 180 or 270"),
        }
        if let Some(d) = self.straighten {
            ops.push(Operation::Straighten { degrees: d });
        }
        if self.flip_horizontal {
            ops.push(Operation::FlipHorizontal);
        }
        if self.flip_vertical {
            ops.push(Operation::FlipVertical);
        }
        if let Some(f) = &self.fit {
            let (w, h) = f.split_once('x').context("Fit must be WIDTHxHEIGHT")?;
            ops.push(Operation::FitWithin {
                max_width: w.parse()?,
                max_height: h.parse()?,
            });
        }
        let rgb = color(&self.redact_color)?;
        for (list, method) in [
            (&self.blur, Concealment::GaussianBlur { sigma: 12. }),
            (&self.pixelate, Concealment::Pixelate { block: 16 }),
            (&self.redact, Concealment::Solid { rgb }),
        ] {
            for r in list {
                ops.push(Operation::Conceal {
                    selection: Selection::Rect(rect(r)?),
                    method,
                    invert: false,
                });
            }
        }
        Ok(ops)
    }
}
fn main() -> Result<()> {
    match Args::parse().command {
        Command::Inspect { input } => {
            let p = load(&input)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"width":p.pixels.width(),"height":p.pixels.height(),"format":format!("{:?}",p.format),"fingerprint":p.fingerprint,"ai_protection":"not evaluated","color_pipeline":"8-bit; source ICC not preserved"})
                )?
            );
        }
        Command::Export {
            inputs,
            output_dir,
            format,
            quality: jpeg_quality,
        } => {
            ensure!(output_dir.is_dir(), "Output directory must already exist");
            let format = ExportFormat::from(format);
            let mut errors = 0;
            for input in inputs {
                match export_one(&input, &output_dir, format, jpeg_quality) {
                    Ok(path) => println!("Exported {}", path.display()),
                    Err(e) => {
                        errors += 1;
                        eprintln!("{}: {e:#}", input.display());
                    }
                }
            }
            ensure!(
                errors == 0,
                "{errors} export(s) failed; successful copies preserved"
            );
        }
        Command::Compare {
            original,
            candidate,
            difference,
        } => {
            let a = load(&original)?;
            let b = load(&candidate)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&quality(&a.pixels, &b.pixels)?)?
            );
            if let Some(path) = difference {
                save_new(
                    &path,
                    &encode(
                        &photo_core::difference(&a.pixels, &b.pixels, 8)?,
                        ExportFormat::Png,
                        95,
                    )?,
                )?;
            }
        }
        Command::Lab {
            original,
            candidate,
            report,
        } => {
            let a = load(&original)?;
            let b = load(&candidate)?;
            let r = photo_lab::evaluate(&a.pixels, &b.pixels)?;
            save_new(&report, &serde_json::to_vec_pretty(&r)?)?;
        }
        Command::VerifyModel { model, manifest } => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&manifest)
                .context("Read manifest")?
                .take(64 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 64 * 1024, "Manifest too large");
            let m = serde_json::from_slice(&bytes)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&photo_models::verify(&model, &m)?)?
            );
        }
        #[cfg(feature = "experimental-clip")]
        Command::ExperimentalClip {
            input,
            model,
            manifest,
            output,
            epsilon,
            steps,
        } => experimental_clip(&input, &model, &manifest, &output, epsilon, steps)?,
        Command::Scan {
            input,
            profile,
            text,
        } => {
            let photo = load(&input)?;
            let spans = text
                .into_iter()
                .map(|t| photo_privacy::TextSpan {
                    text: t,
                    region: None,
                    source: photo_privacy::Source::Text,
                })
                .collect();
            let r = photo_privacy::scan(
                photo_privacy::ScanInput {
                    bytes: &photo.bytes,
                    pixels: &photo.pixels,
                    ocr_text: None,
                    extra_text: spans,
                },
                &profile.load()?,
            );
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        Command::Process {
            input,
            output_dir,
            edit,
            profile,
            format,
            quality: q,
            auto_redact,
            accept_unresolved,
        } => {
            let profile = profile.load()?;
            let photo = load(&input)?;
            let mut edits = photo_edit::EditStack::from_operations(edit.operations()?)?;
            if auto_redact {
                let staged = photo.with_pixels(edits.render(&photo.pixels)?);
                let found = photo_pipeline::scan_photo(&staged, &profile).findings;
                if let Some(op) = photo_pipeline::conceal_findings(&found, profile.redaction, 4) {
                    edits.push(op)?;
                }
            }
            let format = ExportFormat::from(format);
            let prepared = photo_pipeline::prepare(&photo, &edits, format, q, &profile)?;
            let v = &prepared.report.verification;
            eprintln!(
                "Verification: {} | metadata findings {} | remaining findings {} | redactions checked {} | AI protection: not applied",
                if v.passed { "passed" } else { "FAILED" },
                v.metadata_findings.len(),
                v.remaining_findings.len(),
                v.redactions.len()
            );
            for w in &v.warnings {
                eprintln!("Warning: {w}");
            }
            let saved =
                photo_pipeline::publish(&prepared, &input, &output_dir, format, accept_unresolved)?;
            println!("{}", serde_json::to_string_pretty(&saved)?);
        }
        Command::Batch {
            inputs,
            output_dir,
            recursive,
            edit,
            profile,
            format,
            quality: q,
            auto_redact,
            accept_unresolved,
            keep_duplicates,
            watch,
            summary,
        } => {
            use photo_pipeline::batch;
            let opts = batch::BatchOptions {
                output_dir: output_dir.clone(),
                format: format.into(),
                quality: q,
                profile: profile.load()?,
                edits: edit.operations()?,
                auto_redact,
                accept_unresolved,
                skip_duplicates: !keep_duplicates,
            };
            let collect = batch::CollectOptions {
                recursive,
                exclude: vec![output_dir.clone()],
                ..Default::default()
            };
            let control = batch::Control::default();
            let print = |_: usize, p: &Path, o: &batch::Outcome| match o {
                batch::Outcome::Exported {
                    image,
                    verification_passed,
                } => println!(
                    "{} -> {} (verification {})",
                    p.display(),
                    image.display(),
                    if *verification_passed {
                        "passed"
                    } else {
                        "accepted with findings"
                    }
                ),
                batch::Outcome::NeedsReview { findings } => println!(
                    "{}: REVIEW {} located finding(s); not exported",
                    p.display(),
                    findings.len()
                ),
                batch::Outcome::Duplicate { of } => {
                    println!("{}: duplicate of {}", p.display(), of.display())
                }
                batch::Outcome::Failed { error } => eprintln!("{}: {error}", p.display()),
                batch::Outcome::Cancelled => {}
            };
            if let Some(secs) = watch {
                ensure!(
                    (1..=86_400).contains(&secs),
                    "Watch interval must be 1..86400 seconds"
                );
                let mut monitor = batch::FolderMonitor::default();
                eprintln!("Watching for new photographs every {secs}s. Press Ctrl+C to stop.");
                loop {
                    let fresh = monitor.poll(&inputs, &collect)?;
                    if !fresh.is_empty() {
                        batch::run(&fresh, &opts, &control, print)?;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(secs));
                }
            }
            let c = batch::collect(&inputs, &collect)?;
            for (p, why) in &c.skipped {
                eprintln!("Skipped {}: {why}", p.display());
            }
            let s = batch::run(&c.files, &opts, &control, print)?;
            eprintln!(
                "Batch: {} exported, {} need review, {} duplicates, {} failed",
                s.exported, s.review, s.duplicates, s.failed
            );
            if let Some(path) = summary {
                save_new(&path, &serde_json::to_vec_pretty(&s)?)?;
            }
            ensure!(
                s.failed == 0,
                "{} file(s) failed; other results were preserved",
                s.failed
            );
        }
        Command::Forensics { input } => {
            let p = load(&input)?;
            eprintln!(
                "Sensitive: this report contains file metadata values. Do not share it without review."
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&photo_privacy::forensics::inspect(&p.bytes))?
            );
        }
        Command::RawPreview { input, output } => {
            use std::io::Read;
            let mut b = vec![];
            std::fs::File::open(&input)?
                .take(photo_core::raw::MAX_RAW_BYTES + 1)
                .read_to_end(&mut b)?;
            let ext = input.extension().and_then(|e| e.to_str());
            let p = photo_core::raw::extract_preview(&b, ext)?;
            save_new(&output, &p.jpeg)?;
            println!(
                "{:?} preview: {} bytes ({} candidates). Note: embedded camera JPEG, not RAW development; it may contain EXIF.",
                p.kind,
                p.jpeg.len(),
                p.candidates
            );
        }
        Command::WatermarkEmbed {
            input,
            output,
            payload,
            key_env,
            format,
            quality,
            strength,
        } => {
            ensure!(payload < 1 << 48, "Payload must be below 2^48");
            let key = watermark_key(&key_env)?;
            let photo = load(&input)?;
            let mut rgb = photo.pixels.to_rgb8();
            photo_edit::watermark::embed(&mut rgb, key, payload, strength)?;
            let img = image::DynamicImage::ImageRgb8(rgb);
            let bytes = encode(&img, format.into(), quality)?;
            let check =
                photo_edit::watermark::detect(&image::load_from_memory(&bytes)?.to_rgb8(), key)?;
            ensure!(
                check.payload == Some(payload),
                "Watermark did not survive encoding at these settings; raise quality or strength"
            );
            save_new(&output, &bytes)?;
            println!(
                "Embedded and verified after encoding (confidence {:.3}). Not AI protection; does not survive resizing or cropping.",
                check.confidence
            );
        }
        Command::WatermarkDetect { input, key_env } => {
            let key = watermark_key(&key_env)?;
            let photo = load(&input)?;
            let d = photo_edit::watermark::detect(&photo.pixels.to_rgb8(), key)?;
            println!("{}", serde_json::to_string_pretty(&d)?);
            if d.payload.is_none() {
                eprintln!(
                    "No watermark for this passphrase (or it was removed, resized or cropped)."
                );
            }
        }
        Command::MediaInspect { input } => {
            let b = read_media(&input)?;
            let det = photo_media::detect(&b);
            let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("");
            if photo_media::detect::extension_mismatch(&det, ext) {
                eprintln!(
                    "Warning: the .{ext} extension does not match the detected {} content",
                    det.container
                );
            }
            let info = photo_media::inspect(&b)?;
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
        Command::StripVideo { input, output } => {
            let b = read_media(&input)?;
            let (out, report) = photo_media::mp4::strip(&b)?;
            save_new(&output, &out)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::AudioAnalyze { input } => {
            let mut a = photo_media::audio::analyze(&input)?;
            a.waveform.clear();
            a.spectrogram.clear();
            println!("{}", serde_json::to_string_pretty(&a)?);
        }
        Command::AudioCompare {
            original,
            processed,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&photo_media::audio::compare(&original, &processed)?)?
            );
        }
        #[cfg(feature = "video")]
        Command::VideoRedact {
            input,
            output,
            rect,
            from,
            to,
            blur,
        } => {
            let parts: Vec<u32> = rect
                .split(',')
                .map(|s| s.trim().parse::<u32>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("--rect must be x,y,w,h")?;
            anyhow::ensure!(parts.len() == 4, "--rect must have 4 integers: x,y,w,h");
            let r = photo_media::video::RedactRect {
                x: parts[0],
                y: parts[1],
                width: parts[2],
                height: parts[3],
                from_s: from,
                to_s: to,
                blur,
            };
            let report = photo_media::video::redact_mp4(&input, &output, &[r])?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        #[cfg(feature = "video")]
        Command::VideoTrack {
            input,
            output,
            rect,
            at,
            blur,
            accept_tracking_gaps,
        } => {
            let parts: Vec<u32> = rect
                .split(',')
                .map(|s| s.trim().parse::<u32>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("--rect must be x,y,w,h")?;
            anyhow::ensure!(parts.len() == 4, "--rect must have 4 integers: x,y,w,h");
            let kf = photo_media::video_track::Keyframe {
                time_s: at,
                x: parts[0],
                y: parts[1],
                width: parts[2],
                height: parts[3],
            };
            let (tracked, rects) =
                photo_media::video_track::track_object_in_mp4(&input, &[kf], blur)?;
            let review_frames = tracked.iter().filter(|b| {
                !matches!(
                    b.status,
                    photo_media::video_track::TrackStatus::Seed
                        | photo_media::video_track::TrackStatus::Tracked
                )
            }).count();
            anyhow::ensure!(
                review_frames == 0 || accept_tracking_gaps,
                "Tracking is uncertain in {review_frames} frames. Review coverage before export; --accept-tracking-gaps explicitly accepts this limitation"
            );
            let redact = photo_media::video::redact_mp4(&input, &output, &rects)?;
            let verify = photo_media::video_track::verify_redacted_mp4(&output, &rects)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "tracked_frames": tracked.len(),
                    "frames_requiring_review": review_frames,
                    "tracking": tracked,
                    "coverage_verified": false,
                    "redact": redact,
                    "frame_verify": verify,
                }))?
            );
        }
        Command::AudioExport {
            input,
            output,
            mute,
            bleep,
        } => {
            let mut segs = vec![];
            for (list, kind) in [
                (&mute, photo_media::audio::RedactKind::Mute),
                (&bleep, photo_media::audio::RedactKind::Bleep),
            ] {
                for r in list {
                    let (a, b) = r.split_once('-').context("Ranges must look like 12.5-14")?;
                    segs.push(photo_media::audio::Segment {
                        start_s: a.trim().parse()?,
                        end_s: b.trim().parse()?,
                        kind,
                    });
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&photo_media::audio::export_wav(
                    &input, &output, &segs
                )?)?
            );
        }
        Command::AudioProtect {
            input,
            output,
            model,
            profile,
        } => {
            let Some(model_dir) = model else {
                anyhow::bail!(
                    "No validated voice-protection model is installed. The audio protection engine refuses to run without one; no file was changed."
                );
            };
            #[cfg(feature = "voice-protect")]
            {
                let out = output.context("Missing -o/--output WAV path")?;
                let settings = photo_media::protect::Settings::profile(&profile)
                    .with_context(|| format!("Unknown audio protection profile: {profile}"))?;
                let cancel = std::sync::atomic::AtomicBool::new(false);
                let report =
                    photo_media::ecapa::protect_wav(&input, &out, &model_dir, settings, &cancel)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            #[cfg(not(feature = "voice-protect"))]
            {
                let _ = (input, output, model_dir, profile);
                anyhow::bail!(
                    "Built without the `voice-protect` feature; rebuild with `--features voice-protect`."
                );
            }
        }
        #[cfg(feature = "stt")]
        Command::AudioTranscribe {
            input,
            model,
            redact_to,
        } => {
            let segments = photo_media::stt::transcribe(&input, &model)?;
            let bleeps = photo_media::stt::find_private(&segments, &photo_privacy::Country::ALL);
            let export = if let Some(out) = redact_to {
                Some(photo_media::audio::export_wav(&input, &out, &bleeps)?)
            } else {
                None
            };
            let segment_timings: Vec<serde_json::Value> = segments
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "start_s": s.start_s,
                        "end_s": s.end_s,
                    })
                })
                .collect();
            let report = serde_json::json!({
                "segment_count": segments.len(),
                "segments": segment_timings,
                "bleep_ranges": bleeps,
                "export": export,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        #[cfg(feature = "stt")]
        Command::VoiceSafePlayback {
            input,
            output,
            model,
            mode,
            custom,
        } => {
            let supp_mode = match mode.to_ascii_lowercase().as_str() {
                "mute" => photo_media::wake_word::SuppressionMode::Mute,
                "bleep" => photo_media::wake_word::SuppressionMode::Bleep,
                "experimental-band" => {
                    photo_media::wake_word::SuppressionMode::ExperimentalBandSuppress
                }
                other => anyhow::bail!(
                    "Unknown mode '{other}'; expected mute, bleep, or experimental-band"
                ),
            };
            let custom_phrases: Vec<String> = custom
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let hits = photo_media::wake_word::detect_wake_phrases_with_whisper(
                &input,
                &model,
                &photo_media::wake_word::AssistantKind::BUILTINS,
                &custom_phrases,
            )?;
            let rep =
                photo_media::wake_word::apply_safe_playback(&input, &output, &hits, supp_mode)?;
            println!("{}", serde_json::to_string_pretty(&rep)?);
        }
        Command::Subtitles { input, redact_to } => {
            let b = read_media(&input)?;
            let text = String::from_utf8_lossy(&b);
            let cues = photo_media::subtitles::parse(&text)?;
            let found = photo_media::subtitles::scan(&cues, &photo_privacy::Country::ALL);
            println!("{}", serde_json::to_string_pretty(&found)?);
            if let Some(out) = redact_to {
                let flagged: Vec<usize> = found.iter().map(|f| f.cue).collect();
                let vtt = photo_media::detect(&b).container == "WebVTT";
                save_new(
                    &out,
                    photo_media::subtitles::write(&cues, &flagged, vtt).as_bytes(),
                )?;
                eprintln!(
                    "Wrote redacted subtitles ({} cue(s) replaced)",
                    flagged
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                );
            }
        }
        Command::Profiles { name } => match name {
            None => {
                for p in photo_privacy::Profile::all() {
                    println!("{:<16} {}", p.name, p.description);
                }
            }
            Some(n) => {
                let p = photo_privacy::Profile::builtin(&n).context("Unknown profile")?;
                println!("{}", String::from_utf8(p.to_json()?)?);
            }
        },
        Command::Protect { input: _ } => anyhow::bail!(
            "No validated defense model adapter is installed. No image was changed. Use export for metadata removal; it does not block AI."
        ),
    }
    Ok(())
}
fn watermark_key(var: &str) -> Result<u64> {
    let p = std::env::var(var)
        .with_context(|| format!("Set the passphrase in the environment variable {var}"))?;
    ensure!(
        p.chars().count() >= 8,
        "Use a watermark passphrase of at least 8 characters"
    );
    Ok(photo_edit::watermark::key_from_passphrase(&p))
}
/// Bounded read of a media file (regular files only).
fn read_media(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink(),
        "Input must be a regular file"
    );
    ensure!(
        m.len() <= 2 * 1024 * 1024 * 1024,
        "File exceeds the 2 GiB inspection limit"
    );
    let mut b = vec![];
    std::fs::File::open(path)?
        .take(m.len() + 1)
        .read_to_end(&mut b)?;
    Ok(b)
}
fn export_one(input: &Path, dir: &Path, format: ExportFormat, q: u8) -> Result<PathBuf> {
    let p = load(input)?;
    let path = dir.join(format!(
        "{}-private.{}",
        safe_stem(input)?,
        format.extension()
    ));
    let bytes = encode(&p.pixels, format, q)?;
    let decoded = photo_core::decode_export(&bytes)?;
    let metrics = quality(&p.pixels, &decoded)?;
    save_new(&path, &bytes)?;
    // Report publication failure is surfaced while retaining the successfully exported image.
    let report = serde_json::json!({"schema_version":1,"operation":"metadata-free re-encode","ai_efficacy":"not evaluated","source_hash":p.fingerprint,"output_hash":photo_core::fingerprint(&bytes),"quality":metrics,"warnings":["8-bit pixels; source ICC profile is not preserved","Only the first image/frame is exported","Quality metrics do not prove AI resistance"]});
    let report_path = path.with_extension(format!("{}.json", format.extension()));
    save_new(&report_path, &serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("Image saved at {}, but report failed", path.display()))?;
    Ok(path)
}

#[cfg(feature = "experimental-clip")]
fn experimental_clip(
    input: &Path,
    model: &Path,
    manifest: &Path,
    output: &Path,
    epsilon: u8,
    steps: u32,
) -> Result<()> {
    use photo_engine::{Objective, Settings, WeightedObjective};
    use std::io::Read;
    use std::sync::atomic::AtomicBool;
    let settings = Settings {
        epsilon: f32::from(epsilon) / 255.,
        steps,
        ..Settings::default()
    };
    settings.validate()?;
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png")),
        "Experimental output must use .png"
    );
    let photo = load(input)?;
    ensure!(
        u64::from(photo.pixels.width()) * u64::from(photo.pixels.height()) <= 1_048_576,
        "Experimental input limit is 1 megapixel"
    );
    ensure!(
        photo.pixels.to_rgba8().pixels().all(|p| p[3] == 255),
        "Experimental CLIP currently requires an opaque image"
    );
    let mut manifest_bytes = Vec::new();
    std::fs::File::open(manifest)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut manifest_bytes)?;
    ensure!(manifest_bytes.len() <= 64 * 1024, "Manifest too large");
    let m: photo_models::Manifest = serde_json::from_slice(&manifest_bytes)?;
    let verified = photo_models::verify(model, &m)?;
    let mut bytes = Vec::new();
    std::fs::File::open(model)?
        .take(m.size_bytes + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == m.size_bytes
            && photo_core::fingerprint(&bytes)
                .sha256
                .eq_ignore_ascii_case(&m.sha256),
        "Model changed after verification"
    );
    let objective = photo_engine::clip::ClipObjective::from_bytes(
        bytes,
        photo.pixels.width() as usize,
        photo.pixels.height() as usize,
    )?;
    let original: Vec<f32> = photo
        .pixels
        .to_rgb8()
        .as_raw()
        .iter()
        .map(|v| f32::from(*v) / 255.)
        .collect();
    let energy_before = objective.feature_energy(&original)?;
    let result = photo_engine::optimize(
        &original,
        &[WeightedObjective {
            objective: &objective,
            weight: 1.,
        }],
        settings,
        &AtomicBool::new(false),
    )?;
    // Quantize within the integer pixel budget and measure the actual exported pixels.
    let rgb: Vec<u8> = result
        .pixels
        .iter()
        .map(|v| (v * 255.).round().clamp(0., 255.) as u8)
        .collect();
    let actual: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.).collect();
    let energy_after = objective.feature_energy(&actual)?;
    let out = image::DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(photo.pixels.width(), photo.pixels.height(), rgb)
            .context("Output shape")?,
    );
    let bytes = encode(&out, ExportFormat::Png, 95)?;
    let q = quality(&photo.pixels, &out)?;
    ensure!(
        q.max_channel_delta <= epsilon,
        "Quantized output exceeded perturbation budget"
    );
    let report = serde_json::json!({"schema_version":1,"operation":"experimental CLIP feature suppression","objective":objective.name(),"settings":settings,"model":verified,"source_hash":photo.fingerprint,"output_hash":photo_core::fingerprint(&bytes),"quality":q,"feature_energy_before":energy_before,"feature_energy_after":energy_after,"loss_trace":result.trace,"ai_efficacy":"not evaluated","warnings":["Encoder feature change does not demonstrate resistance to downstream AI editing, training, recognition, or face swapping","Bilinear resize differs from canonical CLIP bicubic center crop preprocessing","Not a reproduction of any audited research defense"]});
    save_new(output, &bytes)?;
    save_new(
        &output.with_extension("png.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Experimental output saved. CLIP feature energy: {energy_before:.6} -> {energy_after:.6}. AI efficacy: not evaluated."
    );
    Ok(())
}

