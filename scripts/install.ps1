<#
.SYNOPSIS
Installs the megane binary from GitHub Releases. No Rust toolchain needed.

.DESCRIPTION
Downloads the Windows archive for this machine's CPU, verifies its SHA-256, puts
megane.exe in the install directory and adds that directory to the user PATH.

  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\install.ps1 [-Version v0.1.0] [-InstallDir C:\tools\megane] [-NoPath]
  irm https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.ps1 | iex

Environment variables (overridden by the parameters):
  MEGANE_INSTALL_DIR  where megane.exe goes (default: %LOCALAPPDATA%\Programs\megane)
  MEGANE_VERSION      release tag to install (default: latest)
  MEGANE_REPO         GitHub repository (default: ShintaroOba/megane)
  MEGANE_BASE_URL     download the archive from here instead of GitHub (a mirror or file:/// URL)
  MEGANE_TARGET       force a target triple instead of detecting it (e.g. x86_64-pc-windows-msvc)

The download goes through the system proxy settings (Invoke-WebRequest).
#>
[CmdletBinding()]
param(
    [string]$Version = $(if ($env:MEGANE_VERSION) { $env:MEGANE_VERSION } else { 'latest' }),
    [string]$InstallDir = $(if ($env:MEGANE_INSTALL_DIR) { $env:MEGANE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\megane' }),
    [string]$Repo = $(if ($env:MEGANE_REPO) { $env:MEGANE_REPO } else { 'ShintaroOba/megane' }),
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {}

if ($env:MEGANE_TARGET) {
    $target = $env:MEGANE_TARGET
} else {
    $arch = $env:PROCESSOR_ARCHITEW6432
    if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
    switch ($arch) {
        'AMD64' { $target = 'x86_64-pc-windows-msvc' }
        'ARM64' { $target = 'aarch64-pc-windows-msvc' }
        default { throw "megane install: unsupported CPU architecture: $arch" }
    }
}
$asset = "megane-$target.zip"

if ($env:MEGANE_BASE_URL) {
    $base = $env:MEGANE_BASE_URL.TrimEnd('/')
} elseif ($Version -eq 'latest') {
    $base = "https://github.com/$Repo/releases/latest/download"
} else {
    $base = "https://github.com/$Repo/releases/download/$Version"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('megane-install-' + [Guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $asset
    Write-Host "Downloading $base/$asset"
    try {
        Invoke-WebRequest -Uri "$base/$asset" -OutFile $zip -UseBasicParsing
    } catch {
        throw "megane install: download failed (no release for $target at $base?): $($_.Exception.Message)"
    }

    $sumFile = "$zip.sha256"
    $haveSum = $true
    try {
        Invoke-WebRequest -Uri "$base/$asset.sha256" -OutFile $sumFile -UseBasicParsing
    } catch {
        $haveSum = $false
        Write-Warning "no checksum published for $asset, skipping verification"
    }
    if ($haveSum) {
        $expected = ((Get-Content $sumFile -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) {
            throw "megane install: checksum mismatch for $asset (expected $expected, got $actual)"
        }
    }

    $extract = Join-Path $tmp 'x'
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $exe = Get-ChildItem -Path $extract -Filter megane.exe -Recurse -File | Select-Object -First 1
    if (-not $exe) { throw 'megane install: the archive did not contain megane.exe' }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $InstallDir = (Resolve-Path $InstallDir).Path
    $dest = Join-Path $InstallDir 'megane.exe'
    $old = "$dest.old"
    # A running server locks its exe against overwrite but not against rename, so move the
    # old file aside, drop the new one in, and delete the old one when nothing holds it.
    Remove-Item $old -Force -ErrorAction SilentlyContinue
    if (Test-Path $dest) { Move-Item $dest $old -Force }
    Copy-Item $exe.FullName $dest -Force
    Remove-Item $old -Force -ErrorAction SilentlyContinue
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

function Test-OnPath([string]$list, [string]$dir) {
    $want = $dir.TrimEnd('\')
    foreach ($p in ($list -split ';')) {
        if ($p -and ($p.TrimEnd('\') -ieq $want)) { return $true }
    }
    return $false
}

if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (Test-OnPath $userPath $InstallDir)) {
        $newPath = if ($userPath) { $userPath.TrimEnd(';') + ';' + $InstallDir } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Write-Host "Added $InstallDir to your user PATH. Open a new terminal to pick it up."
    }
    if (-not (Test-OnPath $env:Path $InstallDir)) { $env:Path = "$InstallDir;$env:Path" }
}

& $dest --version
Write-Host "Installed: $dest"

$other = Get-Command megane -ErrorAction SilentlyContinue | Where-Object { $_.Source -and ($_.Source -ne $dest) } | Select-Object -First 1
if ($other) {
    Write-Warning "``megane`` currently resolves to $($other.Source), which comes earlier on PATH."
}
