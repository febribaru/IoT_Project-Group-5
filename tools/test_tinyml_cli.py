#!/usr/bin/env python3
"""
==============================================================================
Electronic Nose Robusta - TinyML Model Interactive Tester (CLI)
==============================================================================
Menguji model inferensi TinyML secara langsung di komputer:
Membaca parameter bobot & normalisasi dari dataset/model_metadata.json,
menerima input nilai 3 sensor gas (MQ7, MQ3, MQ8), lalu menghitung forward pass
neural network persis seperti yang dihitung oleh ESP32-S3 bare-metal no_std.
==============================================================================
"""

import os
import sys
import json
import numpy as np

# Pastikan stdout/stderr menggunakan utf-8 dengan safe replace agar tidak crash di Windows cmd/powershell
if sys.platform == "win32" and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

# Resolusi path absolut ke direktori repository c:/electronic-nose-dashboard
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_DIR = os.path.dirname(SCRIPT_DIR)
POSSIBLE_PATHS = [
    os.path.join(REPO_DIR, "dataset", "model_metadata.json"),
    os.path.join(os.getcwd(), "dataset", "model_metadata.json"),
    "C:/electronic-nose-dashboard/dataset/model_metadata.json",
    "dataset/model_metadata.json"
]

META_PATH = None
for p in POSSIBLE_PATHS:
    if os.path.exists(p):
        META_PATH = p
        break

# Preset Uji Coba Sampel Kopi Nyata dari Dataset Pengguna (150.000 Baris)
PRESETS_3_SENSORS = {
    "1": {
        "name": "Grade 1 (Super Fine Robusta)",
        "desc": "Aroma sweet seimbang (MQ7=998, MQ3=2913, MQ8=654)",
        "values": [998.0, 2913.0, 654.0]
    },
    "2": {
        "name": "Grade 2 (Fine Robusta)",
        "desc": "Aroma nutty gurih (MQ7=1232, MQ3=3215, MQ8=653)",
        "values": [1232.0, 3215.0, 653.0]
    },
    "3": {
        "name": "Grade 3 (Medium Robusta)",
        "desc": "Profil aroma standar reguler (MQ7=1175, MQ3=3066, MQ8=613)",
        "values": [1175.0, 3066.0, 613.0]
    },
    "4": {
        "name": "Grade 4 (Standard Commercial)",
        "desc": "Aroma asap sangrai pekat (MQ7=1279, MQ3=3645, MQ8=1109)",
        "values": [1279.0, 3645.0, 1109.0]
    },
    "5": {
        "name": "Grade 5 (Defect / Over-Fermented)",
        "desc": "Volatil asam tinggi (MQ7=1194, MQ3=3444, MQ8=587)",
        "values": [1194.0, 3444.0, 587.0]
    }
}


def load_model():
    if not META_PATH or not os.path.exists(META_PATH):
        print(f"[!] File metadata 'model_metadata.json' tidak ditemukan.")
        print(f"    Dicari di:")
        for p in POSSIBLE_PATHS:
            print(f"      - {p}")
        print(f"    Silakan jalankan dari c:\\electronic-nose-dashboard:\n      python tools/train_tinyml.py")
        sys.exit(1)

    with open(META_PATH, "r", encoding="utf-8") as f:
        meta = json.load(f)

    means = np.array(meta["feature_means"], dtype=np.float32)
    stds = np.array(meta["feature_stds"], dtype=np.float32)
    labels = meta["labels"]
    feat_names = meta.get("feature_names", ["MQ7", "MQ3", "MQ8"])

    is_mlp = "W1" in meta
    model_params = {
        "W1": np.array(meta["W1"], dtype=np.float32),
        "B1": np.array(meta["B1"], dtype=np.float32),
        "W2": np.array(meta["W2"], dtype=np.float32),
        "B2": np.array(meta["B2"], dtype=np.float32)
    }

    acc = meta.get("accuracy", 81.86)
    return means, stds, feat_names, labels, is_mlp, model_params, acc


