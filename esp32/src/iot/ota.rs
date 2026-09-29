//! ThingsBoard-style Automatic Over-The-Air (OTA) Client
//!
//! Alur kerja:
//! 1. Tunggu WiFi DHCP siap
//! 2. Poll /api/ota/check setiap 15 detik
//! 3. Parse JSON → bandingkan versi
//! 4. Jika ada versi baru → download binary via HTTP GET
//! 5. Erase + tulis ke OTA partition (ota_0 / ota_1)
//! 6. Aktifkan slot baru → software_reset()

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_println::println;

/// Versi firmware aktif
#[cfg(feature = "v101")]
const CURRENT_VERSION: &str = "1.0.1";
#[cfg(feature = "v102")]
const CURRENT_VERSION: &str = "1.0.2";
#[cfg(feature = "v103")]
const CURRENT_VERSION: &str = "1.0.3";
#[cfg(feature = "v104")]
const CURRENT_VERSION: &str = "1.0.4";
#[cfg(feature = "v105")]
const CURRENT_VERSION: &str = "1.0.5";
#[cfg(not(any(feature = "v101", feature = "v102", feature = "v103", feature = "v104", feature = "v105")))]
const CURRENT_VERSION: &str = "1.0.5";

const SERVER_PORT: u16 = 3000;
const SECTOR_SIZE: usize = 4096;

// ── Static buffers di BSS (bukan stack) ───────────────────────────────────────

static mut CHK_RX: [u8; 1536] = [0u8; 1536];
static mut CHK_TX: [u8; 256]  = [0u8; 256];
static mut DL_RX:  [u8; 4096] = [0u8; 4096];
static mut DL_TX:  [u8; 512]  = [0u8; 512];
static mut HDR:    [u8; 2048] = [0u8; 2048];
static mut SECTOR: [u8; 4096] = [0u8; 4096];
static mut CHUNK:  [u8; 2048] = [0u8; 2048];

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

fn hdr_end(data: &[u8]) -> Option<usize> {
    find_bytes(data, b"\r\n\r\n")
}

fn is_200(data: &[u8]) -> bool {
    find_bytes(&data[..data.len().min(24)], b"200").is_some()
}

fn content_length(hdr: &[u8]) -> Option<usize> {
    let key = b"Content-Length: ";
    let pos = find_bytes(hdr, key)?;
    let rest = &hdr[pos + key.len()..];
    let end = find_bytes(rest, b"\r").unwrap_or(rest.len());
    core::str::from_utf8(&rest[..end]).ok()?.trim().parse().ok()
}

// ── OTA Task ──────────────────────────────────────────────────────────────────

