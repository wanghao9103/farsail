param([switch]$Stop)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$local = Join-Path $root '.local'
$envFile = Join-Path $local 'dev.env'
if (!(Test-Path $local)) { New-Item -ItemType Directory -Path $local | Out-Null }
if (!(Test-Path $envFile)) {
    $bytes = New-Object byte[] 32
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $rng.GetBytes($bytes)
    $rng.Dispose()
    $password = ([BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
    "FARSAIL_DB_PASSWORD=$password" | Set-Content -Path $envFile -NoNewline
}
$compose = Join-Path $root 'deploy/local/compose.yaml'
if ($Stop) {
    docker compose --project-name farsail-dev --env-file $envFile -f $compose down
    exit $LASTEXITCODE
}
docker compose --project-name farsail-dev --env-file $envFile -f $compose up -d --wait
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$password = ((Get-Content $envFile) -split '=',2)[1]
$env:FARSAIL_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
$env:FARSAIL_TEST_DATABASE_URL = $env:FARSAIL_DATABASE_URL
$env:FARSAIL_MAIL_MODE = 'smtp-local'
$env:FARSAIL_SMTP_HOST = '127.0.0.1'
$env:FARSAIL_SMTP_PORT = '51025'
cargo test -p farsail-core
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test -p farsail-coordinator
exit $LASTEXITCODE
