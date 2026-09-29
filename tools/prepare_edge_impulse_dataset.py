#!/usr/bin/env python3
"""
==============================================================================
Edge Impulse & TinyML Dataset Converter & Packager
==============================================================================
Mengonversi dataset Robusta (MQ7, MQ3, MQ8, Grade 1..5) menjadi format standar
yang 100% kompatibel dengan Edge Impulse Studio (Time-Series & Tabular):
1. dataset/coffee_dataset_edge_impulse.csv (Tabular comma-separated, label Grade 1..5)
2. dataset/edge_impulse_5grades_timeseries.zip (Package ZIP multi-sample siap upload)
3. Kompatibel penuh dengan tools/train_tinyml.py untuk ESP32 firmware
==============================================================================
"""

import os
import sys
import csv
import zipfile

if sys.platform == "win32" and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

def main():
    root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    data_kopi_path = os.path.join(root_dir, 'dataset', 'Data_Kopi Aroma Sweet.csv')
    out_csv_path = os.path.join(root_dir, 'dataset', 'coffee_dataset_edge_impulse.csv')
    out_zip_path = os.path.join(root_dir, 'dataset', 'edge_impulse_5grades_timeseries.zip')

    if not os.path.exists(data_kopi_path):
        print(f"Error: Dataset {data_kopi_path} tidak ditemukan!")
        sys.exit(1)

    print(f"Membaca dataset: {data_kopi_path}...")
    samples_by_grade = {1: [], 2: [], 3: [], 4: [], 5: []}
    
    with open(data_kopi_path, 'r', encoding='utf-8-sig', errors='ignore') as f:
        # Deteksi pemisah
        first_line = f.readline()
        delimiter = ';' if ';' in first_line else ','
        f.seek(0)
        reader = csv.reader(f, delimiter=delimiter)
        header = next(reader)

        for row in reader:
            if len(row) >= 4:
                try:
                    mq7 = float(row[0].strip())
                    mq3 = float(row[1].strip())
                    mq8 = float(row[2].strip())
                    grade = int(float(row[3].strip()))
                    if grade in samples_by_grade:
                        samples_by_grade[grade].append((mq7, mq3, mq8))
                except ValueError:
                    continue

    for g in range(1, 6):
        print(f"Grade {g}: {len(samples_by_grade[g])} baris sampel")

    # 1. Buat file CSV Tabular standar (Comma separated)
    print(f"\nMembuat CSV tabular standar: {out_csv_path}...")
    with open(out_csv_path, 'w', newline='', encoding='utf-8') as f:
        writer = csv.writer(f, delimiter=',')
        writer.writerow(['mq7', 'mq3', 'mq8', 'grade', 'label'])
        for g in range(1, 6):
            # Ambil 2000 sampel per grade (total 10.000 sampel seimbang)
            for mq7, mq3, mq8 in samples_by_grade[g][:2000]:
                writer.writerow([int(mq7), int(mq3), int(mq8), g, f"Grade {g}"])

    print(f"File CSV tabular berhasil dibuat: {out_csv_path}")

    # 2. Buat ZIP package berisi file Time-Series per sampel untuk Edge Impulse
    # Edge Impulse format: timestamp,mq7,mq3,mq8
    # Filename format: [Label].[SampleName].csv (Edge Impulse otomatis mengenali label dari nama file!)
    print(f"\nMembuat ZIP package time-series Edge Impulse: {out_zip_path}...")
    samples_per_file = 40  # 10 detik pengujian @ 4Hz (interval 250ms)
    num_files_per_grade = 30  # 30 file per grade x 5 grade = 150 file sampel
    total_written = 0

    with zipfile.ZipFile(out_zip_path, 'w', zipfile.ZIP_DEFLATED) as z:
        for g in range(1, 6):
            grade_label = f"Grade_{g}"
            data = samples_by_grade[g]
            for file_idx in range(num_files_per_grade):
                start = file_idx * samples_per_file
                chunk = data[start:start + samples_per_file]
                if len(chunk) < samples_per_file:
                    break

                csv_content = ["timestamp,MQ7,MQ3,MQ8\n"]
                for t_idx, (mq7, mq3, mq8) in enumerate(chunk):
                    ts_ms = t_idx * 250
                    csv_content.append(f"{ts_ms},{int(mq7)},{int(mq3)},{int(mq8)}\n")

                # Nama file: Grade_1.sample_01.csv -> Label otomatis 'Grade_1' di Edge Impulse!
                file_name = f"{grade_label}.sample_{file_idx+1:02d}.csv"
                z.writestr(file_name, "".join(csv_content))
                total_written += 1

    print(f"ZIP package berhasil dibuat: {out_zip_path} ({total_written} file time-series multi-grade)")
    print("\nPetunjuk untuk Pengguna:")
    print("1. Di Edge Impulse -> 'Data acquisition' -> 'Upload data'")
    print("2. Pilih file 'dataset/edge_impulse_5grades_timeseries.zip'")
    print("3. Edge Impulse otomatis mengekstrak semua 5 grade (Grade_1 s/d Grade_5), 3 axis sensor (MQ7, MQ3, MQ8) tanpa potongan kecil!")

if __name__ == '__main__':
    main()

