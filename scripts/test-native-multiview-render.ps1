#requires -Version 7.0
param([string]$Executable, [string]$ReportDir, [ValidateRange(1,5)][int]$Windows=5, [switch]$ForceIpc, [switch]$SlowFirst, [ValidateSet('','shared','ipc')][string]$ExpectedDelivery='')
$ErrorActionPreference='Stop'
if(!$Executable -or !$ReportDir){throw 'Provide the test executable and a fresh report directory'}
$reportRoot=[IO.Path]::GetFullPath($ReportDir)
if(Test-Path -LiteralPath $reportRoot){throw 'Use a fresh directory to avoid mixing reports'}
New-Item -ItemType Directory -Path $reportRoot | Out-Null
$env:FARSAIL_TEST_PROFILE_DIR=Join-Path $reportRoot 'profile'
$env:WEBVIEW2_USER_DATA_FOLDER=Join-Path $reportRoot 'webview'
$env:FARSAIL_RENDER_SMOKE='1'
$env:FARSAIL_RENDER_SMOKE_COUNT="$Windows"
if($ForceIpc){$env:FARSAIL_RENDER_FORCE_IPC='1'}
if($SlowFirst){$env:FARSAIL_RENDER_SLOW_FIRST='1'}
$process=$null
try {
    $process=Start-Process -FilePath $Executable -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $reportRoot 'stdout.log') -RedirectStandardError (Join-Path $reportRoot 'stderr.log')
    $deadline=[DateTime]::UtcNow.AddSeconds(55)
    do {
        Start-Sleep -Milliseconds 250
        $files=@(Get-ChildItem -LiteralPath $env:FARSAIL_TEST_PROFILE_DIR -Filter 'viewer-render-smoke-*.json' -ErrorAction SilentlyContinue)
    } while($files.Count -lt $Windows -and !$process.HasExited -and [DateTime]::UtcNow -lt $deadline)
    if($files.Count -ne $Windows){throw "Expected $Windows native renderer reports; inspect $reportRoot"}
    $reports=@($files | Sort-Object Name | ForEach-Object {Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json})
    $reports | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $reportRoot 'summary.json')
    $reports | Select-Object submittedFps,decoded,decodeP95Ms,raf,surface,width,height,visibility | ConvertTo-Json
    if(@($reports | Where-Object {$_.submitted -lt 10 -or $_.width -ne 1088 -or $_.height -ne 502}).Count){throw 'At least one native view did not render its own real JPEG frames'}
    if($ExpectedDelivery -and @($reports | Where-Object {$_.delivery -ne $ExpectedDelivery}).Count){throw 'Requested native frame delivery path was not exercised'}
} finally {
    if($process -and !$process.HasExited){Stop-Process -Id $process.Id -Force}
    Remove-Item Env:FARSAIL_TEST_PROFILE_DIR,Env:WEBVIEW2_USER_DATA_FOLDER,Env:FARSAIL_RENDER_SMOKE,Env:FARSAIL_RENDER_SMOKE_COUNT,Env:FARSAIL_RENDER_FORCE_IPC,Env:FARSAIL_RENDER_SLOW_FIRST -ErrorAction SilentlyContinue
}
