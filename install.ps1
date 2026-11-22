$ErrorActionPreference = "Stop"
$Repo = "linhhqth08598-droid/Dashboard_BotDiscord_App"

Clear-Host
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "      CAI DAT DISCORD BOT DASHBOARD APP           " -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "[1/3] Dang kiem tra phien ban moi nhat..." -ForegroundColor Yellow
$ApiUrl = "https://api.github.com/repos/linhhqth08598-droid/Dashboard_BotDiscord_App/releases/latest"
$Release = Invoke-RestMethod -Uri $ApiUrl
$Asset = $Release.assets | Where-Object { $_.name -like "*x64-setup.exe" } | Select-Object -First 1

if (-not $Asset) {
    Write-Host "Loi: Khong tim thay file cai dat trong ban phat hanh!" -ForegroundColor Red
    exit 1
}

$InstallerPath = "$env:TEMP\$($Asset.name)"

Write-Host "[2/3] Dang tai $($Asset.name) ($($Release.tag_name))..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $InstallerPath

Write-Host "[3/3] Dang cai dat ung dung..." -ForegroundColor Green
Start-Process -FilePath $InstallerPath -ArgumentList "/S" -Wait

Remove-Item $InstallerPath -Force
Write-Host ""
Write-Host "Hoan tat! Ban co the mo Discord Bot Dashboard tu Start Menu hoac Desktop." -ForegroundColor Cyan
