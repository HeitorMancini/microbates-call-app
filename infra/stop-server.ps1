<#
.SYNOPSIS
    Para o livekit-server e o token-server iniciados por start-server.ps1.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$pidFile = Join-Path $PSScriptRoot '.run\pids.json'

if (-not (Test-Path $pidFile)) {
    Write-Host 'Nada para parar (infra\.run\pids.json não existe).'
    exit 0
}

$pids = Get-Content $pidFile -Raw | ConvertFrom-Json
foreach ($entry in @(@('livekit-server', $pids.livekit), @('token-server', $pids.token))) {
    $name = $entry[0]
    $id = $entry[1]
    $proc = if ($id) { Get-Process -Id $id -ErrorAction SilentlyContinue }
    if ($proc) {
        Stop-Process -Id $id -Force
        Write-Host "$name parado (PID $id)."
    } else {
        Write-Host "$name já não estava rodando."
    }
}
Remove-Item $pidFile
