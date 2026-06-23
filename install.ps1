# EVE Spai installer for Windows.
#
#   irm https://raw.githubusercontent.com/Amryu/eve-spai/main/install.ps1 | iex
#
# For a PRIVATE repo, set a GitHub token with `repo` scope first:
#   $env:GITHUB_TOKEN = "ghp_xxx"
#
# Override the install dir with $env:EVE_SPAI_DIR.
$ErrorActionPreference = "Stop"

$Repo   = "Amryu/eve-spai"   # <-- set to your owner/repo
$Dir    = if ($env:EVE_SPAI_DIR) { $env:EVE_SPAI_DIR } else { "$env:LOCALAPPDATA\Programs\eve-spai" }
$Api    = "https://api.github.com/repos/$Repo"
$Asset  = "eve-spai-windows-x86_64.exe"

$headers = @{ "Accept" = "application/vnd.github+json"; "User-Agent" = "eve-spai-installer" }
if ($env:GITHUB_TOKEN) { $headers["Authorization"] = "token $env:GITHUB_TOKEN" }

Write-Host "Looking up the latest release of $Repo..."
$release = Invoke-RestMethod -Uri "$Api/releases/latest" -Headers $headers
$tag = $release.tag_name
if (-not $tag) { throw "Could not find a release (private repo? set `$env:GITHUB_TOKEN)." }

$assetObj = $release.assets | Where-Object { $_.name -eq $Asset } | Select-Object -First 1
if (-not $assetObj) { throw "Release $tag has no asset '$Asset'." }

New-Item -ItemType Directory -Force -Path $Dir | Out-Null
$out = Join-Path $Dir "eve-spai.exe"

Write-Host "Downloading $Asset ($tag)..."
$dlHeaders = $headers.Clone()
$dlHeaders["Accept"] = "application/octet-stream"
Invoke-WebRequest -Uri "$Api/releases/assets/$($assetObj.id)" -Headers $dlHeaders -OutFile $out

Write-Host "Installed eve-spai $tag to $out"

# Add the install dir to the user PATH if it isn't there.
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$Dir*") {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$Dir", "User")
  Write-Host "Added $Dir to your user PATH (restart your terminal to use 'eve-spai')."
}
Write-Host "Run it with: eve-spai"
