#requires -Version 7.0
param([switch]$SystemTools)
# Only disposable hosted Windows runners may install this test service.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Hosted runner only' }
Set-Location (Split-Path $PSScriptRoot -Parent)
New-Item -ItemType Directory -Force (Join-Path $PWD '.local') | Out-Null
if (Get-Service FarSailDesktopHost -ErrorAction SilentlyContinue) { throw 'Existing service' }
$directory=Join-Path $env:ProgramFiles 'FarSailSystemProbe'
if (Test-Path $directory) { throw 'Existing probe directory' }
New-Item -ItemType Directory $directory | Out-Null
$exe=Join-Path $directory 'farsail-system-host.exe'
Copy-Item target/release/farsail-system-host.exe $exe
try {
    $p=Start-Process $exe -ArgumentList '--install' -PassThru -Wait
    if ($p.ExitCode -ne 0) { throw 'Service installation failed' }
    Write-Output ("Probe caller session=" + (Get-Process -Id $PID).SessionId)
    $p=Start-Process $exe -ArgumentList '--probe' -PassThru -Wait -RedirectStandardError (Join-Path $PWD '.local/system-host-probe-error.txt')
    if ($p.ExitCode -ne 0) { throw 'Authenticated broker capture failed' }
    Write-Output 'PASS protected LocalSystem service, interactive-session agent and authenticated in-memory DXGI capture'
    if ($SystemTools) {
        $messages = @(cargo test --locked --release -p farsail-windows --lib --no-run --message-format=json | ForEach-Object { $_ | ConvertFrom-Json })
        if ($LASTEXITCODE -ne 0) { throw 'System tool test harness build failed' }
        $harness = @($messages | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable -and $_.profile.test })
        if ($harness.Count -ne 1) { throw 'Expected exactly one Windows library test executable' }
        $testExe=Join-Path $directory 'farsail-desktop.exe'
        Copy-Item $harness[0].executable $testExe
        $env:FARSAIL_SYSTEM_TOOLS_TEST='1'
        $env:FARSAIL_SYSTEM_TOOLS_REPORT=Join-Path $PWD '.local/system-tools-report.json'
        & $testExe system_tools_test::real_system_tools_through_broker --ignored --nocapture --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'One or more system tools failed; inspect system-tools-report.json' }
    }

} catch {
    Get-Content (Join-Path $PWD '.local/system-host-probe-error.txt') -ErrorAction SilentlyContinue
    Get-CimInstance Win32_Service -Filter "Name='FarSailDesktopHost'" | Format-List Name,State,StartName,PathName,ProcessId,ExitCode
    Get-CimInstance Win32_Process -Filter "Name='farsail-system-host.exe'" | Format-List ProcessId,ParentProcessId,SessionId,CommandLine
    Get-WinEvent -LogName Application -MaxEvents 200 -ErrorAction SilentlyContinue | Where-Object ProviderName -eq 'FarSailDesktopHost' | Format-List TimeCreated,Message,@{Name='RawData';Expression={$_.Properties.Value -join '; '}}
    throw
} finally {
    $p=Start-Process $exe -ArgumentList '--uninstall' -PassThru -Wait
    if ($p.ExitCode -ne 0) { Write-Warning 'Probe cleanup failed' }
    Remove-Item $directory -Recurse -Force
}
