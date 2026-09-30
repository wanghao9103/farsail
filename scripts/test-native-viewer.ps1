#requires -Version 7.0
# Own debug process and isolated profile only. Requires the Vite server on 1420.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$testRoot = Join-Path $PWD ('.local/native-viewer-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$env:FARSAIL_TEST_PROFILE_DIR = Join-Path $testRoot 'profile'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $testRoot 'webview'
$env:FARSAIL_IPC_SMOKE_PATH = Join-Path $testRoot 'report.json'
$env:FARSAIL_VIEWER_SMOKE = '1'
$process = $null
try {
    $process = Start-Process -FilePath (Join-Path $PWD 'target/debug/farsail-desktop.exe') -WorkingDirectory $PWD -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot 'stdout.log') -RedirectStandardError (Join-Path $testRoot 'stderr.log')
    $until = [DateTime]::UtcNow.AddSeconds(45)
    while (!(Test-Path ($env:FARSAIL_IPC_SMOKE_PATH + '.closed')) -and [DateTime]::UtcNow -lt $until -and !$process.HasExited) { Start-Sleep -Milliseconds 250 }
    if (!(Test-Path $env:FARSAIL_IPC_SMOKE_PATH)) { throw "No native result; inspect $testRoot" }
    $report = Get-Content $env:FARSAIL_IPC_SMOKE_PATH -Raw | ConvertFrom-Json
    if (!$report.ok) { throw ($report | ConvertTo-Json -Depth 5) }
    if (!$report.initial.maximized -or $report.restored.maximized) { throw 'Viewer must start maximized and support restore' }
    $closed = Get-Content ($env:FARSAIL_IPC_SMOKE_PATH + '.closed') -Raw | ConvertFrom-Json
    if (!$closed.mainExists -or $process.HasExited) { throw 'Closing viewer also closed main' }
    Copy-Item $env:FARSAIL_IPC_SMOKE_PATH .local/native-viewer-verification.json -Force
    Write-Output 'PASS native WebView IPC: account/admin/other-session/cross-window denied; maximize/fullscreen/minimize; viewer close preserves main.'
} finally {
    if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
    Remove-Item Env:FARSAIL_TEST_PROFILE_DIR, Env:WEBVIEW2_USER_DATA_FOLDER, Env:FARSAIL_IPC_SMOKE_PATH, Env:FARSAIL_VIEWER_SMOKE -ErrorAction SilentlyContinue
}
