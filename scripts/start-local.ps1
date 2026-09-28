$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try {
    if (!(Test-Path .local/dev.env)) { & (Join-Path $PSScriptRoot 'test-coordinator.ps1'); if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } }
    docker compose --project-name farsail-dev --env-file .local/dev.env -f deploy/local/compose.yaml up -d --wait
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $password = ((Get-Content .local/dev.env) -split '=',2)[1]
    $env:FARSAIL_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
    $env:FARSAIL_MAIL_MODE = 'smtp-local'
    $env:FARSAIL_SMTP_HOST = '127.0.0.1'
    $env:FARSAIL_SMTP_PORT = '51025'
    cargo run -p farsail-coordinator
} finally { Pop-Location }
