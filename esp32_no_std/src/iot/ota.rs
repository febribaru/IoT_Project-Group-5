//! ThingsBoard Cloud & Local Hybrid OTA + Cloud Telemetry Relay Client (Bare-Metal no_std)
//!
//! Fitur Utama:
//! 1. Direct Cloud Telemetry & Heartbeat ke ThingsBoard Cloud (54.208.96.163:80 / thingsboard.cloud)
//!    → Membuat status device di ThingsBoard otomatis ACTIVE begitu tersambung ke Hotspot manapun!
//! 2. Cross-Hotspot Cloud Relay (Client Attributes `live_telemetry` & `cloud_cmd`)
//!    → Memungkinkan Laptop / Backend / Vercel Dashboard berbeda Wi-Fi tetap terhubung real-time!
//! 3. Direct ThingsBoard Cloud OTA & Local Server OTA Fallback
//!    → Mendukung update firmware v1.0.1 - v1.0.9 langsung dari ThingsBoard Cloud maupun Backend Lokal.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;

/// Flag global yang menandakan proses download & flash OTA sedang berjalan (menghentikan sementara siklus sensor)
static OTA_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Cek apakah ESP32 sedang dalam mode fokus eksklusif OTA
pub fn is_ota_in_progress() -> bool {
    OTA_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Versi firmware aktif
#[cfg(feature = "v101")]
pub const CURRENT_VERSION: &str = "1.0.1";
#[cfg(feature = "v102")]
pub const CURRENT_VERSION: &str = "1.0.2";
#[cfg(feature = "v103")]
pub const CURRENT_VERSION: &str = "1.0.3";
#[cfg(feature = "v104")]
pub const CURRENT_VERSION: &str = "1.0.4";
#[cfg(feature = "v105")]
pub const CURRENT_VERSION: &str = "1.0.5";
#[cfg(feature = "v106")]
pub const CURRENT_VERSION: &str = "1.0.6";
#[cfg(feature = "v107")]
pub const CURRENT_VERSION: &str = "1.0.7";
#[cfg(feature = "v108")]
pub const CURRENT_VERSION: &str = "1.0.8";
#[cfg(feature = "v109")]
pub const CURRENT_VERSION: &str = "1.0.9";
#[cfg(feature = "v110")]
pub const CURRENT_VERSION: &str = "1.1.0";
#[cfg(feature = "v120")]
pub const CURRENT_VERSION: &str = "1.2.0";
#[cfg(not(any(
    feature = "v101",
    feature = "v102",
    feature = "v103",
    feature = "v104",
    feature = "v105",
    feature = "v106",
    feature = "v107",
    feature = "v108",
    feature = "v109",
    feature = "v110",
    feature = "v120"
)))]
pub const CURRENT_VERSION: &str = "1.0.8";

/// Access token ThingsBoard untuk autentikasi OTA & Telemetri Cloud
pub const THINGSBOARD_ACCESS_TOKEN: &str = crate::config::THINGSBOARD_ACCESS_TOKEN;

const SERVER_PORT: u16 = 3000;
const TB_PORT: u16 = crate::config::THINGSBOARD_PORT;
const SECTOR_SIZE: usize = 4096;

// ── Static buffers di BSS (bukan stack) ───────────────────────────────────────

static mut CHK_RX: [u8; 2048] = [0u8; 2048];
static mut CHK_TX: [u8; 2048] = [0u8; 2048];
static mut DL_RX:  [u8; 16384] = [0u8; 16384];
static mut DL_TX:  [u8; 512]   = [0u8; 512];
static mut HDR:    [u8; 2048]  = [0u8; 2048];
static mut SECTOR: [u8; 4096]  = [0u8; 4096];
static mut CHUNK:  [u8; 4096]  = [0u8; 4096];

// Buffer telemetri terbaru untuk dikirim ke Cloud Relay (Cross-Hotspot)
static mut CLOUD_TELEM_BUF: [u8; 1536] = [0u8; 1536];
static mut CLOUD_TELEM_LEN: usize = 0;
static PENDING_CLOUD_CMD: AtomicU8 = AtomicU8::new(0); // 0=None, 1=Start, 2=Stop, 3=Purge
static LAST_CMD_SEQ: AtomicU32 = AtomicU32::new(0);

/// Gerbang Utama OTA: ESP32 HANYA melakukan OTA jika ada permintaan eksplisit
/// dari tombol 'Assign & Push Package' di Web Dashboard (cloud_cmd = "OTA").
static OTA_TRIGGER_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Simpan snapshot JSON telemetri terbaru dari loop sensor utama untuk dikirim ke Cloud
pub fn update_cloud_telemetry(json_str: &str) {
    let bytes = json_str.as_bytes();
    if bytes.is_empty() || bytes.len() > 1500 {
        return;
    }
    unsafe {
        let ptr = &raw mut CLOUD_TELEM_BUF;
        (&mut (*ptr))[..bytes.len()].copy_from_slice(bytes);
        CLOUD_TELEM_LEN = bytes.len();
    }
}

/// Ambil perintah remote (START/STOP/PURGE) yang diterima dari Cloud (lintas hotspot)
pub fn take_cloud_command() -> Option<crate::uart::SerialCommand> {
    match PENDING_CLOUD_CMD.swap(0, Ordering::SeqCst) {
        1 => Some(crate::uart::SerialCommand::Start),
        2 => Some(crate::uart::SerialCommand::Stop),
        3 => Some(crate::uart::SerialCommand::Purge),
        _ => None,
    }
}

