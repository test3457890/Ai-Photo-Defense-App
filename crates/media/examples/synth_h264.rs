//! Writes a synthetic H.264 MP4 (coloured frames) for manual testing of the video tools:
//! `cargo run -p photo-media --features video --example synth_h264 -- OUT.mp4`.
fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).expect("usage: synth_h264 OUT.mp4");
    let frames: Vec<(u8, u8, u8)> = (0..90u32)
        .map(|i| ((i * 3) as u8, (200 - i) as u8, 90))
        .collect();
    photo_media::video::write_synthetic_h264_mp4(
        std::path::Path::new(&out),
        320,
        240,
        30,
        &frames,
    )?;
    println!("wrote {out}");
    Ok(())
}
