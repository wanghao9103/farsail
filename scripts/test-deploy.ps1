param([switch]$Build)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root
Remove-Item Env:MSYS_NO_PATHCONV -ErrorAction SilentlyContinue
Remove-Item Env:MSYS2_ARG_CONV_EXCL -ErrorAction SilentlyContinue
$env:PATH = "$root/.local/tools;C:/Program Files/Git/usr/bin;$env:PATH"
$bash = 'C:/Program Files/Git/bin/bash.exe'
if ($Build) {
    foreach ($target in @('coordinator', 'relay')) {
        docker build --target $target --build-arg REVISION=wi008a-local -f deploy/production/Dockerfile -t "farsail/${target}:wi008a-local" .
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
}
& $bash scripts/deploy/test.sh
exit $LASTEXITCODE
