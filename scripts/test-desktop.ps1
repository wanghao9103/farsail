$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try {
    & (Join-Path $PSScriptRoot 'test-coordinator.ps1')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $password = ((Get-Content .local/dev.env) -split '=',2)[1]
    $env:FARSAIL_TEST_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
    npm ci
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p farsail-client
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo check -p farsail-desktop
    exit $LASTEXITCODE
} finally { Pop-Location }
