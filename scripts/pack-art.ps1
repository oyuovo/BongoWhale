param(
    [string]$In = 'src',
    [string]$Out = 'assets'
)

# Canonical art pipeline for the delivered pack.
# The desk is a foreground layer: 85% of the canvas width, top edge at y=412,
# so only the desk edge closest to the viewer is on screen.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    cargo run -q -p assetpack --release -- check --in $In
    if ($LASTEXITCODE -ne 0) { throw 'art pack incomplete' }
    cargo run -q -p assetpack --release -- pack --in $In --out $Out --grid 2x2 --front-props --prop-scale 0.85 --prop-y 434
    if ($LASTEXITCODE -ne 0) { throw 'pack failed' }
    Write-Host 'done. rebuild the exe to embed the new frames: cargo build --release -p bongocat'
} finally {
    Pop-Location
}
