# Puts the 霧宿 mod the launcher embeds into src-tauri/resources/mods: the jar of the mod's latest
# GitHub Release (or of -Version), checked against the digest GitHub keeps for it, as mistinn.jar,
# with its version in mistinn.version. build.ps1 runs it when the mod is missing.
param(
    [string]$Version = "",
    [string]$Repo = "keeiv/mistinn-exile"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ($Version -and $Version -notmatch '^\d+\.\d+\.\d+$') { throw "not a version: $Version" }
$dir = Join-Path $PSScriptRoot "..\src-tauri\resources\mods"
New-Item -ItemType Directory -Force $dir | Out-Null

$headers = @{ "User-Agent" = "mistinn-launcher-build"; "Accept" = "application/vnd.github+json" }
if ($env:GITHUB_TOKEN) { $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN" }
$api = if ($Version) { "https://api.github.com/repos/$Repo/releases/tags/v$Version" } else { "https://api.github.com/repos/$Repo/releases/latest" }
$release = Invoke-RestMethod -Uri $api -Headers $headers

if ($release.tag_name -notmatch '^v(\d+\.\d+\.\d+)$') { throw "unexpected mod release tag: $($release.tag_name)" }
$found = $Matches[1]
$asset = $release.assets | Where-Object { $_.name -eq "mistinn-$found.jar" } | Select-Object -First 1
if (-not $asset) { throw "release $($release.tag_name) has no mistinn-$found.jar" }

$jar = Join-Path $dir "mistinn.jar"
$partial = "$jar.part"
Invoke-WebRequest -Uri $asset.browser_download_url -Headers @{ "User-Agent" = "mistinn-launcher-build" } -OutFile $partial
$bytes = [System.IO.File]::ReadAllBytes($partial)
if ($bytes.Length -lt 1024 -or $bytes[0] -ne 0x50 -or $bytes[1] -ne 0x4B) { throw "the download is not a jar" }
if ($asset.digest -and $asset.digest -like "sha256:*") {
    $want = $asset.digest.Substring(7).ToLowerInvariant()
    $got = (Get-FileHash $partial -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($got -ne $want) { throw "mistinn-$found.jar does not match its release digest" }
}
Move-Item -Force $partial $jar
[System.IO.File]::WriteAllText((Join-Path $dir "mistinn.version"), $found)
Write-Output "embedded mod: mistinn $found"
