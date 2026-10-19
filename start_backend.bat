@echo off
title Electronic Nose Robusta - Backend & Web Dashboard
color 0A

echo ===================================================================
echo   ELECTRONIC NOSE ROBUSTA - BACKEND SERVER (Port 3000)
echo ===================================================================
echo.
echo Direktori Aktif: %~dp0
echo Membuka browser: http://localhost:3000
echo.

cd /d "%~dp0"

:: Buka browser secara otomatis setelah 2 detik di background
start "" /b cmd /c "timeout /t 2 /nobreak >nul & start http://localhost:3000"

:: Sinkronisasi otomatis binary terbaru dari target\release jika ada
if exist "target\release\enose-backend.exe" (
    copy /Y "target\release\enose-backend.exe" "enose-backend.exe" >nul 2>&1
)

:: Cek apakah enose-backend.exe ada
if exist "enose-backend.exe" (
    echo Menjalankan enose-backend.exe (Server + Native Cosmos DB Bridge)...
    enose-backend.exe
) else (
    echo enose-backend.exe tidak ditemukan di root, menjalankan via Cargo di folder backend...
    cd backend
    cargo run --release
)

pause

