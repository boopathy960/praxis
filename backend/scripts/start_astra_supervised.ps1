$ErrorActionPreference = "Stop"

$backendRoot = Split-Path -Parent $PSScriptRoot
Push-Location $backendRoot
try {
    cargo build --release -p astra-server -p astra-supervisor
    & "$backendRoot\target\release\astra-supervisor.exe"
}
finally {
    Pop-Location
}