// ── Utilitas ──────────────────────────────────────────────────────────────────

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() { return None; }
    'o: for i in 0..=(hay.len() - needle.len()) {
        for (j, &nb) in needle.iter().enumerate() {
            if hay[i + j] != nb { continue 'o; }
        }
        return Some(i);
    }
    None
}

/// Case-insensitive byte search untuk HTTP headers (misal `content-length:` vs `Content-Length:`)
fn find_bytes_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() { return None; }
    'o: for i in 0..=(hay.len() - needle.len()) {
        for (j, &nb) in needle.iter().enumerate() {
            if !hay[i + j].eq_ignore_ascii_case(&nb) { continue 'o; }
        }
        return Some(i);
    }
    None
}

/// Ekstrak nilai string JSON: `"key":"<value>"`
fn json_str<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut pat: heapless::Vec<u8, 80> = heapless::Vec::new();
    let _ = pat.push(b'"');
    for &b in key { let _ = pat.push(b); }
    let _ = pat.extend_from_slice(b"\":\"");
    let start = find_bytes(json, pat.as_slice())? + pat.len();
    let end = find_bytes(&json[start..], b"\"")?;
    Some(&json[start..start + end])
}

/// Ekstrak nilai integer JSON: `"key":123`
fn json_u32(json: &[u8], key: &[u8]) -> Option<u32> {
    let mut pat: heapless::Vec<u8, 80> = heapless::Vec::new();
    let _ = pat.push(b'"');
    for &b in key { let _ = pat.push(b); }
    let _ = pat.extend_from_slice(b"\":");
    let mut start = find_bytes(json, pat.as_slice())? + pat.len();
    while start < json.len() && (json[start] == b' ' || json[start] == b'"') {
        start += 1;
    }
    let mut end = start;
    while end < json.len() && json[end].is_ascii_digit() {
        end += 1;
    }
    if end == start { return None; }
    core::str::from_utf8(&json[start..end]).ok()?.parse().ok()
}

fn hdr_end(data: &[u8]) -> Option<usize> {
    find_bytes(data, b"\r\n\r\n")
}

fn is_200(data: &[u8]) -> bool {
    find_bytes(&data[..data.len().min(32)], b"200").is_some()
}

fn content_length(hdr: &[u8]) -> Option<usize> {
    let key = b"content-length:";
    let pos = find_bytes_ci(hdr, key)?;
    let rest = &hdr[pos + key.len()..];
    let end = find_bytes(rest, b"\r").unwrap_or(rest.len());
    core::str::from_utf8(&rest[..end]).ok()?.trim().parse().ok()
}

fn parse_ver_triplet(s: &str) -> (u16, u16, u16) {
    let clean = s.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let mut parts = clean.split('.');
    let mut parse_part = |p: Option<&str>| -> u16 {
        let mut n = 0u16;
        if let Some(sub) = p {
            for b in sub.as_bytes() {
                if b.is_ascii_digit() {
                    n = n.saturating_mul(10).saturating_add((b - b'0') as u16);
                } else {
                    break;
                }
            }
        }
        n
    };
    (parse_part(parts.next()), parse_part(parts.next()), parse_part(parts.next()))
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    let c = parse_ver_triplet(candidate);
    let cur = parse_ver_triplet(current);
    c > cur
}

fn should_trigger_ota(candidate: &str, current: &str) -> bool {
    let clean_cand = candidate.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let clean_cur = current.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    !clean_cand.is_empty() && clean_cand != clean_cur
}

/// Struktur data esp_ota_select_entry_t (32 byte) sesuai standar ESP-IDF bootloader
#[repr(C, align(4))]
#[derive(Clone, Copy)]
struct EspOtaSelectEntry {
    ota_seq: u32,
    seq_label: [u8; 20],
    ota_state: u32, // 2 = Valid, 0 = New
    crc: u32,       // CRC32 dari ota_seq (IEEE 802.3)
}

/// Hitung CRC32 IEEE 802.3 untuk esp_ota_select_entry
fn esp_ota_crc32(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

/// Kirim laporan status / telemetri ke ThingsBoard Cloud
async fn post_thingsboard_json(stack: Stack<'static>, tb_ip: IpAddress, endpoint: &str, body_bytes: &[u8]) -> bool {
    let (rx, tx) = unsafe { (&mut *(&raw mut CHK_RX), &mut *(&raw mut CHK_TX)) };
    let mut sock = TcpSocket::new(stack, rx, tx);
    sock.set_timeout(Some(Duration::from_secs(6)));
    if sock.connect((tb_ip, TB_PORT)).await.is_err() {
        return false;
    }

    let mut hdr: heapless::String<320> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut hdr,
        format_args!(
            "POST /api/v1/{}/{} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            THINGSBOARD_ACCESS_TOKEN,
            endpoint,
            crate::config::THINGSBOARD_HOST,
            body_bytes.len()
        ),
    );

    if sock.write(hdr.as_bytes()).await.is_err() {
        sock.close();
        return false;
    }
    if sock.write(body_bytes).await.is_err() {
        sock.close();
        return false;
    }
    let _ = sock.flush().await;

    let mut resp_buf = [0u8; 64];
    let ok = match sock.read(&mut resp_buf).await {
        Ok(n) if n > 0 => is_200(&resp_buf[..n]),
        _ => false,
    };
    sock.close();
    ok
}

// ── OTA & Cloud Relay Task ────────────────────────────────────────────────────

