# Electronic Nose (E-Nose) - Robusta Coffee Aroma Profiling System

Frontend dashboard modern, premium, dan berkinerja tinggi untuk monitoring dan pengujian mutu aroma biji kopi **Robusta (*Coffea canephora*)**, dibangun dengan **HTML5, CSS3, dan JavaScript murni** dengan integrasi **Chart.js**, **jsPDF**, dan **html2canvas**.

---

## ☕ Khusus Kopi Robusta: Sensor Array & Senyawa Volatil (VOCs)

Sistem E-Nose ini dikhususkan untuk mendeteksi uap *headspace* aroma kopi Robusta dengan 4 channel sensor utama:

| Channel | Sensor | Senyawa Volatil Kopi yang Dideteksi | Peran dalam Analisis Kopi Robusta |
| :--- | :--- | :--- | :--- |
| **CH 01** | **TGS-2602** | **Pyrazines & Sulfida** | Karakter *earthy, roasted nuts*, dan aroma khas Robusta |
| **CH 02** | **MQ-135** | **Furans & Pyridines** | Aroma karamelisasi gula dan keasaman organik |
| **CH 03** | **MQ-3** | **Alcohols & Esters** | Mendeteksi level fermentasi dan potensi biji cacat (*defect / over-fermented*) |
| **CH 04** | **MQ-2** | **Roast Smoke & Hydrocarbons** | Indikator intensitas kematangan sangrai (*Light / Medium / Dark Roast*) |

---

## 🌟 Fitur Baru yang Disesuaikan untuk Kopi Robusta

1. **Prediksi & Evaluasi Mutu Kopi Realtime (Aroma Fingerprint)**:
   - Banner analitik otomatis yang mengevaluasi rasio voltase sensor secara realtime.
   - Mengklasifikasikan sampel:
     - **FINE ROBUSTA (GRADE 1)**: Aroma seimbang (*nutty/earthy* tinggi, alkohol/cacat rendah).
     - **DARK ROAST**: Pirolisis tinggi (*roast smoke* mendominasi).
     - **DEFECT / HONEY**: Peringatan fermentasi berlebih jika alkohol/asam asetat melonjak tajam.

2. **Metadata Sampel Kopi**:
   - Pilihan **Origin Kopi**: Robusta Dampit (Malang), Temanggung, Lampung Barat, Pupuan (Bali).
   - Pilihan **Tingkat Sangrai**: Medium Roast, Dark Roast, Light Roast, Fermented/Honey Process.
   - Kondisi Chamber Headspace: Suhu pemanasan sampel `40.5 °C` dan kelembaban `46 %RH`.

3. **Injeksi Aroma & Purge Chamber**:
   - Tombol **"Injeksi Aroma Kopi"**: Mensimulasikan hisapan uap headspace kopi ke dalam sensor chamber dengan dinamika respon yang berbeda-beda sesuai tingkat sangrai yang dipilih.
   - Tombol **"Purge Air"**: Mengalirkan udara bersih untuk membilas sensor chamber kembali ke baseline nol.

4. **Export Nota Digital Uji Mutu Kopi (PDF Slip ala Tokopedia)**:
   - Header resmi: *"ROBUSTA COFFEE QUALITY & AROMA SLIP"*.
   - Informasi lengkap: No. Laporan, Spesies (*Coffea canephora*), Asal Daerah, Tingkat Sangrai, Kondisi Headspace, Durasi Ekstraksi.
   - **Snapshot Grafik**: Kurva respon sensor langsung disematkan.
   - **Tabel Respon Volatil**: Nilai Min, Rata-rata, dan Puncak ($V_{max}$) per senyawa aroma.
   - **Kesimpulan Evaluasi**: Grade mutu kopi & catatan sensorik.
   - Dua pilihan ukuran: **Struk 80mm POS Roll** dan **Lembar A4 Standar**.

5. **Export CSV Lengkap**:
   - Data rekaman telemetri mencakup label origin, roast level, dan tegangan tiap sensor per detik.

---

## 🚀 Cara Menjalankan di VS Code

1. Di terminal PowerShell VS Code:
   ```powershell
   Start-Process "d:\electronic-nose-dashboard\index.html"
   ```
   *atau buka menggunakan ekstensi Live Server.*

2. Klik **"Connect Device"** di sidebar kiri.
3. Pilih Origin dan Roast Level, lalu klik **"Injeksi Aroma Kopi"** di kanan atas untuk melihat lonjakan grafik respon uap kopi Robusta.
4. Klik **"Export Coffee Quality Slip (PDF)"** untuk mencetak nota hasil uji laboratorium digital!
