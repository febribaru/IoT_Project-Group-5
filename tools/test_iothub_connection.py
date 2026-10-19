"""
Verifikasi Koneksi Azure IoT Hub (Akun Teman)
Menguji apakah connection string dari teman valid dan diterima oleh Azure IoT Hub.
"""
import base64
import hmac
import hashlib
import time
import urllib.parse
import requests

import os

def test_iothub():
    connection_string = os.getenv("AZURE_DEVICE_CONNECTION_STRING", "HostName=enose-robusta-hub123.azure-devices.net;DeviceId=enose-robusta-device-01;SharedAccessKey=YOUR_AZURE_IOTHUB_KEY")
    
    # Parse connection string
    parts = dict(part.split('=', 1) for part in connection_string.split(';') if '=' in part)
    host = parts.get('HostName')
    device = parts.get('DeviceId')
    key = parts.get('SharedAccessKey')

    print("==================================================")
    print(" Pengujian Koneksi Azure IoT Hub (Akun Teman)")
    print(f" HostName : {host}")
    print(f" DeviceId : {device}")
    print("==================================================")

    # Buat SAS Token
    expiry = int(time.time()) + 3600
    uri = f"{host}/devices/{device}"
    encoded_uri = urllib.parse.quote(uri, safe='')
    string_to_sign = f"{encoded_uri}\n{expiry}"
    key_bytes = base64.b64decode(key)
    signature = base64.b64encode(hmac.new(key_bytes, string_to_sign.encode('utf-8'), hashlib.sha256).digest()).decode('utf-8')
    encoded_sig = urllib.parse.quote(signature, safe='')
    token = f"SharedAccessSignature sr={encoded_uri}&sig={encoded_sig}&se={expiry}"

    url = f"https://{host}/devices/{device}/messages/events?api-version=2021-04-12"
    headers = {
        "Authorization": token,
        "Content-Type": "application/json"
    }

    # Paket telemetri uji
    payload = {
        "device_id": device,
        "coffee_name": "Robusta Dampit",
        "coffee_id": "C05",
        "measurement_id": "M01",
        "status": "online_test",
        "source": "E-Nose Robusta Backend",
        "timestamp": time.strftime('%Y-%m-%d %H:%M:%S')
    }

    print("Mengirim pesan telemetri uji ke Azure IoT Hub...")
    try:
        resp = requests.post(url, headers=headers, json=payload, timeout=10)
        if resp.status_code == 204:
            print("--------------------------------------------------")
            print(" [STATUS: BERHASIL! (HTTP 204 No Content)]")
            print(" Pesan telemetri resmi DITERIMA oleh Azure IoT Hub!")
            print(f" Device '{device}' terotentikasi penuh di cloud.")
            print("--------------------------------------------------")
            return True
        else:
            print(f" [STATUS: GAGAL] HTTP {resp.status_code}: {resp.text}")
            return False
    except Exception as e:
        print(f" [ERROR] Gagal menghubungi endpoint: {e}")
        return False

if __name__ == "__main__":
    test_iothub()
