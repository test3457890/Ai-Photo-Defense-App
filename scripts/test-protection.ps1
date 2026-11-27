<#
.SYNOPSIS
    AI Photo Defense - End-to-End Verification & Gemini/ChatGPT Test Helper Script

.DESCRIPTION
    Tests whether AI Photo Defense works on a given photograph:
    1. Inspects the original image (dimensions, format, hashes).
    2. Runs Privacy Scan (metadata, QR/barcodes, credentials).
    3. Exports a metadata-free copy and verifies no metadata remains.
    4. Runs the 23-transform Real-World Durability Suite.
    5. Runs the Validation Suite (quality metrics + any configured local AI models).
    6. Optionally runs hardened CLIP protection if -ClipModel and -ClipManifest are provided.

.EXAMPLE
    .\scripts\test-protection.ps1
    .\scripts\test-protection.ps1 -InputPhoto "C:\Users\brett\Pictures\test.jpg"
    .\scripts\test-protection.ps1 -InputPhoto "C:\Users\brett\Pictures\test.jpg" -FaceModel "C:\models\version-RFB-320.onnx"
#>
param(
    [string]$InputPhoto = "",
    [string]$OutputDir = "",
    [string]$FaceModel = "",
    [string]$ClipModel = "",
    [string]$ClipManifest = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

$env:CARGO_TARGET_DIR = "C:\Users\brett\apd-target"

if ([string]::IsNullOrWhiteSpace($InputPhoto)) {
    $InputPhoto = Join-Path $RepoRoot "tests\fixtures\calibration.png"
}
if (-not (Test-Path $InputPhoto)) {
    throw "Input photo not found: $InputPhoto"
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutputDir = Join-Path $env:USERPROFILE "Desktop\APD-Test-$stamp"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$CliExe = Join-Path $env:CARGO_TARGET_DIR "release\photo-defense.exe"
if (-not (Test-Path $CliExe)) {
    $CliExe = Join-Path $env:CARGO_TARGET_DIR "debug\photo-defense.exe"
}
if (-not (Test-Path $CliExe)) {
    Write-Host "Building photo-defense CLI (with all features)..." -ForegroundColor Cyan
    cargo build --locked -p photo-cli --all-features
    if ($LASTEXITCODE -ne 0) { throw "CLI build failed" }
    $CliExe = Join-Path $env:CARGO_TARGET_DIR "debug\photo-defense.exe"
}

Write-Host "`n=== AI Photo Defense Test Run ===" -ForegroundColor Green
Write-Host "CLI:    $CliExe"
Write-Host "Input:  $InputPhoto"
Write-Host "Output: $OutputDir`n"

Write-Host "[1/5] Inspecting original image..." -ForegroundColor Cyan
& $CliExe inspect $InputPhoto | Out-File -FilePath (Join-Path $OutputDir "1-inspect.json") -Encoding utf8

Write-Host "[2/5] Running Privacy Scan on original..." -ForegroundColor Cyan
& $CliExe scan $InputPhoto --profile "Maximum Privacy" | Out-File -FilePath (Join-Path $OutputDir "2-scan-original.json") -Encoding utf8

Write-Host "[3/5] Exporting metadata-free copy..." -ForegroundColor Cyan
& $CliExe export $InputPhoto --output-dir $OutputDir --format png
$ExportedFile = Get-ChildItem -Path $OutputDir -Filter "*-private.png" | Select-Object -First 1
if (-not $ExportedFile) {
    throw "Exported file not found in $OutputDir"
}

Write-Host "[4/5] Re-scanning exported copy to verify metadata removal..." -ForegroundColor Cyan
& $CliExe scan $ExportedFile.FullName --profile "Maximum Privacy" | Out-File -FilePath (Join-Path $OutputDir "3-scan-exported.json") -Encoding utf8

$CandidateForValidation = $ExportedFile.FullName

if (-not [string]::IsNullOrWhiteSpace($ClipModel) -and -not [string]::IsNullOrWhiteSpace($ClipManifest)) {
    Write-Host "[4b] Running Hardened CLIP Protection (--hardened)..." -ForegroundColor Cyan
    $HardenedOut = Join-Path $OutputDir "protected-hardened.png"
    & $CliExe experimental-clip $ExportedFile.FullName $ClipModel $ClipManifest -o $HardenedOut --epsilon 8 --steps 40 --hardened
    $CandidateForValidation = $HardenedOut
}

Write-Host "[5/5] Running Durability & Validation Suite..." -ForegroundColor Cyan
$DurabilityJson = Join-Path $OutputDir "4-durability.json"
$ValidationJson = Join-Path $OutputDir "5-validation.json"
$ValidationMd   = Join-Path $OutputDir "5-validation.md"

& $CliExe durability $InputPhoto $CandidateForValidation --report $DurabilityJson

$valArgs = @("validate", $InputPhoto, $CandidateForValidation, "--mode", "comprehensive", "--report", $ValidationJson, "--markdown", $ValidationMd)
if (-not [string]::IsNullOrWhiteSpace($FaceModel)) {
    $valArgs += @("--face-model", $FaceModel)
}
if (-not [string]::IsNullOrWhiteSpace($ClipModel) -and -not [string]::IsNullOrWhiteSpace($ClipManifest)) {
    $valArgs += @("--clip-model", $ClipModel, "--clip-manifest", $ClipManifest)
}
& $CliExe @valArgs

Write-Host "`n=== Test Complete ===" -ForegroundColor Green
Write-Host "Exported image to upload to ChatGPT / Gemini for comparison:" -ForegroundColor Yellow
Write-Host "  Original:  $InputPhoto"
Write-Host "  Protected: $CandidateForValidation"
Write-Host "  Reports:   $OutputDir"
Write-Host "`nSuggested ChatGPT / Gemini test:" -ForegroundColor Cyan
Write-Host "  1. Upload BOTH the Original and Protected image in separate chats."
Write-Host "  2. Ask: 'What metadata/location clues can you infer? Can you recreate or edit this person/scene?'"
Write-Host "  3. Compare whether metadata/QR findings were stripped and whether reconstruction still succeeds."
