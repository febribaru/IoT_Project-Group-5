#!/usr/bin/env python3
"""
==============================================================================
Electronic Nose Robusta - TinyML On-Device Model Trainer & Code Generator
==============================================================================
Membaca dataset sensor gas (CSV koma atau titik-koma), melatih neural network
MLP (Multi-Layer Perceptron: Input -> Hidden ReLU -> Output Softmax),
menghitung mean & std untuk Z-score normalization, dan secara otomatis
meng-generate kode Rust `esp32/models/model_data.rs` yang siap dikompilasi
di mikrokontroler ESP32-S3 (bare-metal no_std).

Dapat mendeteksi otomatis:
- Dataset 3 sensor (MQ7, MQ3, MQ8) dengan 5 kelas mutu (Grade 1..5)
- Dataset 8 sensor gas standar Robusta dengan 3 kelas mutu

Penggunaan:
    python tools/train_tinyml.py --data "dataset/Data_Kopi Aroma Sweet.csv"
    python tools/train_tinyml.py --data dataset/coffee_dataset_template.csv
==============================================================================
"""

import os
import sys
import json
import argparse
import numpy as np

# Force UTF-8 on Windows console
if sys.platform == "win32" and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass


def detect_delimiter(first_line: str) -> str:
    """Deteksi apakah CSV menggunakan koma ',' atau titik koma ';'"""
    if first_line.count(";") > first_line.count(","):
        return ";"
    return ","


def load_csv_dataset(csv_path: str):
    """Membaca file CSV dataset sensor dan label target secara otomatis."""
    if not os.path.exists(csv_path):
        raise FileNotFoundError(f"File dataset '{csv_path}' tidak ditemukan!")

    with open(csv_path, "r", encoding="utf-8", errors="ignore") as f:
        raw_lines = [line.strip() for line in f if line.strip() and not line.strip().startswith("#")]

    if not raw_lines:
        raise ValueError(f"Dataset '{csv_path}' kosong!")

    delimiter = detect_delimiter(raw_lines[0])
    raw_header = [col.strip().lstrip('\ufeff') for col in raw_lines[0].split(delimiter)]
    header_lower = [col.lower() for col in raw_header]

    print(f"ℹ️ Format CSV terdeteksi: pemisah '{delimiter}', total {len(raw_lines) - 1} baris.")
    print(f"ℹ️ Kolom header: {raw_header}")

    # Cari kolom target label / grade
    label_candidates = ["grade", "label", "class", "target", "quality", "mutu"]
    label_col_idx = -1
    for cand in label_candidates:
        if cand in header_lower:
            label_col_idx = header_lower.index(cand)
            break
    if label_col_idx == -1:
        label_col_idx = len(raw_header) - 1

    label_col_name = raw_header[label_col_idx]
    print(f"🎯 Kolom target: '{label_col_name}' (indeks {label_col_idx})")

    # Tentukan kolom fitur sensor
    # Cek apakah dataset khusus 3 sensor (MQ7, MQ3, MQ8)
    three_sensors = ["mq7", "mq3", "mq8"]
    if all(s in header_lower for s in three_sensors):
        feature_names = ["MQ7", "MQ3", "MQ8"]
        feat_indices = [header_lower.index(s) for s in three_sensors]
        print(f"✅ Terdeteksi spesifik 3 sensor kopi: {feature_names}")
    else:
        # Gunakan semua kolom numerik kecuali kolom label
        feat_indices = [i for i in range(len(raw_header)) if i != label_col_idx]
        feature_names = [raw_header[i] for i in feat_indices]
        print(f"ℹ️ Menggunakan fitur sensor ({len(feat_indices)} kolom): {feature_names}")

    X = []
    y_raw = []

    for line_num, line in enumerate(raw_lines[1:], start=2):
        parts = line.split(delimiter)
        if len(parts) <= max(max(feat_indices), label_col_idx):
            continue
        try:
            feats = [float(parts[idx].strip()) for idx in feat_indices]
            lbl = parts[label_col_idx].strip()
            X.append(feats)
            y_raw.append(lbl)
        except ValueError:
            continue

    X = np.array(X, dtype=np.float32)
    y_raw = np.array(y_raw)

    if len(X) == 0:
        raise ValueError("Tidak ada baris data valid yang berhasil dibaca!")

    # Format label target
    unique_vals = sorted(list(set(y_raw)), key=lambda x: int(x) if x.isdigit() else x)
    
    # Jika label berupa angka 1..5 atau 0..4
    if all(val.isdigit() for val in unique_vals):
        int_vals = [int(v) for v in unique_vals]
        if set(int_vals) == {1, 2, 3, 4, 5}:
            labels_ordered = ["Grade 1", "Grade 2", "Grade 3", "Grade 4", "Grade 5"]
            label_to_idx = {str(i): i - 1 for i in range(1, 6)}
        elif min(int_vals) == 0:
            labels_ordered = [f"Grade {v+1}" for v in int_vals]
            label_to_idx = {str(v): v for v in int_vals}
        else:
            labels_ordered = [f"Grade {v}" for v in int_vals]
            label_to_idx = {str(v): idx for idx, v in enumerate(int_vals)}
    else:
        labels_ordered = unique_vals
        label_to_idx = {lbl: idx for idx, lbl in enumerate(labels_ordered)}

    y = np.array([label_to_idx[str(lbl)] for lbl in y_raw], dtype=np.int32)

    print(f"📊 Dataset siap: {len(X):,} sampel, {X.shape[1]} fitur.")
    print(f"🏷️ Distribusi Kelas ({len(labels_ordered)}):")
    for idx, lbl in enumerate(labels_ordered):
        count = np.sum(y == idx)
        pct = (count / len(y)) * 100.0
        print(f"   [{idx}] {lbl:<15} : {count:,} sampel ({pct:.1f}%)")

    return X, y, feature_names, labels_ordered


