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

function Ask($q, $default) {
  if (-not [Environment]::UserInteractive) { return ($default -match '^[Yy]') }
  $a = Read-Host $q
  if ([string]::IsNullOrWhiteSpace($a)) { $a = $default }
  return $a -match '^[Yy]'
}

# Start Menu entry.
if (Ask "Create a Start Menu entry? [y/N]" "n") {
  $ico = Join-Path $Dir "eve-spai.ico"
  try { Invoke-WebRequest "https://raw.githubusercontent.com/$Repo/main/assets/eve-spai.ico" -OutFile $ico -UseBasicParsing } catch {}
  $startMenu = [Environment]::GetFolderPath("Programs")
  $lnk = Join-Path $startMenu "EVE Spai.lnk"
  $shell = New-Object -ComObject WScript.Shell
  $sc = $shell.CreateShortcut($lnk)
  $sc.TargetPath = $out
  $sc.WorkingDirectory = $Dir
  $sc.Description = "EVE Online intel tool"
  if (Test-Path $ico) { $sc.IconLocation = $ico }
  $sc.Save()
  Write-Host "Created Start Menu entry."
}

# Add the install dir to the user PATH (with consent).
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$Dir*") {
  if (Ask "Add $Dir to your PATH? [Y/n]" "y") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$Dir", "User")
    Write-Host "Added $Dir to your user PATH (restart your terminal to use 'eve-spai')."
  }
}
Write-Host "Run it with: eve-spai"
