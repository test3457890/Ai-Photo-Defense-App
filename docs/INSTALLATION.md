# Installation and platform verification

This is a source-build development preview. No signed installers or verified public binaries exist yet.

| Platform | Target | Verification requirement |
|---|---|---|
| Windows 10/11 x64 | x86_64-pc-windows-msvc | CI MSVC build passed; Windows 10/11 GUI runtime smoke, file dialogs and signing pending |
| macOS Intel | x86_64-apple-darwin | CI Intel target build passed; Intel Mac GUI runtime smoke and notarization pending |
| macOS Apple Silicon | aarch64-apple-darwin | CI Apple Silicon target build passed; Mac GUI runtime smoke and notarization pending |
| Linux x64 | x86_64-unknown-linux-gnu | CI and local builds passed; native preview/export/lab checked; broader desktop coverage pending |

Install the Rust toolchain through rustup. `rust-toolchain.toml` selects the exact version. Windows requires Visual Studio Build Tools with Desktop development with C++; macOS requires Xcode command line tools. Linux requires a C linker, X11 or Wayland desktop libraries, a usable system font, an xdg-desktop-portal backend for rfd dialogs, and ALSA development headers for local audio playback (Debian/Ubuntu: `sudo apt-get install libasound2-dev pkg-config`). A headless shell can run the CLI but cannot directly start a graphical window without a desktop display.

```sh
cargo build --locked -p photo-cli
cargo build --locked -p photo-desktop
cargo run --locked -p photo-desktop
```

GPU hardware is not required. The initial UI uses a CPU renderer; the optional CLIP experiment runs on CPU. Rust package downloads require internet for the first build. The resulting app performs no model downloads or update checks. Offline operation requires building dependencies and installing any manually reviewed model artifact beforehand.

The experimental model feature is separate:

```sh
cargo build --locked -p photo-cli --features experimental-clip
```

Read [the experimental model guide](EXPERIMENTAL_CLIP.md) before using it. No model is bundled. Do not download untrusted executable model formats or treat a matching user-provided checksum as proof of publisher identity.
