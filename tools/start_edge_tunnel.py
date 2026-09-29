#!/usr/bin/env python3
"""
==============================================================================
Electronic Nose Robusta - Cloudflare Edge Online Tunnel Launcher
==============================================================================
Membuka akses publik HTTPS & WebSocket (WSS) ke dashboard lokal port 3000
secara instan melalui Cloudflare Edge Quick Tunnel (tanpa registrasi/login).

Penggunaan:
    python tools/start_edge_tunnel.py
    python tools/start_edge_tunnel.py --port 3000
==============================================================================
"""

import os
import re
import sys
import time
import urllib.request
import subprocess
import argparse

# Force UTF-8 on Windows console
if sys.platform == "win32" and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

CLOUDFLARED_URL = "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe"
TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
CLOUDFLARED_PATH = os.path.join(TOOLS_DIR, "cloudflared.exe")


def ensure_cloudflared_binary():
    """Memastikan binary cloudflared.exe tersedia di folder tools."""
    if os.path.exists(CLOUDFLARED_PATH):
        return CLOUDFLARED_PATH

    print("⬇️ Cloudflared belum terpasang. Mengunduh Cloudflare Tunnel binary resmi (sekali saja)...")
    try:
        def reporthook(blocknum, blocksize, totalsize):
            read = blocknum * blocksize
            if totalsize > 0:
                percent = min(100, read * 100 // totalsize)
                sys.stdout.write(f"\r   Progres unduhan: {percent}% ({read // (1024*1024)} MB / {totalsize // (1024*1024)} MB)")
                sys.stdout.flush()

        urllib.request.urlretrieve(CLOUDFLARED_URL, CLOUDFLARED_PATH, reporthook)
        print("\n✅ Cloudflared berhasil diunduh!")
        return CLOUDFLARED_PATH
    except Exception as e:
        print(f"\n❌ Gagal mengunduh cloudflared otomatis: {e}")
        print(f"👉 Anda dapat mengunduh manual dari:\n   {CLOUDFLARED_URL}\n   Lalu simpan sebagai: {CLOUDFLARED_PATH}")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description="Launch Cloudflare Edge Online Tunnel for E-Nose Dashboard")
    parser.add_argument("--port", type=int, default=3000, help="Port lokal server backend dashboard (default 3000)")
    args = parser.parse_args()

    exe_path = ensure_cloudflared_binary()

    print("=" * 70)
    print("  🌐 E-NOSE ROBUSTA - CLOUDFLARE EDGE ONLINE TUNNEL")
    print("=" * 70)
    print(f"🚀 Membuka tunnel aman ke http://127.0.0.1:{args.port}...")
    print("⏳ Menghubungkan ke jaringan edge global Cloudflare...\n")

    cmd = [exe_path, "tunnel", "--url", f"http://127.0.0.1:{args.port}"]
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace"
    )

    tunnel_url = None
    url_pattern = re.compile(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com")

    try:
        while True:
            line = proc.stdout.readline()
            if not line and proc.poll() is not None:
                break
            if line:
                match = url_pattern.search(line)
                if match and not tunnel_url:
                    tunnel_url = match.group(0)
                    print("\n" + "=" * 70)
                    print("  🎉 DASHBOARD ANDA KINI ONLINE DI INTERNET!")
                    print("=" * 70)
                    print(f"  👉 URL Publik:  {tunnel_url}")
                    print(f"  👉 Target Port: http://127.0.0.1:{args.port}")
                    print("=" * 70)
                    print("📱 Anda dapat membuka URL di atas dari:")
                    print("   - Smartphone (Android / iPhone) dari mana saja")
                    print("   - Laptop / Tablet di luar jaringan Wi-Fi lokal")
                    print("   - Protokol: HTTPS & WSS (WebSocket aman) aktif otomatis")
                    print("\nTekan Ctrl+C untuk menghentikan tunnel.\n")
                    print("-" * 70)

                # Tampilkan log aktivitas jika tunnel sudah aktif
                if "Registered tunnel connection" in line or "Connection" in line:
                    clean_line = line.strip()
                    if clean_line:
                        print(f"📡 [Tunnel] {clean_line}")

    except KeyboardInterrupt:
        print("\n🛑 Menghentikan Cloudflare Edge Tunnel...")
        proc.terminate()
        proc.wait()
        print("✅ Tunnel ditutup. Dashboard kembali ke mode lokal saja.")


if __name__ == "__main__":
    main()

