# Windows 本地发版：构建 ZIP + Setup → 打标签 → 推送 → 上传 GitHub Release
# 完整四平台产物请用 GitHub Actions：git push origin vX.Y.Z
param(
    [switch]$Retag,
    [switch]$SkipBuild,
    [switch]$AllowDirty,
    [string]$CommitMessage = ""
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $ProjectRoot

function Get-ProjectVersion {
    $cargo = Get-Content (Join-Path $ProjectRoot 'Cargo.toml') -Raw
    if ($cargo -match '(?m)^version\s*=\s*"([^"]+)"') { return $Matches[1] }
    throw 'Cannot read version from Cargo.toml'
}

function Find-GhCli {
    if (Get-Command gh -ErrorAction SilentlyContinue) {
        return (Get-Command gh).Source
    }
    $tempGh = Join-Path $env:TEMP 'gh-cli\bin\gh.exe'
    if (Test-Path $tempGh) { return $tempGh }
    throw @"
GitHub CLI (gh) not found.
Install to PATH, or download portable gh to: $tempGh
See RELEASE.md
"@
}

function Ensure-GhToken {
    if ($env:GH_TOKEN) { return }
    if ($env:GITHUB_TOKEN) { $env:GH_TOKEN = $env:GITHUB_TOKEN; return }
    $filled = "protocol=https`nhost=github.com`n" | git credential fill
    $line = $filled | Select-String '^password=' | Select-Object -First 1
    if (-not $line) { throw 'Cannot read GitHub token from git credential. Run: git push (login once) or set GH_TOKEN.' }
    $env:GH_TOKEN = $line.ToString().Split('=', 2)[1]
}

function Stop-HelperProcesses {
    foreach ($name in @('claude-code-helper.exe')) {
        Get-Process -Name ($name -replace '\.exe$','') -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 800
}

$Version = Get-ProjectVersion
$Tag = "v$Version"
$ZipName = "ClaudeCodeHelper-$Version-win64.zip"
$ZipPath = Join-Path $ProjectRoot "dist\$ZipName"
$SetupPath = Join-Path $ProjectRoot "dist\ClaudeCodeHelper-$Version-Setup.exe"
$NotesPath = Join-Path $ProjectRoot "dist\RELEASE_NOTES_$Tag.md"
$Repo = 'xqnode/claude-code-helper'

Write-Host "=== Claude Code Helper release $Tag ===" -ForegroundColor Cyan

$status = git status --porcelain
if ($status -and -not $AllowDirty) {
    throw "Working tree dirty. Commit first, or pass -AllowDirty.`n$status"
}

Stop-HelperProcesses

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-release.ps1')
}

if (-not (Test-Path $ZipPath)) {
    throw "Missing artifact: $ZipPath"
}
if (-not (Test-Path $SetupPath)) {
    throw "Missing artifact: $SetupPath`nInstall Inno Setup 6 and rerun, or see RELEASE.md."
}

if ($status -and $AllowDirty) {
    if (-not $CommitMessage) {
        $CommitMessage = "chore: release $Tag"
    }
    git add -A
    git commit -m $CommitMessage
}

if ($Retag) {
    git tag -d $Tag 2>$null
}

$existingTag = git tag -l $Tag
if (-not $existingTag) {
    git tag $Tag
}

git push origin main
if ($Retag -or -not $existingTag) {
    git push origin $Tag --force
} else {
    git push origin $Tag
}

$Gh = Find-GhCli
Ensure-GhToken
Write-Host "Using gh: $Gh"

$releaseExists = $false
try {
    & $Gh release view $Tag --repo $Repo 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $releaseExists = $true }
} catch {
    $releaseExists = $false
}

if (-not $releaseExists) {
    if (-not (Test-Path $NotesPath)) {
        throw "Release notes not found: $NotesPath`nCreate it before first release, or push tag to trigger GitHub Actions."
    }
    & $Gh release create $Tag $ZipPath $SetupPath `
        --repo $Repo `
        --title $Tag `
        --notes-file $NotesPath
} else {
    & $Gh release upload $Tag $ZipPath $SetupPath --repo $Repo --clobber
    if (Test-Path $NotesPath) {
        & $Gh release edit $Tag --repo $Repo --notes-file $NotesPath
    }
}

Write-Host ''
Write-Host "Done: https://github.com/$Repo/releases/tag/$Tag" -ForegroundColor Green
Write-Host "Artifacts:"
Write-Host "  $ZipPath ($([math]::Round((Get-Item $ZipPath).Length / 1MB, 2)) MB)"
Write-Host "  $SetupPath ($([math]::Round((Get-Item $SetupPath).Length / 1MB, 2)) MB)"
Write-Host ''
Write-Host 'For macOS .app.zip + DMG, push the same tag — GitHub Actions release.yml builds all platforms.' -ForegroundColor Yellow
