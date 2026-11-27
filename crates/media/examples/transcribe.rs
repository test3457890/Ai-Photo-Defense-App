//! Developer tool: prints a local Whisper transcript with timestamps, to check accuracy by ear.
//! `cargo run -p photo-media --features stt --example transcribe -- MODEL_DIR AUDIO`.
//! The application itself never prints or logs transcript text.
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let model = a.next().expect("usage: transcribe MODEL_DIR AUDIO");
    let audio = a.next().expect("usage: transcribe MODEL_DIR AUDIO");
    let t = std::time::Instant::now();
    let segs =
        photo_media::stt::transcribe(std::path::Path::new(&audio), std::path::Path::new(&model))?;
    for s in &segs {
        println!("[{:7.2} - {:7.2}] {}", s.start_s, s.end_s, s.text.trim());
    }
    eprintln!(
        "{} segment(s) in {:.1} s",
        segs.len(),
        t.elapsed().as_secs_f64()
    );
    Ok(())
}