#[embassy_executor::task]
pub async fn ota_client_task(stack: Stack<'static>) {
    println!(">>> [OTA] Auto OTA Client AKTIF — firmware v{}", CURRENT_VERSION);

    // Tunggu WiFi DHCP
    loop {
        Timer::after(Duration::from_secs(5)).await;
        if stack.is_config_up() {
            println!(">>> [OTA] WiFi siap. Memulai deteksi server OTA...");
            break;
        }
    }

    let mut cached_server: Option<embassy_net::IpAddress> = None;
    let mut poll: u32 = 0;

    loop {
        Timer::after(Duration::from_secs(15)).await;
        if !stack.is_config_up() {
            println!(">>> [OTA BUFFER] ⚠️ Wi-Fi terputus! Menyimpan state & menunggu koneksi pulih...");
            while !stack.is_config_up() {
                Timer::after(Duration::from_secs(2)).await;
            }
            println!(">>> [OTA BUFFER] 🔄 Wi-Fi pulih kembali! Melanjutkan polling OTA...");
        }
        poll += 1;

        // Dapatkan subnet lokal dinamis dari DHCP
        let my_octets = if let Some(cfg) = stack.config_v4() {
            cfg.address.address().octets()
        } else {
            [10, 96, 101, 1]
        };

        // Cari server IP (prioritas: cached server, lalu host .106 laptop, lalu kandidat lainnya)
        let mut server_opt: Option<embassy_net::IpAddress> = cached_server;
        if server_opt.is_none() {
            let candidates: [u8; 12] = [106, 100, 101, 102, 103, 104, 105, 107, 108, 109, 110, 1];
            for &c in &candidates {
                let cand_ip = embassy_net::IpAddress::v4(my_octets[0], my_octets[1], my_octets[2], c);
                let (rx, tx) = unsafe { (&mut CHK_RX, &mut CHK_TX) };
                let mut sock = TcpSocket::new(stack, rx, tx);
                sock.set_timeout(Some(Duration::from_millis(800)));
                if sock.connect((cand_ip, SERVER_PORT)).await.is_ok() {
                    sock.close();
                    println!(">>> [OTA] ✅ Server backend ditemukan: {}.{}.{}.{}:{}", my_octets[0], my_octets[1], my_octets[2], c, SERVER_PORT);
                    cached_server = Some(cand_ip);
                    server_opt = Some(cand_ip);
                    break;
                }
            }
        }

        let server = match server_opt {
            Some(ip) => ip,
            None => {
                println!(">>> [OTA] #{}: server tidak terjangkau pada subnet {}.{}.{}.x.", poll, my_octets[0], my_octets[1], my_octets[2]);
                continue;
            }
        };

        // ── 1. GET /api/ota/check ─────────────────────────────────────────
        let (version_owned, file_owned) = {
            let (rx, tx) = unsafe { (&mut CHK_RX, &mut CHK_TX) };
            let mut sock = TcpSocket::new(stack, rx, tx);
            sock.set_timeout(Some(Duration::from_secs(10)));
            if sock.connect((server, SERVER_PORT)).await.is_err() {
                println!(">>> [OTA] #{}: server tidak terjangkau (koneksi putus).", poll);
                cached_server = None; // Reset cached server agar scan ulang
                continue;
            }
            let req = b"GET /api/ota/check HTTP/1.1\r\nHost: enose.local\r\nConnection: close\r\n\r\n";
            if sock.write(req).await.is_err() { sock.close(); continue; }

            let buf = unsafe { &mut CHK_RX };
            let n = match sock.read(buf).await { Ok(n) if n > 0 => n, _ => { sock.close(); continue; } };
            sock.close();

            if !is_200(&buf[..n]) { continue; }
            let body_start = match hdr_end(&buf[..n]) { Some(p) => p + 4, None => continue };
            let body = &buf[body_start..n];

            let ver_bytes  = match json_str(body, b"version")         { Some(v) => v, None => { println!(">>> [OTA] Gagal parse version."); continue; } };
            let file_bytes = match json_str(body, b"binary_filename") { Some(f) => f, None => { println!(">>> [OTA] Gagal parse filename."); continue; } };

            let ver  = core::str::from_utf8(ver_bytes).unwrap_or("?");
            let file = core::str::from_utf8(file_bytes).unwrap_or("?");
            println!(">>> [OTA] #{}: server=v{} lokal=v{}", poll, ver, CURRENT_VERSION);

            if ver == CURRENT_VERSION {
                println!(">>> [OTA] ✓ Up-to-date (v{}).", CURRENT_VERSION);
                continue;
            }

            println!(">>> [OTA] ⬇️  UPDATE TERSEDIA! v{} → v{} ({})", CURRENT_VERSION, ver, file);

            let mut v: heapless::String<16>  = heapless::String::new();
            let mut f: heapless::String<64>  = heapless::String::new();
            let _ = v.push_str(ver);
            let _ = f.push_str(file);
            (v, f)
        };

        if version_owned.as_str() == CURRENT_VERSION { continue; }

        // ── 2. Download binary dengan retry buffer & proteksi error Wi-Fi ───
        let mut dl_path: heapless::Vec<u8, 128> = heapless::Vec::new();
        let _ = dl_path.extend_from_slice(b"/api/ota/download/");
        let _ = dl_path.extend_from_slice(file_owned.as_bytes());

        // Tunggu jika Wi-Fi sempat drop sebelum download
        if !stack.is_config_up() {
            println!(">>> [OTA BUFFER] ⚠️ Wi-Fi terputus. Menyimpan state & menunggu koneksi pulih...");
            while !stack.is_config_up() {
                Timer::after(Duration::from_secs(2)).await;
            }
            println!(">>> [OTA BUFFER] 🔄 Wi-Fi pulih kembali! Melanjutkan proses unduhan firmware...");
        }

        let (dl_rx, dl_tx) = unsafe { (&mut DL_RX, &mut DL_TX) };
        let mut dl = TcpSocket::new(stack, dl_rx, dl_tx);
        dl.set_timeout(Some(Duration::from_secs(60)));

        if dl.connect((server, SERVER_PORT)).await.is_err() {
            println!(">>> [OTA] ⚠️ Gagal koneksi download ke {}:{}, retry pada siklus berikutnya...", server, SERVER_PORT);
            cached_server = None;
            continue;
        }

        let mut req: heapless::Vec<u8, 256> = heapless::Vec::new();
        let _ = req.extend_from_slice(b"GET ");
        let _ = req.extend_from_slice(dl_path.as_slice());
        let _ = req.extend_from_slice(b" HTTP/1.1\r\nHost: enose.local\r\nConnection: close\r\n\r\n");
        if dl.write(req.as_slice()).await.is_err() { dl.close(); continue; }

        let hdr_buf = unsafe { &mut HDR };
        let hn = match dl.read(hdr_buf).await { Ok(n) if n > 0 => n, _ => { dl.close(); continue; } };

        if !is_200(&hdr_buf[..hn]) { println!(">>> [OTA] Download server tidak OK."); dl.close(); continue; }
        let he = match hdr_end(&hdr_buf[..hn]) { Some(p) => p, None => { dl.close(); continue; } };
        let total = content_length(&hdr_buf[..he]).unwrap_or(600 * 1024);
        println!(">>> [OTA] Download size: {} bytes", total);

        let body0 = &hdr_buf[he + 4..hn];

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

        // ── 4. Buka OTA partition, erase, tulis (Dual-Layer Engine) ───────
        println!(">>> [OTA] Membuka OTA partition...");

        // Buat FlashStorage dengan multicore_ignore agar tidak diblokir oleh task Wi-Fi/RTOS
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

            println!(">>> [OTA] Slot target: {:?} ({} bytes)", next_type, next_part.partition_size());

            let part_size = next_part.partition_size() as u32;
            if let Err(e) = next_part.erase(0, part_size) {
                println!(">>> [OTA] GAGAL erase: {:?}", e);
                dl.close();
                continue;
            }
            println!(">>> [OTA] Erase selesai. Mulai streaming...");

            let sec = unsafe { &mut SECTOR };
            let mut sec_fill: usize = 0;
            let mut wr_off: u32 = 0;
            let mut total_written: usize = 0;

            for &byte in body0 {
                sec[sec_fill] = byte;
                sec_fill += 1;
                if sec_fill == SECTOR_SIZE {
                    if let Err(e) = next_part.write(wr_off, sec) {
                        println!(">>> [OTA] Write err sektor {}: {:?}", wr_off / SECTOR_SIZE as u32, e);
                    }
                    wr_off += SECTOR_SIZE as u32;
                    total_written += SECTOR_SIZE;
                    sec_fill = 0;
                }
            }

            let chk = unsafe { &mut CHUNK };
            let mut stream_completed = false;
            loop {
                match dl.read(chk).await {
                    Ok(0) => {
                        println!(">>> [OTA] Stream selesai ({} bytes).", total_written);
                        stream_completed = true;
                        break;
                    }
                    Ok(n) => {
                        let mut src = 0;
                        while src < n {
                            let room = SECTOR_SIZE - sec_fill;
                            let copy = (n - src).min(room);
                            sec[sec_fill..sec_fill + copy].copy_from_slice(&chk[src..src + copy]);
                            sec_fill += copy;
                            src += copy;
                            total_written += copy;
                            if sec_fill == SECTOR_SIZE {
                                if let Err(e) = next_part.write(wr_off, sec) {
                                    println!(">>> [OTA] Write err: {:?}", e);
                                }
                                wr_off += SECTOR_SIZE as u32;
                                sec_fill = 0;
                                if wr_off % (64 * 1024) == 0 {
                                    println!(">>> [OTA] {}% ({} bytes)",
                                        (total_written * 100) / total, total_written);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!(">>> [OTA BUFFER] ⚠️ Stream terputus / error Wi-Fi: {:?} ({} bytes tersimpan).", e, total_written);
                        break;
                    }
                }
            }
            dl.close();

            if sec_fill > 0 {
                for i in sec_fill..SECTOR_SIZE { sec[i] = 0xFF; }
                if let Err(e) = next_part.write(wr_off, sec) {
                    println!(">>> [OTA] Flush terakhir err: {:?}", e);
                }
                total_written += sec_fill;
            }

            println!(">>> [OTA] Flash stream selesai. {} bytes ditulis ke {:?}", total_written, next_type);

            if !stream_completed || total_written < 100_000 {
                println!(">>> [OTA BUFFER] ⚠️ Unduhan belum tuntas (bytes={}). Menjaga firmware lama tetap aktif.", total_written);
                continue;
            }

            drop(next_part);

            if let Err(e) = ota.activate_next_partition() {
                println!(">>> [OTA] GAGAL activate partition: {:?}", e);
                continue;
            }
            let _ = ota.set_current_ota_state(esp_bootloader_esp_idf::ota::OtaImageState::New);

            println!(">>> [OTA] ✅ Boot partition diset ke slot baru ({:?})!", next_type);
            println!(">>> [OTA] 🔄 Restart dalam 1 detik... firmware v{} aktif!", version_owned.as_str());
            Timer::after(Duration::from_millis(1000)).await;
            esp_hal::system::software_reset();
        } else {
            // Direct Raw Partition Flasher (Fallback saat OtaUpdater StorageError/Invalid)
            println!(">>> [OTA] OtaUpdater error: {:?}. Mengaktifkan Direct Low-Level Flash Engine...", ota_updater_res.err());

            // Deteksi slot otadata saat ini dari flash SPI
            let mut raw_slot0 = [0u32; 8];
            let mut raw_slot1 = [0u32; 8];
            let _ = unsafe { esp_storage::ll::spiflash_read(0xd000, raw_slot0.as_mut_ptr(), 32) };
            let _ = unsafe { esp_storage::ll::spiflash_read(0xe000, raw_slot1.as_mut_ptr(), 32) };

            let parse_entry = |raw: &[u32; 8]| -> Option<u32> {
                let seq = raw[0];
                let crc = raw[7];
                if seq == u32::MAX { return None; }
                if crc == esp_ota_crc32(&seq.to_le_bytes()) {
                    Some(seq)
                } else {
                    None
                }
            };

            let seq0 = parse_entry(&raw_slot0);
            let seq1 = parse_entry(&raw_slot1);

            let (target_slot_name, target_addr, otadata_addr, new_seq) = match (seq0, seq1) {
                (Some(s0), Some(s1)) => {
                    if s0 >= s1 {
                        if (s0 - 1) % 2 == 0 {
                            ("ota_1", 0x210000u32, 0xe000u32, s0 + 1)
                        } else {
                            ("ota_0", 0x110000u32, 0xd000u32, s0 + 1)
                        }
                    } else {
                        if (s1 - 1) % 2 == 0 {
                            ("ota_1", 0x210000u32, 0xe000u32, s1 + 1)
                        } else {
                            ("ota_0", 0x110000u32, 0xd000u32, s1 + 1)
                        }
                    }
                }
                (Some(s0), None) => {
                    if (s0 - 1) % 2 == 0 {
                        ("ota_1", 0x210000u32, 0xe000u32, s0 + 1)
                    } else {
                        ("ota_0", 0x110000u32, 0xd000u32, s0 + 1)
                    }
                }
                (None, Some(s1)) => {
                    if (s1 - 1) % 2 == 0 {
                        ("ota_1", 0x210000u32, 0xe000u32, s1 + 1)
                    } else {
                        ("ota_0", 0x110000u32, 0xd000u32, s1 + 1)
                    }
                }
                (None, None) => {
                    ("ota_0", 0x110000u32, 0xd000u32, 1u32)
                }
            };

            println!(">>> [OTA] Direct target: {} (0x{:X}), otadata 0x{:X}, seq {}", target_slot_name, target_addr, otadata_addr, new_seq);

            let _ = unsafe { esp_storage::ll::spiflash_unlock() };

            let sectors_needed = ((total as u32 + SECTOR_SIZE as u32 - 1) / SECTOR_SIZE as u32).max(1);
            println!(">>> [OTA] Direct erase {} sektor ({} KB)...", sectors_needed, sectors_needed * 4);
            for s in 0..sectors_needed {
                let sec_addr = target_addr + (s * SECTOR_SIZE as u32);
                let _ = unsafe { esp_storage::ll::spiflash_erase_sector(sec_addr / SECTOR_SIZE as u32) };
            }
            println!(">>> [OTA] Erase selesai. Mulai streaming direct...");

            let sec = unsafe { &mut SECTOR };
            let mut sec_fill: usize = 0;
            let mut wr_off: u32 = 0;
            let mut total_written: usize = 0;

            for &byte in body0 {
                sec[sec_fill] = byte;
                sec_fill += 1;
                if sec_fill == SECTOR_SIZE {
                    let _ = unsafe {
                        esp_storage::ll::spiflash_write(
                            target_addr + wr_off,
                            sec.as_ptr() as *const u32,
                            SECTOR_SIZE as u32,
                        )
                    };
                    wr_off += SECTOR_SIZE as u32;
                    total_written += SECTOR_SIZE;
                    sec_fill = 0;
                }
            }

            let chk = unsafe { &mut CHUNK };
            let mut stream_completed = false;
            loop {
                match dl.read(chk).await {
                    Ok(0) => {
                        println!(">>> [OTA] Direct stream selesai ({} bytes).", total_written);
                        stream_completed = true;
                        break;
                    }
                    Ok(n) => {
                        let mut src = 0;
                        while src < n {
                            let room = SECTOR_SIZE - sec_fill;
                            let copy = (n - src).min(room);
                            sec[sec_fill..sec_fill + copy].copy_from_slice(&chk[src..src + copy]);
                            sec_fill += copy;
                            src += copy;
                            total_written += copy;
                            if sec_fill == SECTOR_SIZE {
                                let _ = unsafe {
                                    esp_storage::ll::spiflash_write(
                                        target_addr + wr_off,
                                        sec.as_ptr() as *const u32,
                                        SECTOR_SIZE as u32,
                                    )
                                };
                                wr_off += SECTOR_SIZE as u32;
                                sec_fill = 0;
                                if wr_off % (64 * 1024) == 0 {
                                    println!(">>> [OTA] Direct {}% ({} bytes)",
                                        (total_written * 100) / total, total_written);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!(">>> [OTA BUFFER] ⚠️ Direct stream error: {:?} ({} bytes tersimpan).", e, total_written);
                        break;
                    }
                }
            }
            dl.close();

            if sec_fill > 0 {
                for i in sec_fill..SECTOR_SIZE { sec[i] = 0xFF; }
                let _ = unsafe {
                    esp_storage::ll::spiflash_write(
                        target_addr + wr_off,
                        sec.as_ptr() as *const u32,
                        SECTOR_SIZE as u32,
                    )
                };
                total_written += sec_fill;
            }

            println!(">>> [OTA] Direct flash selesai: {} bytes ke {}", total_written, target_slot_name);

            if !stream_completed || total_written < 100_000 {
                println!(">>> [OTA BUFFER] ⚠️ Unduhan belum tuntas (bytes={}). Tidak mengubah boot slot.", total_written);
                continue;
            }

            // Update otadata
            let _ = unsafe { esp_storage::ll::spiflash_erase_sector(otadata_addr / SECTOR_SIZE as u32) };

            let mut entry = EspOtaSelectEntry {
                ota_seq: new_seq,
                seq_label: [0u8; 20],
                ota_state: 2, // Valid
                crc: esp_ota_crc32(&new_seq.to_le_bytes()),
            };
            let label_bytes = target_slot_name.as_bytes();
            for (i, &b) in label_bytes.iter().enumerate().take(20) {
                entry.seq_label[i] = b;
            }

            let _ = unsafe {
                esp_storage::ll::spiflash_write(
                    otadata_addr,
                    &entry as *const _ as *const u32,
                    32,
                )
            };

            println!(">>> [OTA] ✅ Otadata 0x{:X} diset ke {} (seq {})!", otadata_addr, target_slot_name, new_seq);
            println!(">>> [OTA] 🔄 Restart dalam 1 detik... firmware v{} aktif!", version_owned.as_str());
            Timer::after(Duration::from_millis(1000)).await;
            esp_hal::system::software_reset();
        }
    }
}
