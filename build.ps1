# Builds the launcher from Windows PowerShell. The sources sit in OneDrive, whose sync locks
# build files, so Cargo's output goes to ~/.cargo-targets instead of src-tauri/target.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Command = @("build"))
$ErrorActionPreference = "Continue"
$env:CARGO_TARGET_DIR = Join-Path $HOME ".cargo-targets\mistinn-launcher"
# Tauri and the windows crate build with the MSVC toolchain on Windows.
if ($IsWindows -or $env:OS -eq "Windows_NT") { $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc" }
Push-Location $PSScriptRoot
try {
    # the 霧宿 mod is built in: fetch its latest release when it is not there yet
    $mods = Join-Path $PSScriptRoot "src-tauri/resources/mods"
    if (-not (Test-Path (Join-Path $mods "mistinn.jar")) -or -not (Test-Path (Join-Path $mods "mistinn.version"))) {
        & (Join-Path $PSScriptRoot "scripts/fetch-mod.ps1")
        if (-not $?) { exit 1 }
    }
    if ($Command[0] -eq "test" -or $Command[0] -eq "check") {
        cargo @Command --manifest-path src-tauri/Cargo.toml
    } else {
        npx tauri @Command
    }
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
