#requires -Version 7.0
# Actual native IPC + real DPAPI + ordinary close/restart. Own synthetic profile/server only.
$ErrorActionPreference='Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$testRoot=Join-Path $PWD ('.local/share020-native-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $testRoot | Out-Null
$fixturePath=Join-Path $testRoot 'fixture.json'
$fixtureProcess=$null; $appProcess=$null
try {
  $fixtureProcess=Start-Process node -ArgumentList @('scripts/share-preferences-fixture.cjs', ('"'+$fixturePath+'"')) -WindowStyle Hidden -PassThru
  $until=[DateTime]::UtcNow.AddSeconds(15)
  while (!(Test-Path $fixturePath) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
  $fixture=Get-Content $fixturePath -Raw | ConvertFrom-Json
  $env:FARSAIL_TEST_PROFILE_DIR=Join-Path $testRoot 'profile'
  $env:WEBVIEW2_USER_DATA_FOLDER=Join-Path $testRoot 'webview'
  $env:FARSAIL_SHARE_SMOKE_SERVER=$fixture.server
  $results=@()
  foreach ($mode in @('enable','restore','watch-off','restore','share-off','off')) {
    $index=$results.Count
    $env:FARSAIL_SHARE_SMOKE=$mode
    $env:FARSAIL_IPC_SMOKE_PATH=Join-Path $testRoot "report-$index.json"
    $appProcess=Start-Process (Join-Path $PWD 'target/debug/farsail-desktop.exe') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot "stdout-$index.log") -RedirectStandardError (Join-Path $testRoot "stderr-$index.log")
    $until=[DateTime]::UtcNow.AddSeconds(60)
    while (!(Test-Path $env:FARSAIL_IPC_SMOKE_PATH) -and !$appProcess.HasExited -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 250 }
    if (!(Test-Path $env:FARSAIL_IPC_SMOKE_PATH)) { throw "Native report missing: $testRoot" }
    $report=Get-Content $env:FARSAIL_IPC_SMOKE_PATH -Raw | ConvertFrom-Json
    if (!$report.ok) { throw ($report | ConvertTo-Json -Depth 6) }
    $sharing=$index -lt 4; $watch=$index -lt 2
    if ($report.state.sharing -ne $sharing -or $report.state.remoteWatch -ne $watch -or $report.state.sharePreferences.sharing -ne $sharing -or $report.state.sharePreferences.watch -ne $watch) { throw "Wrong persisted runtime choices: $mode ($index)" }
    if (!$appProcess.WaitForExit(15000)) { throw 'Ordinary native close failed' }
    $results+=$report
  }
  $results | ConvertTo-Json -Depth 8 | Set-Content .local/share020-native-verification.json
  Write-Output 'PASS actual native sharing/watch IPC, desktop preflight without pixels/input, DPAPI close/restart, watch off and sharing off persist.'
} finally {
  foreach ($process in @($appProcess,$fixtureProcess)) { if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force } }
  Remove-Item Env:FARSAIL_TEST_PROFILE_DIR,Env:WEBVIEW2_USER_DATA_FOLDER,Env:FARSAIL_SHARE_SMOKE_SERVER,Env:FARSAIL_SHARE_SMOKE,Env:FARSAIL_IPC_SMOKE_PATH -ErrorAction SilentlyContinue
}
