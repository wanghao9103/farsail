#requires -Version 7.0
# Destructive install lifecycle testing is restricted to disposable GitHub runners.
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Only run on a disposable GitHub-hosted Windows runner.'
}
Set-Location (Split-Path $PSScriptRoot -Parent)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$artifacts = Join-Path $PWD '.local/windows-package/artifacts'
$meta = Get-Content (Join-Path $artifacts 'release-metadata.json') -Raw | ConvertFrom-Json
$installer = Join-Path $artifacts $meta.installer
$installDir = Join-Path $PWD '.local/windows-package/installed'
$profileDir = Join-Path $env:LOCALAPPDATA 'app.farsail.desktop'
if (Test-Path $profileDir) { throw 'Runner already has an application profile' }
if (Test-Path $installDir) { throw 'Runner already has an installation' }
$script:appProcess = $null
$script:root = $null
function Install {
    $p = Start-Process -FilePath $installer -ArgumentList "/S /D=$installDir" -PassThru -Wait -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { throw "NSIS install failed: $($p.ExitCode)" }
}
function Find-UI([string]$Name, [string]$Type = '') {
    $until = [DateTime]::UtcNow.AddSeconds(45)
    do {
        if ($script:appProcess.HasExited) { throw 'Native application exited unexpectedly' }
        $items = $script:root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        foreach ($item in $items) {
            if ($item.Current.Name -like $Name -and (!$Type -or $item.Current.ControlType.ProgrammaticName -eq "ControlType.$Type")) { return $item }
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $until)
    # Only names from this synthetic, signed-out app window, never the desktop.
    $items | ForEach-Object { "$($_.Current.ControlType.ProgrammaticName): $($_.Current.Name)" } | Write-Output
    throw "UI element not found: $Name ($Type)"
}
function Click-UI([string]$Name) {
    $item = Find-UI $Name 'Button'
    $pattern = $item.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
}
function Start-App {
    $script:appProcess = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
    $until = [DateTime]::UtcNow.AddSeconds(45)
    do {
        Start-Sleep -Milliseconds 500
        $script:appProcess.Refresh()
        if ($script:appProcess.HasExited) { throw 'Native release failed to start' }
    } while (!$script:appProcess.MainWindowHandle -and [DateTime]::UtcNow -lt $until)
    if (!$script:appProcess.MainWindowHandle) { throw 'No native window' }
    $script:root = [Windows.Automation.AutomationElement]::FromHandle($script:appProcess.MainWindowHandle)
    $null = Find-UI '欢迎登船'
    $null = Find-UI '未登录'
    $null = Find-UI '本机未共享'
}
function Close-App {
    if ($script:appProcess -and !$script:appProcess.HasExited) {
        $null = $script:appProcess.CloseMainWindow()
        if (!$script:appProcess.WaitForExit(15000)) { throw 'Native close failed' }
    }
}
try {
    Install
    $exe = Join-Path $installDir 'farsail-desktop.exe'
    if ((Get-FileHash $exe).Hash.ToLowerInvariant() -ne $meta.application_sha256) { throw 'Installed binary differs' }
    $entry = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\FarSail' -ErrorAction Stop
    if ($entry.DisplayVersion -ne $meta.version) { throw 'Installed version differs' }
    # No remote debugging port or production test command. Accessibility drives ordinary UI.
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--force-renderer-accessibility'
    $env:FARSAIL_IPC_SMOKE_PATH = Join-Path $PWD '.local/windows-package/forbidden-report.json'
    $env:FARSAIL_TEST_PROFILE_DIR = Join-Path $PWD '.local/windows-package/forbidden-profile'
    Start-App
    Click-UI '*设置'
    $input = Find-UI '服务地址' 'Edit'
    $value = $input.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    $value.SetValue('https://example.invalid')
    Click-UI '保存地址'
    $null = Find-UI '服务地址已保存'
    Close-App
    Start-App
    Click-UI '*设置'
    $input = Find-UI '服务地址' 'Edit'
    if ($input.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -notmatch '^https://example.invalid/?$') { throw 'Native setting did not persist' }
    Close-App
    if (Test-Path $env:FARSAIL_IPC_SMOKE_PATH) { throw 'Release executed debug probe' }
    if (Test-Path $env:FARSAIL_TEST_PROFILE_DIR) { throw 'Release used debug profile override' }
    # Harmless sentinel in the credential directory proves installer retention; not a real token.
    $sentinel = Join-Path $profileDir 'installer-retention-sentinel.txt'
    'synthetic retention marker' | Set-Content $sentinel
    $before = (Get-FileHash $sentinel).Hash
    Install
    if ((Get-FileHash $sentinel).Hash -ne $before) { throw 'Reinstall changed user data' }
    Start-App
    Click-UI '*设置'
    $input = Find-UI '服务地址' 'Edit'
    if ($input.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -notmatch '^https://example.invalid/?$') { throw 'Reinstall lost settings' }
    Close-App
    $uninstall = Join-Path $installDir 'uninstall.exe'
    $p = Start-Process $uninstall -ArgumentList '/S' -PassThru -Wait -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { throw 'Uninstall failed' }
    $until = [DateTime]::UtcNow.AddSeconds(30)
    while ((Test-Path $exe) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 500 }
    if (Test-Path $exe) { throw 'Installed binary remains after uninstall' }
    if (Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\FarSail') { throw 'Uninstall registration remains' }
    if ((Get-FileHash $sentinel).Hash -ne $before) { throw 'Uninstall deleted user data' }
    [ordered]@{ source_commit=$meta.source_commit; installer_sha256=$meta.installer_sha256; install='passed'; embedded_frontend='passed'; native_settings_ipc_restart='passed'; reinstall_retention='passed'; uninstall_retention='passed'; debug_probes_absent='passed'; capture_input_or_public_network='not tested' } |
        ConvertTo-Json | Set-Content (Join-Path $artifacts 'installation-verification.json') -Encoding utf8
} finally {
    if ($script:appProcess -and !$script:appProcess.HasExited) { Stop-Process -Id $script:appProcess.Id -Force }
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, Env:FARSAIL_IPC_SMOKE_PATH, Env:FARSAIL_TEST_PROFILE_DIR -ErrorAction SilentlyContinue
}