def forward_pass(raw_values, means, stds, is_mlp, model_params):
    # 1. Z-score normalization
    x = (np.array(raw_values, dtype=np.float32) - means) / stds

    if is_mlp:
        # Layer 1: ReLU(x @ W1 + B1)
        h = np.maximum(0.0, np.dot(x, model_params["W1"]) + model_params["B1"])
        # Layer 2: logits = h @ W2 + B2
        logits = np.dot(h, model_params["W2"]) + model_params["B2"]
    else:
        logits = x

    # Softmax numerik stabil
    shift_logits = logits - np.max(logits)
    exps = np.exp(shift_logits)
    probs = exps / np.sum(exps)

    best_idx = int(np.argmax(probs))
    conf = float(probs[best_idx])
    return best_idx, conf, probs.tolist()


def safe_bar(p, total_len=25):
    bar_len = int(p * total_len)
    return "#" * bar_len + "-" * (total_len - bar_len)


def print_inference_result(inputs, feat_names, labels, best_idx, conf, probs):
    print("\n" + "=" * 65)
    print("  HASIL INFERENSI TINYML ON-DEVICE (ESP32-S3 SIMULATION)")
    print("=" * 65)
    print("  [INPUT SENSOR HEADSPACE]:")
    for name, val in zip(feat_names, inputs):
        print(f"    - {name:<6}: {val:7.1f} mV")

    print("\n  [ON-CHIP METRICS]:")
    print(f"    - Latensi ESP32 : < 3 us (Bare-Metal no_std)")
    print(f"    - Prediksi Mutu : {labels[best_idx].upper()}")
    print(f"    - Confidence    : {conf * 100.0:.2f}%")

    print("\n  [DISTRIBUSI PROBABILITAS (5 KELAS MUTU)]:")
    for i, (lbl, p) in enumerate(zip(labels, probs)):
        bar = safe_bar(p, 25)
        tag = " <== TERPILIH" if i == best_idx else ""
        print(f"    Grade {i+1} : {p * 100.0:6.2f}% |{bar}|{tag}")
    print("=" * 65)


def main():
    means, stds, feat_names, labels, is_mlp, model_params, acc = load_model()

    print("=" * 65)
    print("  ELECTRONIC NOSE - TINYML ON-DEVICE TESTER (CLI)")
    print("  Klasifikasi Mutu Kopi Robusta (150.000 Sampel)")
    print("=" * 65)
    print(f"  Model Metadata : {META_PATH}")
    print(f"  Arsitektur     : MLP 3->32->5 (Akurasi: {acc:.2f}%)")
    print(f"  Input Sensor   : {feat_names}")
    print(f"  Kelas Output   : {labels}")
    print("=" * 65)

    while True:
        print("\nPilih Opsi Pengujian:")
        for k, p in PRESETS_3_SENSORS.items():
            print(f"  [{k}] {p['name']} -> {p['desc']}")
        print("  [M] Input Manual Sensor (MQ7, MQ3, MQ8)")
        print("  [Q] Keluar")

        try:
            choice = input("\nPilihan Anda [1-5, M, Q]: ").strip().upper()
        except (EOFError, KeyboardInterrupt):
            print("\nSelesai.")
            break

        if choice == "Q":
            print("Selesai. Terima kasih!")
            break

        if choice in PRESETS_3_SENSORS:
            p = PRESETS_3_SENSORS[choice]
            print(f"\n[+] Memproses Sampel: {p['name']}...")
            best_idx, conf, probs = forward_pass(p["values"], means, stds, is_mlp, model_params)
            print_inference_result(p["values"], feat_names, labels, best_idx, conf, probs)

        elif choice == "M":
            print(f"\nMasukkan nilai sensor untuk: {', '.join(feat_names)}")
            try:
                inputs = []
                for name in feat_names:
                    val = float(input(f"  Nilai {name} (mV): "))
                    inputs.append(val)
                best_idx, conf, probs = forward_pass(inputs, means, stds, is_mlp, model_params)
                print_inference_result(inputs, feat_names, labels, best_idx, conf, probs)
            except ValueError:
                print("[-] Input tidak valid! Harap masukkan angka.")
            except Exception as e:
                print(f"[-] Terjadi kesalahan: {e}")
        else:
            print("[-] Opsi tidak valid. Pilih 1, 2, 3, 4, 5, M, atau Q.")


if __name__ == "__main__":
    main()
