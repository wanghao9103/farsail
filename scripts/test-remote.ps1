param([switch]$SkipServices, [switch]$RealCapture)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try {
    if (-not $SkipServices) {
        & (Join-Path $PSScriptRoot 'test-coordinator.ps1')
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    $password = ((Get-Content .local/dev.env) -split '=',2)[1]
    $env:FARSAIL_TEST_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
    if ($RealCapture) { $env:FARSAIL_REAL_CAPTURE = '1' }
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p farsail-media -p farsail-windows
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p farsail-transport
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p farsail-client
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p farsail-desktop --lib
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo clippy -p farsail-core -p farsail-media -p farsail-windows -p farsail-transport -p farsail-client -p farsail-coordinator -p farsail-desktop --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo build -p farsail-desktop
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    exit 0
} finally {
    Remove-Item Env:FARSAIL_REAL_CAPTURE -ErrorAction SilentlyContinue
    Remove-Item Env:FARSAIL_TEST_DATABASE_URL -ErrorAction SilentlyContinue
    Pop-Location
}
