Write-Host "🦀 Ferris.uz CLI (ferris-cli) Windows uchun yuklanmoqda..." -ForegroundColor Cyan

$installDir = "$env:LOCALAPPDATA\Programs\ferris"
if (!(Test-Path -Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

$exePath = "$installDir\ferris.exe"
Invoke-WebRequest -Uri "https://ferris.uz/downloads/ferris.exe" -OutFile $exePath

$userPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", [EnvironmentVariableTarget]::User)
}

Write-Host "✅ ferris-cli muvaffaqiyatli o'rnatildi: $exePath" -ForegroundColor Green
Write-Host "Terminalni qayta ochib 'ferris --help' buyrug'ini tering!" -ForegroundColor Yellow