def train_mlp_model(X, y, num_classes, hidden_dim=32, epochs=60, lr=0.08, batch_size=512):
    """
    Melatih Multi-Layer Perceptron (MLP):
    Input [N, D] -> Hidden [N, H] (ReLU) -> Output [N, C] (Softmax)
    Menggunakan Mini-Batch SGD dengan Momentum & L2 Regularization.
    """
    num_samples, num_features = X.shape

    # 1. Z-score normalization
    means = np.mean(X, axis=0)
    stds = np.std(X, axis=0)
    stds[stds < 1e-6] = 1.0
    X_norm = (X - means) / stds

    # 2. Train-Validation Split (85% Train, 15% Validation)
    np.random.seed(42)
    indices = np.random.permutation(num_samples)
    split_pt = int(0.85 * num_samples)
    train_idx = indices[:split_pt]
    val_idx = indices[split_pt:]

    X_train, y_train = X_norm[train_idx], y[train_idx]
    X_val, y_val = X_norm[val_idx], y[val_idx]

    # 3. Inisialisasi bobot He/Kaiming
    W1 = np.random.randn(num_features, hidden_dim).astype(np.float32) * np.sqrt(2.0 / num_features)
    b1 = np.zeros(hidden_dim, dtype=np.float32)
    W2 = np.random.randn(hidden_dim, num_classes).astype(np.float32) * np.sqrt(2.0 / hidden_dim)
    b2 = np.zeros(num_classes, dtype=np.float32)

    # Momentum buffers
    v_W1 = np.zeros_like(W1)
    v_b1 = np.zeros_like(b1)
    v_W2 = np.zeros_like(W2)
    v_b2 = np.zeros_like(b2)
    momentum = 0.9
    reg = 0.0005

    print(f"\n🚀 Memulai Training MLP ({num_features} -> {hidden_dim} -> {num_classes}) | {epochs} Epochs...")

    best_val_acc = 0.0
    best_weights = (W1.copy(), b1.copy(), W2.copy(), b2.copy())

    num_batches = len(X_train) // batch_size

    for ep in range(1, epochs + 1):
        perm = np.random.permutation(len(X_train))
        X_shuf = X_train[perm]
        y_shuf = y_train[perm]

        for b in range(num_batches):
            xb = X_shuf[b * batch_size : (b + 1) * batch_size]
            yb = y_shuf[b * batch_size : (b + 1) * batch_size]
            N = len(xb)

            # Forward
            z1 = xb @ W1 + b1
            a1 = np.maximum(0.0, z1)  # ReLU
            z2 = a1 @ W2 + b2
            shift_z2 = z2 - np.max(z2, axis=1, keepdims=True)
            exp_z2 = np.exp(shift_z2)
            probs = exp_z2 / np.sum(exp_z2, axis=1, keepdims=True)

            # Backward
            Y_onehot = np.zeros((N, num_classes), dtype=np.float32)
            Y_onehot[np.arange(N), yb] = 1.0

            dz2 = (probs - Y_onehot) / N
            dW2 = a1.T @ dz2 + reg * W2
            db2 = np.sum(dz2, axis=0)

            da1 = dz2 @ W2.T
            dz1 = da1 * (z1 > 0.0)
            dW1 = xb.T @ dz1 + reg * W1
            db1 = np.sum(dz1, axis=0)

            # Update with momentum
            v_W1 = momentum * v_W1 - lr * dW1
            v_b1 = momentum * v_b1 - lr * db1
            v_W2 = momentum * v_W2 - lr * dW2
            v_b2 = momentum * v_b2 - lr * db2

            W1 += v_W1
            b1 += v_b1
            W2 += v_W2
            b2 += v_b2

        # Validasi pada akhir epoch
        z1_val = np.maximum(0.0, X_val @ W1 + b1)
        z2_val = z1_val @ W2 + b2
        val_preds = np.argmax(z2_val, axis=1)
        val_acc = np.mean(val_preds == y_val) * 100.0

        if val_acc > best_val_acc:
            best_val_acc = val_acc
            best_weights = (W1.copy(), b1.copy(), W2.copy(), b2.copy())

        if ep % 10 == 0 or ep == epochs:
            print(f"   Epoch {ep:3d}/{epochs} | Val Akurasi: {val_acc:.2f}% (Terbaik: {best_val_acc:.2f}%)")

    W1, b1, W2, b2 = best_weights
    print(f"\n✅ Training selesai! Akurasi Validasi Terbaik: {best_val_acc:.2f}%")

    # Evaluasi Confusion Matrix pada data validasi
    z1_val = np.maximum(0.0, X_val @ W1 + b1)
    z2_val = z1_val @ W2 + b2
    val_preds = np.argmax(z2_val, axis=1)
    cm = np.zeros((num_classes, num_classes), dtype=int)
    for t, p in zip(y_val, val_preds):
        cm[t, p] += 1

    print("\n📈 Confusion Matrix Validasi (Baris: Aktual, Kolom: Prediksi):")
    print("      " + " ".join(f"G{i+1:<4}" for i in range(num_classes)))
    for i, row in enumerate(cm):
        print(f"  G{i+1}: " + " ".join(f"{val:4d}" for val in row))

    return W1, b1, W2, b2, means, stds, best_val_acc, cm.tolist()


