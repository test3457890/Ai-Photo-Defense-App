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
        Command::Protect { input: _ } => anyhow::bail!(
            "No validated defense model adapter is installed. No image was changed. Use export for metadata removal; it does not block AI."
        ),
    }
    Ok(())
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
