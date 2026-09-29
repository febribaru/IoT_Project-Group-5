@echo off
echo ============================================
echo  FLASH ESP32-S3 dengan Partition Table OTA
echo  Firmware v1.0.1 (Merah-Kuning-Hijau-Biru)
echo ============================================
echo.
echo Memastikan ESP32 sudah tercolok via USB...
echo.

cd /d "%~dp0"

echo [1/2] Build firmware v1.0.1...
cargo build --release --no-default-features --features v101
if errorlevel 1 (
    echo BUILD GAGAL! Cek error di atas.
    pause
    exit /b 1
)

echo.
echo [2/2] Flash ke ESP32 dengan partisi OTA...
echo        (nvs + otadata + ota_0 + ota_1 akan dibuat)
echo.
espflash flash --partition-table partitions.csv --monitor target\xtensa-esp32s3-none-elf\release\esp32-coffee

pause