def generate_rust_model_code(W1, b1, W2, b2, means, stds, feature_names, labels, output_file, accuracy=0.0):
    """
    Men-generate file Rust `model_data.rs` yang valid dan siap dikompilasi
    oleh ESP32 Rust bare-metal (no_std).
    """
    num_features, hidden_dim = W1.shape
    _, num_classes = W2.shape

    labels_rust = ",\n".join([f'    "{lbl}"' for lbl in labels])
    means_rust = ", ".join([f"{m:.4f}" for m in means])
    stds_rust = ", ".join([f"{s:.4f}" for s in stds])

    # W1: [NUM_FEATURES, HIDDEN_DIM]
    w1_rows = []
    for f in range(num_features):
        row_vals = ", ".join([f"{W1[f, h]:.6f}" for h in range(hidden_dim)])
        feat_label = feature_names[f] if f < len(feature_names) else f"Feat_{f}"
        w1_rows.append(f"    // Input {feat_label}\n    [{row_vals}],")
    w1_rust = "\n".join(w1_rows)

    b1_rust = ", ".join([f"{b1[h]:.6f}" for h in range(hidden_dim)])

    # W2: [HIDDEN_DIM, NUM_CLASSES]
    w2_rows = []
    for h in range(hidden_dim):
        row_vals = ", ".join([f"{W2[h, c]:.6f}" for c in range(num_classes)])
        w2_rows.append(f"    [{row_vals}], // Hidden neuron {h}")
    w2_rust = "\n".join(w2_rows)

    b2_rust = ", ".join([f"{b2[c]:.6f}" for c in range(num_classes)])

    rust_code = f"""//! Model Data & Bobot Neural Network On-Device (ESP32-S3 Bare-Metal no_std)
//! Klasifikasi Kopi Robusta ({num_features} Sensor -> {hidden_dim} Hidden -> {num_classes} Kelas Mutu)
//! Dihasilkan otomatis oleh tools/train_tinyml.py
//! Akurasi Validasi Model: {accuracy:.2f}%

pub const NUM_FEATURES: usize = {num_features};
pub const HIDDEN_DIM: usize = {hidden_dim};
pub const NUM_CLASSES: usize = {num_classes};

pub const CLASS_LABELS: [&str; NUM_CLASSES] = [
{labels_rust},
];

/// Nilai Mean Baseline [{", ".join(feature_names)}]
pub const FEATURE_MEANS: [f32; NUM_FEATURES] = [
    {means_rust},
];

/// Nilai Standard Deviasi [{", ".join(feature_names)}]
pub const FEATURE_STDS: [f32; NUM_FEATURES] = [
    {stds_rust},
];

/// Bobot Lapisan 1 (Input ke Hidden Layer: {num_features}x{hidden_dim})
pub const W1: [[f32; HIDDEN_DIM]; NUM_FEATURES] = [
{w1_rust}
];

/// Bias Lapisan 1 (Hidden Layer: {hidden_dim})
pub const B1: [f32; HIDDEN_DIM] = [
    {b1_rust},
];

/// Bobot Lapisan 2 (Hidden Layer ke Output: {hidden_dim}x{num_classes})
pub const W2: [[f32; NUM_CLASSES]; HIDDEN_DIM] = [
{w2_rust}
];

/// Bias Lapisan 2 (Output Layer: {num_classes})
pub const B2: [f32; NUM_CLASSES] = [
    {b2_rust},
];
"""

    os.makedirs(os.path.dirname(os.path.abspath(output_file)), exist_ok=True)
    with open(output_file, "w", encoding="utf-8") as f:
        f.write(rust_code)

    print(f"\n🎉 File model Rust berhasil dibuat: {output_file}")