#[embassy_executor::task]
pub async fn ota_client_task(stack: Stack<'static>) {
    println!(
        ">>> [CLOUD & OTA] ThingsBoard Cloud Relay + Auto OTA AKTIF — firmware v{} (Token: {})",
        CURRENT_VERSION, THINGSBOARD_ACCESS_TOKEN
    );

    // Tunggu WiFi DHCP siap
    loop {
        Timer::after(Duration::from_secs(2)).await;
        if stack.is_config_up() {
            println!(">>> [CLOUD & OTA] Wi-Fi tersambung! Menghubungkan ke ThingsBoard Cloud & Azure Relay...");
            break;
        }
    }

    let mut cached_tb_ip: Option<IpAddress> = None;
    let mut cached_local_server: Option<IpAddress> = None;
    let mut poll: u32 = 0;

    loop {
        OTA_IN_PROGRESS.store(false, Ordering::SeqCst);
        Timer::after(Duration::from_secs(3)).await;
        if !stack.is_config_up() {
            while !stack.is_config_up() {
                Timer::after(Duration::from_secs(2)).await;
            }
            println!(">>> [CLOUD & OTA] 🔄 Wi-Fi pulih kembali! Melanjutkan sinkronisasi Cloud...");
        }
        poll = poll.wrapping_add(1);

        // ── 0. Pastikan koneksi IP ke ThingsBoard Cloud (54.208.96.163 dkk) ───
        let mut tb_ip_opt = cached_tb_ip;
        if tb_ip_opt.is_none() {
            for oct in &crate::config::THINGSBOARD_CLOUD_IPS {
                let cand = IpAddress::v4(oct[0], oct[1], oct[2], oct[3]);
                let (rx, tx) = unsafe { (&mut *(&raw mut CHK_RX), &mut *(&raw mut CHK_TX)) };
                let mut sock = TcpSocket::new(stack, rx, tx);
                sock.set_timeout(Some(Duration::from_secs(4)));
                if sock.connect((cand, TB_PORT)).await.is_ok() {
                    sock.close();
                    println!(">>> [THINGSBOARD CLOUD] ✅ Terhubung ke Cloud Server {}.{}.{}.{}:{}!", oct[0], oct[1], oct[2], oct[3], TB_PORT);
                    cached_tb_ip = Some(cand);
                    tb_ip_opt = Some(cand);
                    break;
                }
            }
        }

        // ── 1. Push Live Telemetry & Heartbeat ke ThingsBoard Cloud (Menjaga Status ACTIVE & Lintas Hotspot) ──
        if let Some(tb_ip) = tb_ip_opt {
            let telem_len = unsafe { CLOUD_TELEM_LEN };
            if telem_len > 0 {
                // Kirim sebagai client attribute `live_telemetry` agar Backend / Vercel lintas Wi-Fi bisa membaca real-time
                let mut attr_body: heapless::Vec<u8, 1700> = heapless::Vec::new();
                let _ = attr_body.extend_from_slice(b"{\"live_telemetry\":");
                unsafe {
                    let t_slice = &(&*(&raw const CLOUD_TELEM_BUF))[..telem_len];
                    let _ = attr_body.extend_from_slice(t_slice);
                }
                let _ = attr_body.extend_from_slice(b",\"fw_version\":\"");
                let _ = attr_body.extend_from_slice(CURRENT_VERSION.as_bytes());
                let _ = attr_body.extend_from_slice(b"\",\"status\":\"ONLINE\"}");

                if !post_thingsboard_json(stack, tb_ip, "attributes", attr_body.as_slice()).await {
                    cached_tb_ip = None;
                }
            }

            // Setiap ~6 detik, kirim juga ke endpoint `/telemetry` ThingsBoard agar grafik & status Device di ThingsBoard selalu ACTIVE
            if poll % 2 == 1 {
                let mut tb_telem: heapless::String<256> = heapless::String::new();
                let _ = core::fmt::Write::write_fmt(
                    &mut tb_telem,
                    format_args!(
                        "{{\"current_fw_title\":\"fimware\",\"current_fw_version\":\"{}\",\"fw_state\":\"UPDATED\",\"status\":\"ONLINE\"}}",
                        CURRENT_VERSION
                    ),
                );
                let _ = post_thingsboard_json(stack, tb_ip, "telemetry", tb_telem.as_bytes()).await;
            }
        }

        // ── 2. Poll Remote Command (`cloud_cmd`) & Cek OTA Firmware setiap siklus ──
        let mut target_ota: Option<(heapless::String<16>, heapless::String<64>, IpAddress, u16, bool)> = None;

        if let Some(tb_ip) = cached_tb_ip {
            let (rx, tx) = unsafe { (&mut *(&raw mut CHK_RX), &mut *(&raw mut CHK_TX)) };
            let mut sock = TcpSocket::new(stack, rx, tx);
            sock.set_timeout(Some(Duration::from_secs(6)));

            if sock.connect((tb_ip, TB_PORT)).await.is_ok() {
                let mut tb_req: heapless::String<256> = heapless::String::new();
                let _ = core::fmt::Write::write_fmt(
                    &mut tb_req,
                    format_args!(
                        "GET /api/v1/{}/attributes?sharedKeys=fw_title,fw_version,fw_size&clientKeys=cloud_cmd,cmd_seq HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                        THINGSBOARD_ACCESS_TOKEN,
                        crate::config::THINGSBOARD_HOST
                    ),
                );

                if sock.write(tb_req.as_bytes()).await.is_ok() {
                    let _ = sock.flush().await;
                    let buf = unsafe { &mut *(&raw mut CHK_RX) };
                    if let Ok(n) = sock.read(buf).await {
                        if is_200(&buf[..n]) {
                            if let Some(body_start) = hdr_end(&buf[..n]).map(|p| p + 4) {
                                let body = &buf[body_start..n];

                                // A. Cek apakah ada perintah remote START/STOP/PURGE dari Dashboard Lintas-WiFi
                                if let Some(cmd_bytes) = json_str(body, b"cloud_cmd") {
                                    let seq = json_u32(body, b"cmd_seq").unwrap_or(1);
                                    let last = LAST_CMD_SEQ.load(Ordering::SeqCst);
                                    if seq != last {
                                        LAST_CMD_SEQ.store(seq, Ordering::SeqCst);
                                        // Abaikan perintah lama dari sesi sebelumnya saat pertama kali booting (last == 0)
                                        if last != 0 {
                                            if let Ok(cmd_str) = core::str::from_utf8(cmd_bytes) {
                                                if cmd_str.eq_ignore_ascii_case("START") {
                                                    println!(">>> [CLOUD CMD] ☁️ Menerima perintah START lintas jaringan!");
                                                    PENDING_CLOUD_CMD.store(1, Ordering::SeqCst);
                                                } else if cmd_str.eq_ignore_ascii_case("STOP") {
                                                    println!(">>> [CLOUD CMD] ☁️ Menerima perintah STOP lintas jaringan!");
                                                    PENDING_CLOUD_CMD.store(2, Ordering::SeqCst);
                                                } else if cmd_str.eq_ignore_ascii_case("PURGE") {
                                                    println!(">>> [CLOUD CMD] ☁️ Menerima perintah PURGE lintas jaringan!");
                                                    PENDING_CLOUD_CMD.store(3, Ordering::SeqCst);
                                                } else if cmd_str.eq_ignore_ascii_case("OTA") || cmd_str.eq_ignore_ascii_case("UPDATE") || cmd_str.eq_ignore_ascii_case("FLASH") || cmd_str.eq_ignore_ascii_case("SYNC_OTA") {
                                                    println!(">>> [CLOUD CMD] ⚡ Menerima perintah SINKRONISASI OTA dari Web Dashboard!");
                                                    OTA_TRIGGER_REQUESTED.store(true, Ordering::SeqCst);
                                                    // LANGSUNG STOP SELURUH SISTEM (SENSOR & POMPA) UNTUK CEK VERSI THINGSBOARD
                                                    OTA_IN_PROGRESS.store(true, Ordering::SeqCst);
                                                    println!(">>> [OTA SYNC] 🛑 Seluruh sistem (sensor & pompa) DIHENTIKAN sementara untuk cek versi ThingsBoard...");
                                                }
                                            }
                                        } else {
                                            println!(">>> [CLOUD CMD] ☁️ Urutan perintah Cloud disinkronkan (seq={}). Perintah lama diabaikan saat boot. Gerbang OTA Terkunci.", seq);
                                        }
                                    }
                                }

                                // B. Cek apakah ada update OTA di ThingsBoard Shared Attributes
                                if let Some(v_bytes) = json_str(body, b"fw_version") {
                                    if let Ok(v_str) = core::str::from_utf8(v_bytes) {
                                        let title_str = json_str(body, b"fw_title")
                                            .and_then(|t| core::str::from_utf8(t).ok())
                                            .unwrap_or("fimware");
                                        let is_unlocked = OTA_TRIGGER_REQUESTED.load(Ordering::SeqCst);
                                        if poll % 5 == 0 {
                                            println!(
                                                ">>> [OTA THINGSBOARD] Cloud=v{} ({}) | Lokal=v{} | Gerbang OTA: {}",
                                                v_str,
                                                title_str,
                                                CURRENT_VERSION,
                                                if is_unlocked {
                                                    "🔓 DIIZINKAN (Dipicu Tombol Sinkronisasi)"
                                                } else {
                                                    "🔒 STANDBY (Menunggu Tombol Sinkronisasi di Dashboard)"
                                                }
                                            );
                                        }

                                        // EKSEKUSI CEK VERSI SETELAH TOMBOL DIPENCET:
                                        if OTA_TRIGGER_REQUESTED.swap(false, Ordering::SeqCst) {
                                            if !v_str.is_empty() && should_trigger_ota(v_str, CURRENT_VERSION) {
                                                let is_up = is_newer_version(v_str, CURRENT_VERSION);
                                                println!(
                                                    ">>> [OTA THINGSBOARD] ⬇️ PERUBAHAN VERSI TERDETEKSI DI THINGSBOARD! {} v{} → v{} (Title: {})",
                                                    if is_up { "UPGRADE" } else { "ROLLBACK" },
                                                    CURRENT_VERSION, v_str, title_str
                                                );
                                                println!(">>> [OTA THINGSBOARD] Memulai download firmware dari ThingsBoard & flashing...");
                                                let mut ver_s: heapless::String<16> = heapless::String::new();
                                                let mut tit_s: heapless::String<64> = heapless::String::new();
                                                let _ = ver_s.push_str(v_str);
                                                let _ = tit_s.push_str(title_str);
                                                target_ota = Some((ver_s, tit_s, tb_ip, TB_PORT, true));
                                            } else {
                                                // Versi di ThingsBoard sama dengan firmware lokal
                                                println!(
                                                    ">>> [OTA THINGSBOARD] ✅ Versi di ThingsBoard (v{}) SAMA dengan firmware ESP32 (v{}). Tidak ada perubahan!",
                                                    v_str, CURRENT_VERSION
                                                );
                                                let mut tb_resp: heapless::String<160> = heapless::String::new();
                                                let _ = core::fmt::Write::write_fmt(
                                                    &mut tb_resp,
                                                    format_args!(
                                                        "{{\"fw_state\":\"UPDATED\",\"status\":\"UP_TO_DATE\",\"current_fw_version\":\"{}\"}}",
                                                        CURRENT_VERSION
                                                    ),
                                                );
                                                let _ = post_thingsboard_json(stack, tb_ip, "telemetry", tb_resp.as_bytes()).await;
                                                // Reset client attribute cloud_cmd ke IDLE di ThingsBoard agar tidak memicu loop
                                                let _ = post_thingsboard_json(stack, tb_ip, "attributes", b"{\"cloud_cmd\":\"IDLE\",\"ota_trigger\":false}").await;
                                                // Kembalikan sistem ke mode normal (lanjutkan pembacaan sensor & pompa)
                                                OTA_IN_PROGRESS.store(false, Ordering::SeqCst);
                                                println!(">>> [OTA THINGSBOARD] ▶️ Tidak ada update baru. Seluruh sistem (sensor & pompa) dilanjutkan kembali secara normal.");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                sock.close();
            } else {
                cached_tb_ip = None;
            }
        }

        // ── 3. Fallback Cek Local Backend Server (:3000) HANYA jika dipicu tombol sinkronisasi & Cloud tidak ada ──
        if target_ota.is_none() && OTA_TRIGGER_REQUESTED.load(Ordering::SeqCst) && poll % 5 == 0 {
            let my_octets = if let Some(cfg) = stack.config_v4() {
                cfg.address.address().octets()
            } else {
                [10, 96, 101, 1]
            };

            let mut srv_opt = cached_local_server;
            if srv_opt.is_none() {
                let candidates: [u8; 6] = [106, 100, 101, 102, 103, 1];
                for &c in &candidates {
                    let cand_ip = IpAddress::v4(my_octets[0], my_octets[1], my_octets[2], c);
                    let (rx, tx) = unsafe { (&mut *(&raw mut CHK_RX), &mut *(&raw mut CHK_TX)) };
                    let mut sock = TcpSocket::new(stack, rx, tx);
                    sock.set_timeout(Some(Duration::from_millis(500)));
                    if sock.connect((cand_ip, SERVER_PORT)).await.is_ok() {
                        sock.close();
                        cached_local_server = Some(cand_ip);
                        srv_opt = Some(cand_ip);
                        break;
                    }
                }
            }

            if let Some(local_srv) = srv_opt {
                let (rx2, tx2) = unsafe { (&mut *(&raw mut CHK_RX), &mut *(&raw mut CHK_TX)) };
                let mut sock2 = TcpSocket::new(stack, rx2, tx2);
                sock2.set_timeout(Some(Duration::from_secs(5)));
                if sock2.connect((local_srv, SERVER_PORT)).await.is_ok() {
                    let req = b"GET /api/ota/check HTTP/1.1\r\nHost: enose.local\r\nConnection: close\r\n\r\n";
                    if sock2.write(req).await.is_ok() {
                        let buf = unsafe { &mut *(&raw mut CHK_RX) };
                        if let Ok(n) = sock2.read(buf).await {
                            if n > 0 && is_200(&buf[..n]) {
                                if let Some(body_start) = hdr_end(&buf[..n]).map(|p| p + 4) {
                                    let body = &buf[body_start..n];
                                    if let (Some(ver_b), Some(file_b)) = (json_str(body, b"version"), json_str(body, b"binary_filename")) {
                                        let ver = core::str::from_utf8(ver_b).unwrap_or("");
                                        let file = core::str::from_utf8(file_b).unwrap_or("");
                                        if !ver.is_empty() && should_trigger_ota(ver, CURRENT_VERSION) {
                                            if OTA_TRIGGER_REQUESTED.swap(false, Ordering::SeqCst) {
                                                let is_up = is_newer_version(ver, CURRENT_VERSION);
                                                println!(
                                                    ">>> [OTA LOCAL] ⬇️ {} DIAKTIFKAN OLEH TOMBOL DASHBOARD! v{} → v{}",
                                                    if is_up { "UPGRADE" } else { "ROLLBACK / KEMBALI" },
                                                    CURRENT_VERSION, ver
                                                );
                                                let mut v: heapless::String<16> = heapless::String::new();
                                                let mut f: heapless::String<64> = heapless::String::new();
                                                let _ = v.push_str(ver);
                                                let _ = f.push_str(file);
                                                target_ota = Some((v, f, local_srv, SERVER_PORT, false));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    sock2.close();
                } else {
                    cached_local_server = None;
                }
            }
        }

        // Jika belum ada update firmware, lanjut ke siklus berikutnya
        let (version_owned, file_owned, dl_server, dl_port, is_thingsboard) = match target_ota {
            Some(t) => t,
            None => continue,
        };

        // ── 4. Aktifkan OTA Focus Mode (Hentikan Pembacaan Sensor & Pompa) ──
        OTA_IN_PROGRESS.store(true, Ordering::SeqCst);
        println!(
            ">>> [OTA FOCUS MODE] 🛑 Menghentikan pembacaan sensor & siklus pompa — Fokus 100% pada OTA v{}...",
            version_owned.as_str()
        );

        if is_thingsboard {
            let _ = embassy_time::with_timeout(
                Duration::from_secs(3),
                post_thingsboard_json(stack, dl_server, "telemetry", b"{\"fw_state\":\"DOWNLOADING\"}"),
            )
            .await;
            let _ = embassy_time::with_timeout(
                Duration::from_secs(3),
                post_thingsboard_json(stack, dl_server, "attributes", b"{\"cloud_cmd\":\"IDLE\",\"ota_trigger\":false}"),
            )
            .await;
        }

        let mut dl_path: heapless::Vec<u8, 160> = heapless::Vec::new();
        if is_thingsboard {
            let _ = dl_path.extend_from_slice(b"/api/v1/");
            let _ = dl_path.extend_from_slice(THINGSBOARD_ACCESS_TOKEN.as_bytes());
            let _ = dl_path.extend_from_slice(b"/firmware?title=");
            let _ = dl_path.extend_from_slice(file_owned.as_bytes());
            let _ = dl_path.extend_from_slice(b"&version=");
            let _ = dl_path.extend_from_slice(version_owned.as_bytes());
        } else {
            let _ = dl_path.extend_from_slice(b"/api/ota/download/");
            let _ = dl_path.extend_from_slice(file_owned.as_bytes());
        }

        let (dl_rx, dl_tx) = unsafe { (&mut *(&raw mut DL_RX), &mut *(&raw mut DL_TX)) };
        let mut dl = TcpSocket::new(stack, dl_rx, dl_tx);
        dl.set_timeout(Some(Duration::from_secs(60)));

        if embassy_time::with_timeout(Duration::from_secs(10), dl.connect((dl_server, dl_port))).await.is_err() {
            println!(">>> [OTA] ⚠️ Gagal koneksi download ke {}:{}, retry...", dl_server, dl_port);
            continue;
        }

        let host_hdr = if is_thingsboard {
            crate::config::THINGSBOARD_HOST
        } else {
            "enose.local"
        };

        let mut req: heapless::String<320> = heapless::String::new();
        let path_str = core::str::from_utf8(dl_path.as_slice()).unwrap_or("/");
        let _ = core::fmt::Write::write_fmt(
            &mut req,
            format_args!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path_str, host_hdr
            ),
        );

        if embassy_time::with_timeout(Duration::from_secs(10), dl.write(req.as_bytes())).await.is_err() {
            dl.close();
            continue;
        }
        let _ = dl.flush().await;

        let hdr_buf = unsafe { &mut *(&raw mut HDR) };
        let mut hn: usize = 0;
        // Baca sampai seluruh HTTP header (\r\n\r\n) diterima
        loop {
            if hn >= hdr_buf.len() { break; }
            match embassy_time::with_timeout(Duration::from_secs(15), dl.read(&mut hdr_buf[hn..])).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => {
                    hn += n;
                    if hdr_end(&hdr_buf[..hn]).is_some() {
                        break;
                    }
                }
                _ => break,
            }
        }

        if hn == 0 || !is_200(&hdr_buf[..hn]) {
            println!(">>> [OTA] Download server tidak mengembalikan HTTP 200.");
            dl.close();
            continue;
        }
        let he = match hdr_end(&hdr_buf[..hn]) {
            Some(p) => p,
            None => {
                dl.close();
                continue;
            }
        };
        let total = content_length(&hdr_buf[..he]).unwrap_or(600 * 1024);
        println!(">>> [OTA] Mengunduh firmware v{} (Ukuran: {} bytes)...", version_owned.as_str(), total);

        let body0 = &hdr_buf[he + 4..hn];

        // ── 5. Buka OTA partition & tulis dengan Per-Sector Write ──
        let booted_paddr = unsafe { ((0x600C5000 as *const u32).read_volatile() & 0xff) << 16 };
        println!(
            ">>> [OTA] Membuka OTA partition (Booted paddr saat ini: 0x{:06X})...",
            booted_paddr
        );

        // Bersihkan slot otadata (0xd000 / 0xe000) yang berstatus Aborted/Invalid/Corrupt
        // agar OtaUpdater tidak pernah terjebak pada slot gagal sebelumnya
        unsafe {
            let _ = esp_storage::ll::spiflash_unlock();
            for &slot_addr in &[0xd000u32, 0xe000u32] {
                let mut raw = [0u32; 8];
                let _ = esp_storage::ll::spiflash_read(slot_addr, raw.as_mut_ptr(), 32);
                let seq = raw[0];
                let state = raw[6];
                let crc = raw[7];
                let is_erased = seq == u32::MAX && state == u32::MAX;
                let is_valid_active = seq != u32::MAX
                    && seq != 0
                    && (state <= 2 || state == u32::MAX)
                    && crc == esp_ota_crc32(&seq.to_le_bytes());
                if !is_erased && !is_valid_active {
                    println!(
                        ">>> [OTA] Membersihkan otadata slot 0x{:04X} yang Aborted/Invalid (seq={}, state={})...",
                        slot_addr, seq, state
                    );
                    let _ = esp_storage::ll::spiflash_erase_sector(slot_addr / SECTOR_SIZE as u32);
                }
            }
        }

        let flash_peri = unsafe { esp_hal::peripherals::FLASH::steal() };
        let mut flash  = unsafe { esp_storage::FlashStorage::new(flash_peri).multicore_ignore() };

        let mut pt_buf = [0u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN];
        let ota_updater_res = esp_bootloader_esp_idf::ota_updater::OtaUpdater::new(&mut flash, &mut pt_buf);

        if let Ok(mut ota) = ota_updater_res {
            println!(">>> [OTA] Menggunakan OtaUpdater standar (partition table valid).");
            let (mut next_part, next_type) = match ota.next_partition() {
                Ok(p) => p,
                Err(e) => {
                    println!(">>> [OTA] Gagal next partition: {:?}", e);
                    dl.close();
                    continue;
                }
            };

            println!(">>> [OTA] Slot target: {:?} ({} bytes). Mulai streaming...", next_type, next_part.partition_size());

            let sec = unsafe { &mut *(&raw mut SECTOR) };
            let mut sec_fill: usize = 0;
            let mut wr_off: u32 = 0;
            let mut total_written: usize = 0;

            for &byte in body0 {
                sec[sec_fill] = byte;
                sec_fill += 1;
                if sec_fill == SECTOR_SIZE {
                    let _ = next_part.write(wr_off, sec);
                    wr_off += SECTOR_SIZE as u32;
                    total_written += SECTOR_SIZE;
                    sec_fill = 0;
                }
            }

            let chk = unsafe { &mut *(&raw mut CHUNK) };
            let mut stream_completed = false;
            loop {
                if total_written + sec_fill >= total {
                    println!(">>> [OTA] 100% Stream selesai ({} bytes).", total_written + sec_fill);
                    stream_completed = true;
                    break;
                }
                match embassy_time::with_timeout(Duration::from_secs(20), dl.read(chk)).await {
                    Ok(Ok(0)) => {
                        println!(">>> [OTA] 100% Stream selesai ({} bytes).", total_written + sec_fill);
                        stream_completed = true;
                        break;
                    }
                    Ok(Ok(n)) => {
                        let mut src = 0;
                        while src < n {
                            let room = SECTOR_SIZE - sec_fill;
                            let copy = (n - src).min(room);
                            sec[sec_fill..sec_fill + copy].copy_from_slice(&chk[src..src + copy]);
                            sec_fill += copy;
                            src += copy;
                            if sec_fill == SECTOR_SIZE {
                                let _ = next_part.write(wr_off, sec);
                                wr_off += SECTOR_SIZE as u32;
                                total_written += SECTOR_SIZE;
                                sec_fill = 0;
                                if wr_off % (32 * 1024) == 0 {
                                    println!(
                                        ">>> [OTA] {}% ({} bytes)",
                                        (total_written * 100) / total.max(1),
                                        total_written
                                    );
                                }
                            }
                            if total_written + sec_fill >= total {
                                break;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        println!(">>> [OTA BUFFER] ⚠️ Stream terputus: {:?} ({} bytes).", e, total_written + sec_fill);
                        break;
                    }
                    Err(_) => {
                        println!(">>> [OTA BUFFER] ⚠️ Timeout 20s saat mengunduh ({} bytes). Mengulang...", total_written + sec_fill);
                        break;
                    }
                }
            }
            dl.close();

            if sec_fill > 0 {
                for i in sec_fill..SECTOR_SIZE { sec[i] = 0xFF; }
                let _ = next_part.write(wr_off, sec);
                total_written += sec_fill;
            }

            if !stream_completed || total_written < 100_000 {
                println!(">>> [OTA BUFFER] ⚠️ Unduhan belum tuntas (bytes={}). Menjaga firmware lama tetap aktif.", total_written);
                continue;
            }

            drop(next_part);
            let _ = ota.activate_next_partition();
            let _ = ota.set_current_ota_state(esp_bootloader_esp_idf::ota::OtaImageState::Valid);
            drop(ota);
            drop(flash);

            // Pastikan otadata (0xd000 & 0xe000) selalu tertulis bersih & valid (ota_state = 2 / Valid)
            // bahkan jika slot sebelumnya sempat terputus di tengah jalan (current == next_type)
            let (otadata_addr, new_seq, slot_label) = match next_type {
                esp_bootloader_esp_idf::partitions::AppPartitionSubType::Ota1 => (0xe000u32, 2u32, b"ota_1" as &[u8]),
                _ => (0xd000u32, 1u32, b"ota_0" as &[u8]),
            };
            let _ = unsafe { esp_storage::ll::spiflash_unlock() };
            let _ = unsafe { esp_storage::ll::spiflash_erase_sector(0xd000 / SECTOR_SIZE as u32) };
            let _ = unsafe { esp_storage::ll::spiflash_erase_sector(0xe000 / SECTOR_SIZE as u32) };
            let mut entry = EspOtaSelectEntry {
                ota_seq: new_seq,
                seq_label: [0u8; 20],
                ota_state: 2, // ESP_OTA_IMG_VALID
                crc: esp_ota_crc32(&new_seq.to_le_bytes()),
            };
            for (i, &b) in slot_label.iter().enumerate().take(20) {
                entry.seq_label[i] = b;
            }
            let _ = unsafe {
                esp_storage::ll::spiflash_write(otadata_addr, &entry as *const _ as *const u32, 32)
            };

            println!(">>> [OTA] ✅ Boot partition diset ke slot baru ({:?} / seq={})!", next_type, new_seq);
            println!(">>> [OTA] 🔄 Restart dalam 1 detik... firmware v{} aktif!", version_owned.as_str());

            if is_thingsboard {
                let _ = embassy_time::with_timeout(
                    Duration::from_secs(3),
                    post_thingsboard_json(stack, dl_server, "telemetry", b"{\"fw_state\":\"UPDATED\"}"),
                )
                .await;
            }

            Timer::after(Duration::from_millis(800)).await;
            esp_hal::system::software_reset();
        } else {
            // Direct Raw Partition Flasher (Fallback)
            println!(">>> [OTA] Mengaktifkan Direct Low-Level Flash Engine...");

            let mut raw_slot0 = [0u32; 8];
            let mut raw_slot1 = [0u32; 8];
            let _ = unsafe { esp_storage::ll::spiflash_read(0xd000, raw_slot0.as_mut_ptr(), 32) };
            let _ = unsafe { esp_storage::ll::spiflash_read(0xe000, raw_slot1.as_mut_ptr(), 32) };

            let parse_entry = |raw: &[u32; 8]| -> Option<u32> {
                let seq = raw[0];
                let crc = raw[7];
                if seq == u32::MAX { return None; }
                if crc == esp_ota_crc32(&seq.to_le_bytes()) { Some(seq) } else { None }
            };

            let seq0 = parse_entry(&raw_slot0);
            let seq1 = parse_entry(&raw_slot1);

            let (target_slot_name, target_addr, otadata_addr, new_seq) = match (seq0, seq1) {
                (Some(s0), Some(s1)) => {
                    if s0 >= s1 {
                        if (s0 - 1) % 2 == 0 { ("ota_1", 0x210000u32, 0xe000u32, s0 + 1) }
                        else { ("ota_0", 0x110000u32, 0xd000u32, s0 + 1) }
                    } else {
                        if (s1 - 1) % 2 == 0 { ("ota_1", 0x210000u32, 0xe000u32, s1 + 1) }
                        else { ("ota_0", 0x110000u32, 0xd000u32, s1 + 1) }
                    }
                }
                (Some(s0), None) => {
                    if (s0 - 1) % 2 == 0 { ("ota_1", 0x210000u32, 0xe000u32, s0 + 1) }
                    else { ("ota_0", 0x110000u32, 0xd000u32, s0 + 1) }
                }
                (None, Some(s1)) => {
                    if (s1 - 1) % 2 == 0 { ("ota_1", 0x210000u32, 0xe000u32, s1 + 1) }
                    else { ("ota_0", 0x110000u32, 0xd000u32, s1 + 1) }
                }
                (None, None) => ("ota_0", 0x110000u32, 0xd000u32, 1u32),
            };

            let _ = unsafe { esp_storage::ll::spiflash_unlock() };

            let sec = unsafe { &mut *(&raw mut SECTOR) };
            let mut sec_fill: usize = 0;
            let mut wr_off: u32 = 0;
            let mut total_written: usize = 0;

            for &byte in body0 {
                sec[sec_fill] = byte;
                sec_fill += 1;
                if sec_fill == SECTOR_SIZE {
                    let sec_addr = target_addr + wr_off;
                    let _ = unsafe { esp_storage::ll::spiflash_erase_sector(sec_addr / SECTOR_SIZE as u32) };
                    let _ = unsafe {
                        esp_storage::ll::spiflash_write(sec_addr, sec.as_ptr() as *const u32, SECTOR_SIZE as u32)
                    };
                    wr_off += SECTOR_SIZE as u32;
                    total_written += SECTOR_SIZE;
                    sec_fill = 0;
                }
            }

            let chk = unsafe { &mut *(&raw mut CHUNK) };
            let mut stream_completed = false;
            loop {
                if total_written + sec_fill >= total {
                    println!(">>> [OTA] 100% Stream selesai ({} bytes).", total_written + sec_fill);
                    stream_completed = true;
                    break;
                }
                match embassy_time::with_timeout(Duration::from_secs(20), dl.read(chk)).await {
                    Ok(Ok(0)) => {
                        println!(">>> [OTA] 100% Stream selesai ({} bytes).", total_written + sec_fill);
                        stream_completed = true;
                        break;
                    }
                    Ok(Ok(n)) => {
                        let mut src = 0;
                        while src < n {
                            let room = SECTOR_SIZE - sec_fill;
                            let copy = (n - src).min(room);
                            sec[sec_fill..sec_fill + copy].copy_from_slice(&chk[src..src + copy]);
                            sec_fill += copy;
                            src += copy;
                            if sec_fill == SECTOR_SIZE {
                                let sec_addr = target_addr + wr_off;
                                let _ = unsafe { esp_storage::ll::spiflash_erase_sector(sec_addr / SECTOR_SIZE as u32) };
                                let _ = unsafe {
                                    esp_storage::ll::spiflash_write(sec_addr, sec.as_ptr() as *const u32, SECTOR_SIZE as u32)
                                };
                                wr_off += SECTOR_SIZE as u32;
                                total_written += SECTOR_SIZE;
                                sec_fill = 0;
                                if wr_off % (32 * 1024) == 0 {
                                    println!(
                                        ">>> [OTA] {}% ({} bytes)",
                                        (total_written * 100) / total.max(1),
                                        total_written
                                    );
                                }
                            }
                            if total_written + sec_fill >= total {
                                break;
                            }
                        }
                    }
                    _ => break,
                }
            }
            dl.close();

            if sec_fill > 0 {
                for i in sec_fill..SECTOR_SIZE { sec[i] = 0xFF; }
                let sec_addr = target_addr + wr_off;
                let _ = unsafe { esp_storage::ll::spiflash_erase_sector(sec_addr / SECTOR_SIZE as u32) };
                let _ = unsafe {
                    esp_storage::ll::spiflash_write(sec_addr, sec.as_ptr() as *const u32, SECTOR_SIZE as u32)
                };
                total_written += sec_fill;
            }

            if !stream_completed || total_written < 100_000 {
                continue;
            }

            let _ = unsafe { esp_storage::ll::spiflash_erase_sector(otadata_addr / SECTOR_SIZE as u32) };

            let mut entry = EspOtaSelectEntry {
                ota_seq: new_seq,
                seq_label: [0u8; 20],
                ota_state: 2,
                crc: esp_ota_crc32(&new_seq.to_le_bytes()),
            };
            let label_bytes = target_slot_name.as_bytes();
            for (i, &b) in label_bytes.iter().enumerate().take(20) {
                entry.seq_label[i] = b;
            }

            let _ = unsafe {
                esp_storage::ll::spiflash_write(otadata_addr, &entry as *const _ as *const u32, 32)
            };

            println!(">>> [OTA] ✅ Boot partition diset ke slot baru ({})!", target_slot_name);
            println!(">>> [OTA] 🔄 Restart dalam 1 detik... firmware v{} aktif!", version_owned.as_str());

            if is_thingsboard {
                let _ = embassy_time::with_timeout(
                    Duration::from_secs(3),
                    post_thingsboard_json(stack, dl_server, "telemetry", b"{\"fw_state\":\"UPDATED\"}"),
                )
                .await;
            }

            Timer::after(Duration::from_millis(800)).await;
            esp_hal::system::software_reset();
        }
    }
}
