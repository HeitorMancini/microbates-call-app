<#
.SYNOPSIS
    Sobe o livekit-server e o token-server do Sheeshcord (rede Radmin VPN).
.EXAMPLE
    .\infra\start-server.ps1 -NodeIp 26.1.2.3
.EXAMPLE
    .\infra\start-server.ps1 -OpenFirewall   # como administrador, na primeira vez
#>
[CmdletBinding()]
param(
    # IP anunciado aos clientes (IP do Radmin). Padrão: primeiro IPv4 26.x.x.x encontrado.
    [string]$NodeIp,
    [string]$LiveKitBin,
    [string]$ApiKey = 'devkey',
    [string]$ApiSecret = 'devsecretdevsecretdevsecretdevsecret',
    [int]$TokenPort = 7881,
    # Recompila o token-server (release) mesmo se o .exe já existir.
    [switch]$Rebuild,
    # Cria regras de firewall (requer PowerShell como administrador).
    [switch]$OpenFirewall
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$runDir = Join-Path $PSScriptRoot '.run'
$pidFile = Join-Path $runDir 'pids.json'
New-Item -ItemType Directory -Force $runDir | Out-Null

if (Test-Path $pidFile) {
    $old = Get-Content $pidFile -Raw | ConvertFrom-Json
    $alive = @($old.livekit, $old.token) | Where-Object { $_ -and (Get-Process -Id $_ -ErrorAction SilentlyContinue) }
    if ($alive) {
        throw "Servidores já em execução (PIDs $($alive -join ', ')). Rode .\infra\stop-server.ps1 antes."
    }
    Remove-Item $pidFile
}

if (-not $NodeIp) {
    $radmin = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.IPAddress -like '26.*' } | Select-Object -First 1
    if ($radmin) {
        $NodeIp = $radmin.IPAddress
    } else {
        Write-Warning 'IP do Radmin (26.x.x.x) não encontrado; usando 127.0.0.1 (só funciona nesta máquina).'
        $NodeIp = '127.0.0.1'
    }
}

if (-not $LiveKitBin) {
    $cmd = Get-Command livekit-server -ErrorAction SilentlyContinue
    $LiveKitBin = if ($cmd) { $cmd.Source } else { Join-Path $env:USERPROFILE '.local\bin\livekit-server.exe' }
}
if (-not (Test-Path $LiveKitBin)) {
    throw "livekit-server não encontrado ($LiveKitBin). Use -LiveKitBin <caminho>."
}

if ($OpenFirewall) {
    $rules = @(
        @{ Name = 'Sheeshcord LiveKit TCP'; Protocol = 'TCP'; Port = '7880,7882' },
        @{ Name = 'Sheeshcord token-server'; Protocol = 'TCP'; Port = "$TokenPort" },
        @{ Name = 'Sheeshcord LiveKit UDP'; Protocol = 'UDP'; Port = '50000-50100' }
    )
    foreach ($r in $rules) {
        if (-not (Get-NetFirewallRule -DisplayName $r.Name -ErrorAction SilentlyContinue)) {
            New-NetFirewallRule -DisplayName $r.Name -Direction Inbound -Action Allow `
                -Protocol $r.Protocol -LocalPort ($r.Port -split ',') | Out-Null
            Write-Host "Regra de firewall criada: $($r.Name)"
        }
    }
}

$tokenExe = Join-Path $root 'target\release\token-server.exe'
if ($Rebuild -or -not (Test-Path $tokenExe)) {
    Write-Host 'Compilando token-server (release)...'
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    Push-Location $root
    try {
        cargo build --release -p token-server
        if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar o token-server.' }
    } finally {
        Pop-Location
    }
}

$livekit = Start-Process -FilePath $LiveKitBin -PassThru -WindowStyle Hidden `
    -ArgumentList @('--config', "`"$(Join-Path $PSScriptRoot 'livekit.yaml')`"", '--node-ip', $NodeIp, '--keys', "`"${ApiKey}: ${ApiSecret}`"") `
    -RedirectStandardOutput (Join-Path $runDir 'livekit.out.log') `
    -RedirectStandardError (Join-Path $runDir 'livekit.err.log')

$env:LIVEKIT_API_KEY = $ApiKey
$env:LIVEKIT_API_SECRET = $ApiSecret
$env:LIVEKIT_URL = "ws://${NodeIp}:7880"
$env:TOKEN_SERVER_BIND = "0.0.0.0:$TokenPort"
$token = Start-Process -FilePath $tokenExe -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $runDir 'token-server.out.log') `
    -RedirectStandardError (Join-Path $runDir 'token-server.err.log')

@{ livekit = $livekit.Id; token = $token.Id } | ConvertTo-Json | Set-Content $pidFile

$healthy = $false
for ($i = 0; $i -lt 30 -and -not $healthy; $i++) {
    Start-Sleep -Milliseconds 500
    try {
        $h = Invoke-RestMethod -Uri "http://127.0.0.1:$TokenPort/health" -TimeoutSec 2
        $lk = Invoke-WebRequest -Uri 'http://127.0.0.1:7880/' -UseBasicParsing -TimeoutSec 2
        $healthy = ($h.status -eq 'ok') -and ($lk.StatusCode -eq 200)
    } catch {
        $healthy = $false
    }
}

if (-not $healthy) {
    Write-Warning "Servidores não responderam. Veja os logs em $runDir"
    exit 1
}

Write-Host ''
Write-Host 'Sheeshcord no ar:' -ForegroundColor Green
Write-Host "  LiveKit      ws://${NodeIp}:7880   (PID $($livekit.Id))"
Write-Host "  token-server http://${NodeIp}:$TokenPort (PID $($token.Id))"
Write-Host "  Logs         $runDir"
Write-Host '  Parar        .\infra\stop-server.ps1'