def main():
    parser = argparse.ArgumentParser(description="E-Nose Robusta TinyML Trainer & Code Generator")
    parser.add_argument("--data", type=str, default="dataset/Data_Kopi Aroma Sweet.csv", help="Path file CSV dataset")
    parser.add_argument("--output", type=str, default="esp32/models/model_data.rs", help="Path file output Rust")
    parser.add_argument("--hidden", type=int, default=32, help="Jumlah neuron hidden layer (default 32)")
    parser.add_argument("--epochs", type=int, default=60, help="Jumlah epoch training (default 60)")
    parser.add_argument("--lr", type=float, default=0.08, help="Learning rate (default 0.08)")
    parser.add_argument("--batch_size", type=int, default=512, help="Ukuran mini-batch (default 512)")
    parser.add_argument("--meta", type=str, default="dataset/model_metadata.json", help="Path metadata JSON")
    args = parser.parse_args()

    print("=" * 70)
    print("  ☕ E-NOSE ROBUSTA - ON-DEVICE TINYML TRAINING PIPELINE")
    print("=" * 70)

    # 1. Muat dataset
    X, y, feature_names, labels = load_csv_dataset(args.data)

    # 2. Latih model MLP
    W1, b1, W2, b2, means, stds, acc, cm = train_mlp_model(
        X, y, num_classes=len(labels), hidden_dim=args.hidden,
        epochs=args.epochs, lr=args.lr, batch_size=args.batch_size
    )

    # 3. Generate file Rust model_data.rs
    generate_rust_model_code(
        W1, b1, W2, b2, means, stds, feature_names, labels, args.output, accuracy=acc
    )

    # 4. Simpan metadata JSON
    meta = {
        "dataset_file": args.data,
        "num_samples": len(X),
        "num_features": len(feature_names),
        "feature_names": feature_names,
        "hidden_dim": args.hidden,
        "num_classes": len(labels),
        "labels": labels,
        "accuracy": acc,
        "confusion_matrix": cm,
        "feature_means": means.tolist(),
        "feature_stds": stds.tolist(),
        "W1": W1.tolist(),
        "B1": b1.tolist(),
        "W2": W2.tolist(),
        "B2": b2.tolist()
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.meta)), exist_ok=True)
    with open(args.meta, "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)
    print(f"📝 Metadata disimpan: {args.meta}")

    print("\n" + "=" * 70)
    print("✅ Model siap dipakai di ESP32 & Dashboard!")
    print("=" * 70)


if __name__ == "__main__":
    main()
