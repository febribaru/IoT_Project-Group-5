@echo off
title Electronic Nose - Azure Cosmos DB Bridge
echo ========================================================
echo  Electronic Nose Robusta - Azure Cosmos DB Bridge
echo ========================================================
echo Menghubungkan ke Azure Cosmos DB: enose_db / telemetry...
python tools\cosmos_bridge.py
pause
