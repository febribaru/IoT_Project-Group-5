# ==============================================================================
# Electronic Nose Robusta - Cloudflare Edge Online Tunnel Launcher (PowerShell)
# ==============================================================================

param (
    [int]$Port = 3000
)

$ToolsDir = $PSScriptRoot
$CloudflaredExe = Join-Path $ToolsDir "cloudflared.exe"
$CloudflaredUrl = "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe"

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "  ☕ E-NOSE ROBUSTA - CLOUDFLARE EDGE ONLINE TUNNEL" -ForegroundColor Yellow
Write-Host "======================================================================" -ForegroundColor Cyan

if (-not (Test-Path $CloudflaredExe)) {
    Write-Host "⬇️ Mengunduh Cloudflare Tunnel binary resmi..." -ForegroundColor Green
    Invoke-WebRequest -Uri $CloudflaredUrl -OutFile $CloudflaredExe
    Write-Host "✅ Selesai mengunduh cloudflared.exe!" -ForegroundColor Green
}

Write-Host "🚀 Menjalankan Edge Tunnel ke http://127.0.0.1:$Port..." -ForegroundColor Cyan
Write-Host "⏳ Tunggu URL publik https://*.trycloudflare.com muncul di bawah ini..." -ForegroundColor DarkGray

& $CloudflaredExe tunnel --url "http://127.0.0.1:$Port"

