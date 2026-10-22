param(
    [switch]$Installer
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    Write-Host '> cargo build --release'
    cargo build --release -p bongocat
    if ($LASTEXITCODE -ne 0) { throw 'build failed' }

    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item 'target\release\bongocat.exe' 'dist\bongocat.exe' -Force
    $mb = (Get-Item 'dist\bongocat.exe').Length / 1MB
    Write-Host ("exe  : {0:N2} MB" -f $mb)

    $makensis = @(
        'C:\Program Files (x86)\NSIS\makensis.exe',
        'C:\Program Files\NSIS\makensis.exe'
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if ($Installer -or $makensis) {
        if (-not $makensis) {
            Write-Warning 'NSIS not found. Install it, or run without -Installer for a portable zip.'
        } else {
            & $makensis 'installer\bongocat.nsi'
            if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
            Write-Host ("setup: {0:N2} MB" -f ((Get-Item 'dist\BongoWhale-Setup.exe').Length / 1MB))
        }
    }

    if (-not $makensis) {
        if (Test-Path 'dist\BongoWhale-portable.zip') { Remove-Item 'dist\BongoWhale-portable.zip' }
        Compress-Archive -Path 'dist\bongocat.exe' -DestinationPath 'dist\BongoWhale-portable.zip'
        Write-Host 'portable zip created'
    }
} finally {
    Pop-Location
}
