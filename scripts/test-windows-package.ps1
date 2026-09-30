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
$fixtureProcess = $null
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
function Start-App([bool]$SignedOut = $true) {
    $script:appProcess = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
    $until = [DateTime]::UtcNow.AddSeconds(45)
    do {
        Start-Sleep -Milliseconds 500
        $script:appProcess.Refresh()
        if ($script:appProcess.HasExited) { throw 'Native release failed to start' }
    } while (!$script:appProcess.MainWindowHandle -and [DateTime]::UtcNow -lt $until)
    if (!$script:appProcess.MainWindowHandle) { throw 'No native window' }
    $script:root = [Windows.Automation.AutomationElement]::FromHandle($script:appProcess.MainWindowHandle)
    $null = Find-UI '设备控制台'
    if ($SignedOut) { $null = Find-UI '未登录'; $null = Find-UI '本机未共享' }
}
function Set-UIValue([string]$Name, [string]$Value) {
    (Find-UI $Name 'Edit').GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
}
function Read-Preference {
    $bytes=[IO.File]::ReadAllBytes($preferencePath)
    $plain=[Security.Cryptography.ProtectedData]::Unprotect($bytes,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
    [Text.Encoding]::UTF8.GetString($plain) | ConvertFrom-Json
}
function Wait-Restore {
    $until=[DateTime]::UtcNow.AddSeconds(45)
    do {
        $items=$script:root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        foreach ($item in $items) {
            if ($item.Current.Name -eq '本机屏幕共享中') { return 'passed' }
            if ($item.Current.Name -like '*本次未恢复共享*') {
                $status=Invoke-RestMethod ($fixture.server+'/fixture/status')
                if ($status.sharing) { throw 'Failed restoration still advertised sharing' }
                return 'unavailable_fail_closed'
            }
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $until)
    throw 'Startup restoration did not settle'
}
function Close-App {
    if ($script:appProcess -and !$script:appProcess.HasExited) {
        Click-UI '关闭窗口'
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
    Click-UI '最大化或还原'
    Start-Sleep -Milliseconds 800
    $windowPattern = $script:root.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)
    if ($windowPattern.Current.WindowVisualState -ne [Windows.Automation.WindowVisualState]::Maximized) { throw 'Custom maximize failed' }
    Click-UI '最大化或还原'
    Click-UI '最小化'
    Start-Sleep -Milliseconds 800
    if ($windowPattern.Current.WindowVisualState -ne [Windows.Automation.WindowVisualState]::Minimized) { throw 'Custom minimize failed' }
    $windowPattern.SetWindowVisualState([Windows.Automation.WindowVisualState]::Normal)
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
    # Disposable runner only: normal UI creates synthetic login/device credentials.
    # Seed a known encrypted opt-in to exercise production startup without a test bypass.
    Add-Type -AssemblyName System.Security
    $fixturePath=Join-Path $PWD '.local/windows-package/share-fixture.json'
    $fixtureProcess=Start-Process node -ArgumentList @('scripts/share-preferences-fixture.cjs', ('"'+$fixturePath+'"')) -WindowStyle Hidden -PassThru
    $until=[DateTime]::UtcNow.AddSeconds(15)
    while (!(Test-Path $fixturePath) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
    $fixture=Get-Content $fixturePath -Raw | ConvertFrom-Json
    Start-App
    Click-UI '*设置'; Set-UIValue '服务地址' $fixture.server; Click-UI '保存地址'
    $null=Find-UI '服务地址已保存'
    Click-UI '*我的设备'
    Set-UIValue '邮箱地址' 'fixture@example.invalid'; Set-UIValue '密码' 'synthetic-password'
    Click-UI '登录'
    $null=Find-UI '*还没有设备*'
    Click-UI '*总览'
    Click-UI '添加这台电脑'
    $null=Find-UI '*这台电脑已添加*'
    Close-App
    $preferencePath=Join-Path $profileDir 'sharing-preferences.bin'
    $saved=[ordered]@{version=1;scope=@{server=$fixture.server;owner=$fixture.owner;device=$fixture.device;session=$fixture.session};sharing=$true;watch=$true}
    $plain=[Text.Encoding]::UTF8.GetBytes(($saved | ConvertTo-Json -Depth 4 -Compress))
    [IO.File]::WriteAllBytes($preferencePath,[Security.Cryptography.ProtectedData]::Protect($plain,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))
    $optInHash=(Get-FileHash $preferencePath).Hash
    Start-App $false
    $sharingPreflight=Wait-Restore
    if ((Get-FileHash $preferencePath).Hash -ne $optInHash) { throw 'Startup changed remembered intent' }
    Click-UI '关闭远程值守'; Close-App
    $preference=Read-Preference
    if (!$preference.sharing -or $preference.watch) { throw 'Native watch opt-out did not persist' }
    Start-App $false
    $null=Wait-Restore
    $null=Find-UI '开启远程值守'
    Click-UI '停止本机共享'; Close-App
    $preference=Read-Preference
    if ($preference.sharing -or $preference.watch) { throw 'Native sharing opt-out did not persist' }
    $optOutHash=(Get-FileHash $preferencePath).Hash
    Start-App $false
    $null=Find-UI '本机未共享'
    if ((Get-FileHash $preferencePath).Hash -ne $optOutHash) { throw 'Restart changed explicit opt-out' }
    Click-UI '*账号安全'; Click-UI '退出登录'
    $null=Find-UI '未登录'
    if (Test-Path $preferencePath) { throw 'Explicit logout retained approval preference' }
    Click-UI '*设置'; Set-UIValue '服务地址' 'https://example.invalid'; Click-UI '保存地址'
    $null=Find-UI '服务地址已保存'; Close-App
    # Off-only encrypted fixture verifies reinstall/uninstall preserve this new record too.
    $saved.sharing=$false; $saved.watch=$false
    $plain=[Text.Encoding]::UTF8.GetBytes(($saved | ConvertTo-Json -Depth 4 -Compress))
    [IO.File]::WriteAllBytes($preferencePath,[Security.Cryptography.ProtectedData]::Protect($plain,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))
    $retainedPreferenceHash=(Get-FileHash $preferencePath).Hash
    # Harmless sentinel in the credential directory proves installer retention; not a real token.
    $sentinel = Join-Path $profileDir 'installer-retention-sentinel.txt'
    'synthetic retention marker' | Set-Content $sentinel
    $before = (Get-FileHash $sentinel).Hash
    Install
    if ((Get-FileHash $sentinel).Hash -ne $before) { throw 'Reinstall changed user data' }
    if ((Get-FileHash $preferencePath).Hash -ne $retainedPreferenceHash) { throw 'Reinstall changed sharing preferences' }
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
    if ((Get-FileHash $preferencePath).Hash -ne $retainedPreferenceHash) { throw 'Uninstall changed sharing preferences' }
    [ordered]@{ source_commit=$meta.source_commit; installer_sha256=$meta.installer_sha256; install='passed'; embedded_frontend='passed'; native_settings_ipc_restart='passed'; sharing_preferences_restart='passed'; sharing_desktop_preflight=$sharingPreflight; reinstall_retention='passed'; uninstall_retention='passed'; debug_probes_absent='passed'; capture_input_or_public_network='not tested' } |
        ConvertTo-Json | Set-Content (Join-Path $artifacts 'installation-verification.json') -Encoding utf8
} finally {
    if ($script:appProcess -and !$script:appProcess.HasExited) { Stop-Process -Id $script:appProcess.Id -Force }
    if ($fixtureProcess -and !$fixtureProcess.HasExited) { Stop-Process -Id $fixtureProcess.Id -Force }
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, Env:FARSAIL_IPC_SMOKE_PATH, Env:FARSAIL_TEST_PROFILE_DIR -ErrorAction SilentlyContinue
}
