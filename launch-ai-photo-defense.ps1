<#
.SYNOPSIS
    One-click launcher for AI Photo Defense after downloading/cloning from GitHub.
.DESCRIPTION
    1. Checks that Rust/Cargo is installed (or guides the user to rustup.rs).
    2. Sets a short CARGO_TARGET_DIR on Windows to avoid MAX_PATH linker errors.
    3. Builds photo-desktop and photo-cli (with faces, c2pa, video, opus, experimental-clip)
       if not already built or if -Rebuild is passed.
    4. Launches the AI Photo Defense desktop GUI.
#>
param(
    [switch]$Rebuild,
    [switch]$RunTest
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
Set-Location $RepoRoot

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Rust/Cargo is not installed or not on PATH." -ForegroundColor Red
    Write-Host "Install Rust from https://rustup.rs/ and re-run this launcher." -ForegroundColor Yellow
    exit 1
}

# Use a short target directory on Windows to prevent LNK1104 MAX_PATH failures
if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE "apd-target"
}

$DesktopExe = Join-Path $env:CARGO_TARGET_DIR "release\photo-desktop.exe"
$CliExe     = Join-Path $env:CARGO_TARGET_DIR "release\photo-defense.exe"

if ($Rebuild -or -not (Test-Path $DesktopExe) -or -not (Test-Path $CliExe)) {
    Write-Host "Building AI Photo Defense (release, full feature set)..." -ForegroundColor Cyan
    Write-Host "Target dir: $env:CARGO_TARGET_DIR"
    cargo build --release --locked -p photo-desktop --features faces,c2pa,video,opus,experimental-clip
    if ($LASTEXITCODE -ne 0) { throw "Desktop build failed" }
    cargo build --release --locked -p photo-cli --all-features
    if ($LASTEXITCODE -ne 0) { throw "CLI build failed" }
}

if ($RunTest) {
    & (Join-Path $RepoRoot "scripts\test-protection.ps1")
}

Write-Host "Launching AI Photo Defense..." -ForegroundColor Green
Start-Process -FilePath $DesktopExe -WorkingDirectory $RepoRoot
