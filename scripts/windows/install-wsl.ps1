#Requires -Version 5.1
# Installe WSL2 + Ubuntu 24.04 sur D:\WSL et configure .wslconfig.
# Ensuite, dans Ubuntu : git clone du dépôt puis scripts/setup-ubuntu.sh.
# Relançable sans risque : chaque étape déjà faite est sautée.

$ErrorActionPreference = 'Continue'
$env:WSL_UTF8 = '1'   # sortie de wsl.exe en UTF-8 au lieu d'UTF-16

$Distro  = 'Ubuntu-24.04'
$Root    = 'D:\WSL'
$Here    = Split-Path -Parent $PSCommandPath

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Pause-Exit($code) {
    try { Stop-Transcript | Out-Null } catch {}
    Read-Host "`nAppuie sur Entrée pour fermer"
    exit $code
}

# On ne juge wsl.exe que sur son code de retour. Le passage par cmd.exe évite que
# PowerShell 5.1 transforme ce que wsl.exe écrit sur stderr en exception.
function Invoke-Wsl {
    $line = 'wsl.exe ' + ($args -join ' ') + ' 2>&1'
    cmd.exe /c $line
}

[Console]::OutputEncoding = [Text.Encoding]::UTF8

# --- Droits administrateur -------------------------------------------------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Start-Process powershell.exe -Verb RunAs -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
    exit
}

try { Start-Transcript -Path (Join-Path $Here 'installer.log') -Append -ErrorAction Stop | Out-Null }
catch { Write-Host "Journal désactivé : $($_.Exception.Message)" }

try {
    # --- Disque D: ---------------------------------------------------------
    Step 'Vérification du disque D:'
    $d = Get-PSDrive -Name D -ErrorAction SilentlyContinue
    if (-not $d) { throw 'Lecteur D: introuvable.' }
    $freeGb = [math]::Round($d.Free / 1GB)
    Write-Host "$freeGb Go libres sur D:"
    if ($freeGb -lt 150) { throw 'Il faut au moins 150 Go libres sur D: (PX4 + Yocto).' }
    New-Item -ItemType Directory -Force -Path $Root -ErrorAction Stop | Out-Null

    # --- WSL lui-même ------------------------------------------------------
    Step 'Activation / mise à jour de WSL'
    # --version n'existe que dans le WSL moderne (Store) : s'il échoue, on installe.
    Invoke-Wsl --version | Write-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host 'WSL absent : activation des composants Windows...'
        Invoke-Wsl --install --no-distribution | Write-Host
        if ($LASTEXITCODE -ne 0) { throw "Activation de WSL impossible (code $LASTEXITCODE). Vérifie que la virtualisation est activée dans le BIOS." }
    }
    # Le WSL moderne (paquet Store) n'a besoin que de la « Plateforme de machine virtuelle ».
    # L'ancienne fonctionnalité Microsoft-Windows-Subsystem-Linux peut rester désactivée.
    $pending = @('VirtualMachinePlatform') | Where-Object {
        (Get-WindowsOptionalFeature -Online -FeatureName $_ -ErrorAction Stop).State -ne 'Enabled'
    }
    if ($pending) {
        Write-Host "`nWSL vient d'être activé ($($pending -join ', ')). REDÉMARRE le PC, puis relance le script." -ForegroundColor Yellow
        Pause-Exit 0
    }
    Invoke-Wsl --update | Write-Host
    Invoke-Wsl --set-default-version 2 | Out-Null

    # --- .wslconfig ----------------------------------------------------------
    Step 'Configuration des ressources (.wslconfig)'
    $cfgPath = Join-Path $env:USERPROFILE '.wslconfig'
    if (Test-Path $cfgPath) {
        $bak = "$cfgPath.bak-$(Get-Date -Format yyyyMMdd-HHmmss)"
        Copy-Item $cfgPath $bak -ErrorAction Stop
        Write-Host "Ancien fichier sauvegardé : $bak"
    }
    @"
[wsl2]
memory=24GB
swap=16GB
swapFile=D:\\WSL\\swap.vhdx
# WSL partage l'IP de Windows : QGroundControl voit le SITL sur localhost.
networkingMode=mirrored

[experimental]
sparseVhd=true
autoMemoryReclaim=gradual
"@ | Set-Content -Path $cfgPath -Encoding ASCII -ErrorAction Stop
    Write-Host "Écrit : $cfgPath"

    # --- Ubuntu ----------------------------------------------------------------
    $installed = (Invoke-Wsl --list --quiet) -split "`r?`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ }
    if ($installed -notcontains $Distro) {
        Step "Installation de $Distro dans $Root\$Distro"
        Write-Host @"

Ubuntu va démarrer et te demander un nom d'utilisateur puis un mot de passe
(rien ne s'affiche quand tu tapes le mot de passe, c'est normal).
Quand l'invite '$' apparaît, tape :  exit   puis Entrée.

"@ -ForegroundColor Yellow
        wsl.exe --install -d $Distro --location "$Root\$Distro"
        if ($LASTEXITCODE -ne 0) { throw "L'installation d'Ubuntu a échoué (code $LASTEXITCODE)." }
    } else {
        Write-Host "$Distro déjà installé, étape sautée."
    }

    Invoke-Wsl --shutdown | Out-Null

    Step 'Terminé'
    Write-Host "Ouvre 'Ubuntu 24.04', clone le dépôt puis lance scripts/setup-ubuntu.sh" -ForegroundColor Green
    Pause-Exit 0
}
catch {
    Write-Host "`nERREUR : $($_.Exception.Message)" -ForegroundColor Red
    Pause-Exit 1
}
