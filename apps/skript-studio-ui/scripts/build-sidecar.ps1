<#
.SYNOPSIS
    Build the skript-lsp sidecar binary and copy it to where Tauri expects it.

.DESCRIPTION
    Compiles the skript-lsp crate from the workspace root in release mode,
    then copies the binary to src-tauri/ with the target-triple filename that
    Tauri's externalBin resolver requires.

    The output filename follows Tauri's convention:
        <name>-<target-triple>[.exe]

.EXAMPLE
    .\scripts\build-sidecar.ps1
#>

$ErrorActionPreference = "Stop"

$WorkspaceRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$TargetDir     = Join-Path $WorkspaceRoot "target\release"
$DestDir       = Join-Path (Split-Path $PSScriptRoot -Parent) "src-tauri"

# Determine target triple. Tauri's externalBin resolver matches the sidecar
# filename against the *host* triple used to compile the app, so we default to
# `rustc`'s host triple. Override with TAURI_SIDECAR_TARGET for cross-builds.
$TargetTriple = if ($env:TAURI_SIDECAR_TARGET) {
    $env:TAURI_SIDECAR_TARGET
} else {
    $rustcVv = & rustc -vV
    if ($LASTEXITCODE -ne 0) {
        throw "rustc -vV failed; is the Rust toolchain installed?"
    }
    $hostLine = ($rustcVv | Select-String -Pattern '^host:\s*(.+)$')
    if (-not $hostLine) {
        throw 'Could not determine host target triple from rustc -vV'
    }
    $hostLine.Matches[0].Groups[1].Value.Trim()
}
Write-Host "==> Target triple: $TargetTriple" -ForegroundColor DarkGray

$ExeExt = if ($IsWindows -or ($env:OS -eq "Windows_NT")) { ".exe" } else { "" }
$BinaryName  = "skript-lsp${ExeExt}"
$DestName    = "skript-lsp-${TargetTriple}${ExeExt}"

Write-Host "==> Building skript-lsp sidecar (release)..." -ForegroundColor Cyan
Push-Location $WorkspaceRoot
try {
    # Cargo emits progress/diagnostics on stderr. Under $ErrorActionPreference="Stop"
    # PowerShell would convert that native-stderr output into a terminating error,
    # aborting the script before we ever check $LASTEXITCODE. Run cargo under
    # "Continue" and decide success solely from the exit code.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        cargo build --release -p skript-lsp
        $buildExit = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $prevEap
    }
    if ($buildExit -ne 0) {
        throw "cargo build failed with exit code $buildExit"
    }
}
finally {
    Pop-Location
}

$SrcPath = Join-Path $TargetDir $BinaryName
if (-not (Test-Path $SrcPath)) {
    throw "Expected binary not found: $SrcPath"
}

$DstPath = Join-Path $DestDir $DestName
Copy-Item -Path $SrcPath -Destination $DstPath -Force
Write-Host "==> Copied sidecar to $DstPath" -ForegroundColor Green

# On Windows the runtime matches the sidecar against the host triple (e.g.
# x86_64-pc-windows-gnu), but the MSI/WiX bundler looks for the msvc variant.
# The sidecar is a standalone executable invoked as a subprocess (never linked),
# so its filename is purely a matching convention — emit both names so dev and
# bundling both resolve.
if ($ExeExt -eq ".exe" -and $TargetTriple -notmatch "-msvc$") {
    $MsvcName = "skript-lsp-x86_64-pc-windows-msvc.exe"
    $MsvcPath = Join-Path $DestDir $MsvcName
    Copy-Item -Path $SrcPath -Destination $MsvcPath -Force
    Write-Host "==> Copied msvc-aliased sidecar to $MsvcPath (for MSI bundling)" -ForegroundColor Green
}

Write-Host "==> Done." -ForegroundColor Green
