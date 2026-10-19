@echo off
title Flash ESP32-S3 E-Nose Robusta
color 0B

echo ===================================================================
echo   PILIHAN FLASH FIRMWARE ESP32-S3 (BARE-METAL NO_STD)
echo ===================================================================
echo.
echo Pastikan ESP32-S3 sudah terhubung ke PC lewat kabel USB.
echo.
echo  [1] Flash Firmware v1.2.0 (Rekomendasi • ThingsBoard Sync & IoT Nirkabel)
echo  [2] Flash Firmware v1.0.8 (DHT Support & Real Reading OK)
echo  [3] Flash Firmware v1.0.9 (Indikator STOP DHTMATI)
echo  [4] Flash Firmware v1.0.7
echo  [5] Buka Serial Monitor (espflash monitor)
echo  [6] Keluar
echo.
set /p opt="Pilih opsi [1-6] (default: 1): "

if "%opt%"=="" set opt=1
if "%opt%"=="1" goto flash120
if "%opt%"=="2" goto flash108
if "%opt%"=="3" goto flash109
if "%opt%"=="4" goto flash107
if "%opt%"=="5" goto monitor
if "%opt%"=="6" goto exit

:flash120
echo.
echo === Mereset OTA Boot Slot & Melakukan Flash Firmware v1.2.0 ke ESP32-S3 (COM6)... ===
espflash erase-region --port COM6 --chip esp32s3 0xd000 0x2000
espflash write-bin --port COM6 --chip esp32s3 0x10000 "backend\firmware\bin\firmware_v1.2.0.bin"
goto finish

:flash108
echo.
echo === Mereset OTA Boot Slot & Melakukan Flash Firmware v1.0.8 ke ESP32-S3 (COM6)... ===
espflash erase-region --port COM6 --chip esp32s3 0xd000 0x2000
espflash write-bin --port COM6 --chip esp32s3 0x10000 "backend\firmware\bin\firmware_v1.0.8.bin"
goto finish

:flash109
echo.
echo === Mereset OTA Boot Slot & Melakukan Flash Firmware v1.0.9 ke ESP32-S3 (COM6)... ===
espflash erase-region --port COM6 --chip esp32s3 0xd000 0x2000
espflash write-bin --port COM6 --chip esp32s3 0x10000 "backend\firmware\bin\firmware_v1.0.9.bin"
goto finish

:flash107
echo.
echo === Mereset OTA Boot Slot & Melakukan Flash Firmware v1.0.7 ke ESP32-S3 (COM6)... ===
espflash erase-region --port COM6 --chip esp32s3 0xd000 0x2000
espflash write-bin --port COM6 --chip esp32s3 0x10000 "backend\firmware\bin\firmware_v1.0.7.bin"
goto finish

:monitor
echo.
echo === Membuka Serial Monitor ESP32-S3... ===
espflash monitor
goto finish

:finish
echo.
echo Selesai!
pause
:exit

