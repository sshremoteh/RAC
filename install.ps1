# RAC (Rust Android Compiler) Installer for Windows (PowerShell)
# Usage:
#   irm https://raw.githubusercontent.com/sshremoteh/RAC/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "sshremoteh/RAC"
$BinaryName = "rac.exe"
$InstallDir = Join-Path $HOME ".rac\bin"
$TargetBin = Join-Path $InstallDir $BinaryName

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host " 🚀 RAC (Rust Android Compiler) Installer " -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "[INFO] Target installation directory: $InstallDir" -ForegroundColor Blue

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$Installed = $false

# 1. Check if running from local repo build
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path -ErrorAction SilentlyContinue
if ($ScriptDir) {
    $LocalBin = Join-Path $ScriptDir "target\release\$BinaryName"
    if (Test-Path $LocalBin) {
        Write-Host "[INFO] Found local compiled binary at $LocalBin" -ForegroundColor Blue
        Copy-Item -Path $LocalBin -Destination $TargetBin -Force
        $Installed = $true
    }
}

# 2. Try to download prebuilt binary from GitHub Releases
if (-not $Installed) {
    $Arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }
    $ReleaseUrl = "https://github.com/$Repo/releases/latest/download/rac-$Arch-pc-windows-msvc.zip"

    Write-Host "[INFO] Checking for prebuilt release at GitHub..." -ForegroundColor Blue
    $TempZip = Join-Path $env:TEMP "rac-windows.zip"
    $TempExtract = Join-Path $env:TEMP "rac-extract"

    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $ReleaseUrl -OutFile $TempZip -UseBasicParsing
        if (Test-Path $TempExtract) { Remove-Item -Recurse -Force $TempExtract }
        Expand-Archive -Path $TempZip -DestinationPath $TempExtract -Force

        $DownloadedExe = Get-ChildItem -Path $TempExtract -Filter $BinaryName -Recurse | Select-Object -First 1
        if ($DownloadedExe) {
            Copy-Item -Path $DownloadedExe.FullName -Destination $TargetBin -Force
            $Installed = $true
            Write-Host "[SUCCESS] Downloaded and extracted prebuilt binary." -ForegroundColor Green
        }
        Remove-Item -Force $TempZip -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force $TempExtract -ErrorAction SilentlyContinue
    } catch {
        Write-Host "[WARN] Prebuilt Windows binary not found on GitHub release." -ForegroundColor Yellow
    }
}

# 3. Fallback to Cargo if installed
if (-not $Installed) {
    $Cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($Cargo) {
        Write-Host "[INFO] Compiling from source using Cargo..." -ForegroundColor Blue
        cargo install --git "https://github.com/$Repo.git" rac-cli --bin rac --root (Join-Path $HOME ".rac")
        $Installed = $true
    } else {
        Write-Host "[ERROR] Could not download prebuilt release and Cargo is not installed." -ForegroundColor Red
        Write-Host "[ERROR] Please install Rust from https://rustup.rs or check https://github.com/$Repo/releases" -ForegroundColor Red
        exit 1
    }
}

# 4. Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    $NewUserPath = "$UserPath;$InstallDir"
    [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
    Write-Host "[INFO] Added $InstallDir to User Environment PATH." -ForegroundColor Blue
}

# Also update current session PATH
if ($env:Path -notlike "*$InstallDir*") {
    $env:Path = "$env:Path;$InstallDir"
}

# 5. Verify installation
if (Test-Path $TargetBin) {
    Write-Host ""
    Write-Host "[SUCCESS] RAC successfully installed to $TargetBin!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Get started with:" -ForegroundColor Cyan
    Write-Host "  rac new my_app" -ForegroundColor White
    Write-Host "  rac build my_app" -ForegroundColor White
    Write-Host ""
    Write-Host "Restart your PowerShell terminal or Windows Terminal to apply PATH changes." -ForegroundColor Yellow
} else {
    Write-Host "[ERROR] Verification failed. Binary not found." -ForegroundColor Red
    exit 1
}
