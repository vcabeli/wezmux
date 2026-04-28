param(
  [string]$TargetDir = "target/release",
  [string]$PackageRoot = "target/windows-package",
  [string]$PackageName = "Wezmux",
  [string]$ZipName = "Wezmux-windows-x64.zip"
)

$ErrorActionPreference = "Stop"

$target = (Resolve-Path $TargetDir).Path
$packageDir = Join-Path $PackageRoot $PackageName

if (Test-Path $packageDir) {
  Remove-Item -Recurse -Force $packageDir
}
New-Item -ItemType Directory -Force $packageDir | Out-Null

$requiredFiles = @(
  "wezterm.exe",
  "wezterm-gui.exe",
  "wezterm-mux-server.exe",
  "strip-ansi-escapes.exe",
  "libEGL.dll",
  "libGLESv2.dll",
  "conpty.dll",
  "OpenConsole.exe"
)

foreach ($file in $requiredFiles) {
  $source = Join-Path $target $file
  if (!(Test-Path $source)) {
    throw "Missing required Windows package file: $source"
  }
  Copy-Item $source $packageDir
}

$mesaSource = Join-Path $target "mesa"
$mesaDll = Join-Path $mesaSource "opengl32.dll"
if (!(Test-Path $mesaDll)) {
  throw "Missing required Windows package file: $mesaDll"
}
Copy-Item -Recurse $mesaSource $packageDir

Copy-Item "LICENSE.md" $packageDir

@"
Wezmux Windows preview

Run wezterm-gui.exe to start Wezmux.

This zip is the preview support path. It does not install shell integrations,
PATH entries, or agent hook wrappers.
"@ | Set-Content -Encoding UTF8 (Join-Path $packageDir "README-WINDOWS.txt")

$zipPath = Join-Path (Get-Location) $ZipName
if (Test-Path $zipPath) {
  Remove-Item -Force $zipPath
}
Compress-Archive -Path $packageDir -DestinationPath $zipPath

Write-Host "Windows package ready: $zipPath"
