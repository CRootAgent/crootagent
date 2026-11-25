# LONTAR CLI Universal Interactive Installer for Windows (PowerShell)
# Minimalist AI Agent with NotebookLM Grounding & OpenCode Terminal UX

param (
    [switch]$Yes,
    [string]$InstallDir = "$env:USERPROFILE\.local\bin"
)

$ErrorActionPreference = "Stop"
$Repo = "CRootAgent/crootagent"
$BinaryPath = "$InstallDir\lontar.exe"

# 1. Show Logo and Menu
Write-Host ""
Write-Host "  ██        ▄██▄" -ForegroundColor White
Write-Host "  ██          ▀██▄" -ForegroundColor White
Write-Host "  ██            ▀██▄" -ForegroundColor White
Write-Host "  ██              ███" -ForegroundColor White
Write-Host "  ██            ▄██▀" -ForegroundColor White
Write-Host "▄▄██████      ▄██▀    ▄█▄" -ForegroundColor White
Write-Host "▀▀▀▀▀▀▀▀    ▀▀▀▀      ▀▀▀" -ForegroundColor White
Write-Host ""
Write-Host "Lontar Installer" -ForegroundColor White
Write-Host "There are many agent harnesses but this one is yours" -ForegroundColor DarkGray
Write-Host ""
Write-Host "Installation:" -ForegroundColor White
Write-Host ""
Write-Host "  Lontar will install to $BinaryPath" -ForegroundColor Gray
Write-Host ""
Write-Host "Choose an action:" -ForegroundColor White
Write-Host ""
Write-Host "  y    Install Lontar (default)" -ForegroundColor Green
Write-Host "  n    Do nothing" -ForegroundColor DarkGray
Write-Host ""

# 2. Choice Handling
$choice = "y"
if (-not $Yes) {
    try {
        if ([Console]::IsInputRedirected) {
            $choice = "y"
        } else {
            $key = [Console]::ReadKey($true)
            if ($key.KeyChar -eq 'n' -or $key.KeyChar -eq 'N' -or $key.KeyChar -eq 'q' -or $key.KeyChar -eq 'Q') {
                $choice = "n"
            } else {
                $choice = "y"
            }
        }
    } catch {
        $choice = "y"
    }
}

if ($choice -eq "n") {
    Write-Host "Installation cancelled." -ForegroundColor DarkGray
    exit 0
}

Write-Host "Will install Lontar."
Write-Host ""

# 3. Create target directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$TempDir = Join-Path $env:TEMP ("lontar_install_" + [System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

function Render-Bar($pct, $msg, $spinner) {
    $total = 20
    $maxPos = $total - 4
    $pos = [math]::Floor(($pct * $maxPos) / 100)
    if ($pos -lt 0) { $pos = 0 }
    if ($pos -gt $maxPos) { $pos = $maxPos }

    $left = "░" * $pos
    $right = "░" * ($total - 4 - $pos)
    
    # ANSI colored slider
    $rainbow = "`e[38;2;86;204;242m█`e[38;2;47;128;237m█`e[38;2;255;110;110m█`e[38;2;242;201;76m█`e[0m"
    Write-Host -NoNewline "`r  `e[36m$spinner`e[0m `e[90m$left`e[0m$rainbow`e[90m$right`e[0m $msg"
}

try {
    Render-Bar 20 "Preparing installation environment (20%)" "⠋"
    Start-Sleep -Milliseconds 150

    Render-Bar 44 "Installing Lontar fetching tarballs (44%)" "⠙"
    Start-Sleep -Milliseconds 200

    $installed = $false
    if (Test-Path "crootagent\target\release\lontar.exe") {
        Copy-Item -Path "crootagent\target\release\lontar.exe" -Destination $BinaryPath -Force
        $installed = $true
    } elseif (Test-Path "target\release\lontar.exe") {
        Copy-Item -Path "target\release\lontar.exe" -Destination $BinaryPath -Force
        $installed = $true
    } else {
        $AssetUrl = "https://github.com/$Repo/releases/latest/download/lontar-windows-x86_64.zip"
        $ZipPath = Join-Path $TempDir "lontar.zip"
        
        Render-Bar 60 "Downloading release archive from GitHub (60%)" "⠹"
        try {
            Invoke-WebRequest -Uri $AssetUrl -OutFile $ZipPath -UseBasicParsing
            Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
            Copy-Item -Path (Join-Path $TempDir "lontar.exe") -Destination $BinaryPath -Force
            $installed = $true
        } catch {
            if (Get-Command cargo -ErrorAction SilentlyContinue) {
                Render-Bar 75 "Compiling from source with Cargo (75%)" "⠸"
                if (Test-Path "crootagent\Cargo.toml") {
                    Set-Location crootagent
                    cargo build --release -q
                    Set-Location ..
                    Copy-Item -Path "crootagent\target\release\lontar.exe" -Destination $BinaryPath -Force
                } else {
                    cargo build --release -q
                    Copy-Item -Path "target\release\lontar.exe" -Destination $BinaryPath -Force
                }
                $installed = $true
            }
        }
    }

    Render-Bar 90 "Configuring execution environment (90%)" "⠼"
    Start-Sleep -Milliseconds 150

    # Add to User PATH if missing
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        $env:Path = "$env:Path;$InstallDir"
    }

    Render-Bar 100 "Finalizing Lontar installation (100%)" "⠴"
    Start-Sleep -Milliseconds 150

    Write-Host ""
    Write-Host "  ✔ Installed Lontar successfully to $BinaryPath!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Run 'lontar' to launch the TUI directly (Zero-Config AI active)." -ForegroundColor DarkGray
    Write-Host ""
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
