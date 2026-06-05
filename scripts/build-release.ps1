param(
    [switch]$SkipBuild,
    [switch]$ZipOnly,
    [switch]$SetupOnly
)

$ErrorActionPreference = 'Stop'

$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$DistDir = Join-Path $ProjectRoot 'dist'
$ExePath = Join-Path $ProjectRoot 'target\release\claude-code-helper.exe'
$ReadmeSrc = Join-Path $ProjectRoot 'installer\USAGE-zh-CN.txt'
$IssPath = Join-Path $ProjectRoot 'installer\ClaudeCodeHelper.iss'

function Get-ProjectVersion {
    $cargo = Get-Content (Join-Path $ProjectRoot 'Cargo.toml') -Raw
    if ($cargo -match '(?m)^version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    throw 'Cannot read version from Cargo.toml'
}

function Find-Iscc {
    @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}

$Version = Get-ProjectVersion
Write-Host "Claude Code Helper v$Version"

if (-not $SkipBuild) {
    Write-Host 'Building release...'
    Push-Location $ProjectRoot
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $ExePath)) {
    throw "Missing release binary: $ExePath`nRun: cargo build --release"
}

New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
$built = @()

if (-not $SetupOnly) {
    Write-Host 'Packaging ZIP...'
    $staging = Join-Path $env:TEMP "claude-code-helper-pack-$Version"
    if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    Copy-Item $ExePath (Join-Path $staging 'claude-code-helper.exe') -Force
    if (-not (Test-Path $ReadmeSrc)) {
        throw "Missing $ReadmeSrc"
    }
    Copy-Item $ReadmeSrc (Join-Path $staging 'USAGE-zh-CN.txt') -Force

    $zipPath = Join-Path $DistDir "ClaudeCodeHelper-$Version-win64.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zipPath -CompressionLevel Optimal
    Remove-Item $staging -Recurse -Force -ErrorAction SilentlyContinue
    $built += $zipPath
    Write-Host "  OK  $zipPath"
}

if (-not $ZipOnly) {
    $iscc = Find-Iscc
    if (-not $iscc) {
        throw @"
Inno Setup 6 (ISCC.exe) not found.
Install from: https://jrsoftware.org/isinfo.php
Expected paths:
  $env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe
  ${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe
"@
    }

    Write-Host "Building Setup with: $iscc"
    & $iscc "/DMyAppVersion=$Version" $IssPath
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }

    $setupPath = Join-Path $DistDir "ClaudeCodeHelper-$Version-Setup.exe"
    if (-not (Test-Path $setupPath)) {
        throw "Setup exe not found after compile: $setupPath"
    }
    $built += $setupPath
    Write-Host "  OK  $setupPath"
}

Write-Host ''
Write-Host 'Done. Artifacts:'
foreach ($item in $built) {
    $sizeMb = [math]::Round((Get-Item $item).Length / 1MB, 2)
    Write-Host "  $item  ($sizeMb MB)"
}
