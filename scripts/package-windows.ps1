#requires -Version 7.0
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
function Run([scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "Command failed: $Command" }
}
if (-not $IsWindows -or (rustc -vV | Out-String) -notmatch 'host: x86_64-pc-windows-msvc') {
    throw 'Build on Windows x64 with the MSVC Rust host.'
}
$source = (git rev-parse HEAD).Trim()
if (git status --porcelain --untracked-files=no) { throw 'Commit tracked changes before packaging.' }
$output = Join-Path $PWD '.local/windows-package/artifacts'
New-Item -ItemType Directory -Force $output | Out-Null
Run { npm.cmd ci }
Run { npm.cmd run typecheck }
Run { cargo fmt --all -- --check }
Run { npm.cmd run --workspace @farsail/desktop tauri build -- --bundles nsis -- --locked }
$config = Get-Content apps/desktop/src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$name = "FarSail_$($config.version)_x64-setup.exe"
$installer = Join-Path $PWD "target/release/bundle/nsis/$name"
if (-not (Test-Path $installer)) { throw "Missing NSIS output: $name" }
$exe = Join-Path $PWD 'target/release/farsail-desktop.exe'
$bytes = [IO.File]::ReadAllBytes($exe)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Application is not AMD64' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Release must use Windows GUI subsystem' }
$strings = [Text.Encoding]::ASCII.GetString($bytes)
foreach ($probe in @('FARSAIL_IPC_SMOKE_PATH','FARSAIL_TEST_PROFILE_DIR','ipc_smoke_report','ipc_media_smoke')) {
    if ($strings.Contains($probe)) { throw "Debug probe in release: $probe" }
}
if ((Get-FileHash apps/desktop/src-tauri/icons/icon.ico).Hash -ne (Get-FileHash assets/branding/farsail-v2/icon.ico).Hash) {
    throw 'Expected FarSail v2 icon'
}
Copy-Item -LiteralPath $installer -Destination (Join-Path $output $name) -Force
$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $name" | Set-Content (Join-Path $output 'SHA256SUMS.txt') -Encoding utf8
[ordered]@{
    version = $config.version; source_commit = $source; target = 'x86_64-pc-windows-msvc'
    installer = $name; installer_sha256 = $hash; installer_bytes = (Get-Item $installer).Length
    application_sha256 = (Get-FileHash $exe).Hash.ToLowerInvariant()
    icon_sha256 = (Get-FileHash apps/desktop/src-tauri/icons/icon.ico).Hash.ToLowerInvariant()
    signature = (Get-AuthenticodeSignature $installer).Status.ToString()
    webview2 = 'embedded bootstrapper; internet required if runtime missing'
    configuration = 'release, embedded frontend, per-user NSIS'
} | ConvertTo-Json | Set-Content (Join-Path $output 'release-metadata.json') -Encoding utf8
Write-Output "Packaged $name SHA256=$hash source=$source"
