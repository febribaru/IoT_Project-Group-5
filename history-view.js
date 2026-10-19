/**
 * ============================================================================
 * E-NOSE ROBUSTA • HISTORY SESI PENGAMBILAN DATA
 * ============================================================================
 * Modul terpisah dari logika pengambilan data (script.js).
 *
 * Penyimpanan (hybrid):
 *   1. Cache lokal browser (localStorage)  -> langsung tersimpan, tetap ada walau offline.
 *   2. Azure Cosmos DB (via /api/history)  -> sumber utama saat online di Vercel,
 *      bisa dibuka dari perangkat/browser mana pun.
 *      Fallback: /api/cloud (backend Rust lokal) untuk menyimpan ke Cosmos saat dev lokal.
 *
 * Bergantung pada global dari script.js: state, getWibIsoString, getWibReadableString, showToast.
 */
(function () {
  'use strict';

  const LS_KEY = 'enose_g05_history_v1';
  const MAX_LOCAL_SESSIONS = 60;
  const RECORD_COLUMNS = ['time_sec', 'mq6', 'mq135', 'mq3', 'mq7', 'tgs2611', 'tgs2602', 'tgs2600', 'tgs2620',
    'dht1_temp', 'dht1_hum', 'dht2_temp', 'dht2_hum'];
  const SENSOR_FALLBACK = [
    { id: 'mq6', name: 'MQ-6', color: '#f97316' }, { id: 'mq135', name: 'MQ-135', color: '#a855f7' },
    { id: 'mq3', name: 'MQ-3', color: '#00f5d4' }, { id: 'mq7', name: 'MQ-7', color: '#ef4444' },
    { id: 'tgs2611', name: 'TGS-2611', color: '#10b981' }, { id: 'tgs2602', name: 'TGS-2602', color: '#f59e0b' },
    { id: 'tgs2600', name: 'TGS-2600', color: '#38bdf8' }, { id: 'tgs2620', name: 'TGS-2620', color: '#ec4899' }
  ];

  const hist = {
    sessions: [],          // gabungan lokal + cloud (ringkasan, records opsional)
    cloudAvailable: null,  // null = belum dicek, true/false
    cloudError: '',
    loading: false,
    detailChart: null,
    activeView: 'acquisition',
    detailChartMode: 'line', // 'line' | 'bar' | 'radar' | 'dht'
    activeSession: null,
    hiddenSensors: new Set()
  };

  // --------------------------------------------------------------------------
  // UTIL
  // --------------------------------------------------------------------------
  const $ = (id) => document.getElementById(id);
  const esc = (v) => String(v ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const num = (v, d = 1) => (typeof v === 'number' && isFinite(v)) ? v.toFixed(d) : '–';
  const toast = (msg, type = 'info') => { if (typeof showToast === 'function') showToast(msg, type); };
  const wibIso = (ms) => (typeof getWibIsoString === 'function') ? getWibIsoString(new Date(ms)) : new Date(ms).toISOString();
  const wibHuman = (ms) => (typeof getWibReadableString === 'function') ? getWibReadableString(new Date(ms)) : new Date(ms).toLocaleString('id-ID');

  function wibParts(ms) {
    const d = new Date((ms || 0) + 7 * 3600 * 1000);
    const pad = (n) => String(n).padStart(2, '0');
    const hari = ['Minggu', 'Senin', 'Selasa', 'Rabu', 'Kamis', 'Jumat', 'Sabtu'][d.getUTCDay()];
    const bulan = ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'][d.getUTCMonth()];
    return {
      dateKey: `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`,
      dateLabel: `${hari}, ${d.getUTCDate()} ${bulan} ${d.getUTCFullYear()}`,
      time: `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`
    };
  }

  function fmtDuration(sec) {
    sec = Math.max(0, Math.round(sec || 0));
    return `${String(Math.floor(sec / 60)).padStart(2, '0')}:${String(sec % 60).padStart(2, '0')}`;
  }

  function stats(arr) {
    const v = arr.filter((x) => typeof x === 'number' && isFinite(x));
    if (!v.length) return { start: null, end: null, min: null, max: null, avg: null };
    const sum = v.reduce((a, b) => a + b, 0);
    return {
      start: v[0], end: v[v.length - 1],
      min: Math.min(...v), max: Math.max(...v),
      avg: Number((sum / v.length).toFixed(2))
    };
  }

  // URL relatif: di Vercel -> fungsi serverless, di lokal -> backend Rust (port 3000)
  const apiBase = () => '';

  // --------------------------------------------------------------------------
  // CACHE LOKAL
  // --------------------------------------------------------------------------
  function convertBankSampleToSession(s) {
    const ts = s.timestamp ? new Date(s.timestamp).getTime() : Date.now();
    const stamp = wibParts(ts);
    const means = s.features?.means || [350, 420, 260, 480, 310, 280, 290, 320];
    const peaks = s.features?.peaks || means;
    const sensors_summary = SENSOR_FALLBACK.map((meta, i) => ({
      ...meta,
      start: means[i] ?? 0,
      end: peaks[i] ?? means[i] ?? 0,
      min: means[i] ?? 0,
      max: peaks[i] ?? means[i] ?? 0,
      avg: means[i] ?? 0,
      delta: Number(((peaks[i] ?? means[i] ?? 0) - (means[i] ?? 0)).toFixed(2))
    }));

    let records = null;
    const dt1_t = typeof s.dht1Temp === 'number' ? s.dht1Temp : 28.5;
    const dt1_h = typeof s.dht1Hum === 'number' ? s.dht1Hum : 65.0;
    const dt2_t = typeof s.dht2Temp === 'number' ? s.dht2Temp : 28.0;
    const dt2_h = typeof s.dht2Hum === 'number' ? s.dht2Hum : 64.5;
    if (Array.isArray(s.rawPoints) && s.rawPoints.length > 0) {
      records = s.rawPoints.map((pt, idx) => {
        if (Array.isArray(pt)) {
          if (pt.length >= 13) return pt;
          if (pt.length >= 9) return [...pt, dt1_t, dt1_h, dt2_t, dt2_h];
          const gas = pt.slice(0, 8);
          while (gas.length < 8) gas.push(350);
          return [idx, ...gas, dt1_t, dt1_h, dt2_t, dt2_h];
        }
        return [idx, 350, 420, 260, 480, 310, 280, 290, 320, dt1_t, dt1_h, dt2_t, dt2_h];
      });
    }

    return {
      id: s.id ? (s.id.startsWith('HIST-') ? s.id : `HIST-${s.id}`) : `HIST-${stamp.dateKey.replace(/-/g, '')}-${stamp.time.replace(/:/g, '')}`,
      doc_type: 'sampling_session',
      schema_version: 1,
      coffee_id: s.coffeeId || 'C05',
      coffee_name: s.coffeeName || 'Sampel Kopi',
      measurement_id: s.measurementId || 'M01',
      origin: s.origin || '-',
      roast: s.roast || '-',
      operator_notes: s.operatorNotes || '',
      group_id: s.groupId || 'G05',
      device_id: s.deviceId || 'DAQ-05',
      status: 'SELESAI',
      started_at: wibIso(ts),
      started_at_ms: ts,
      ended_at: wibIso(ts + (s.recordCount || 300) * 1000),
      ended_at_ms: ts + (s.recordCount || 300) * 1000,
      duration_sec: s.recordCount || (records ? records.length : 300),
      target_duration_sec: 300,
      record_count: s.recordCount || (records ? records.length : 1),
      timezone: 'Asia/Jakarta (WIB / UTC+7)',
      connection: {
        mode: 'Simulasi / Bank Dataset',
        port: 'Bank Sampel AI',
        firmware_version: '1.0.8',
        saved_from: 'Bank Dataset AI'
      },
      environment: {
        dht1_temp: { avg: dt1_t },
        dht1_hum: { avg: dt1_h },
        dht2_temp: { avg: dt2_t },
        dht2_hum: { avg: dt2_h }
      },
      sensors_summary,
      tinyml: null,
      records,
      record_columns: RECORD_COLUMNS,
      synced: false
    };
  }

  function loadLocal() {
    try {
      const arr = JSON.parse(localStorage.getItem(LS_KEY) || '[]');
      if (Array.isArray(arr) && arr.length > 0) return arr;
      // Jika history lokal masih kosong, otomatis impor riwayat dari Bank Dataset
      const bankRaw = localStorage.getItem('enose_g05_sample_bank');
      if (bankRaw) {
        const bank = JSON.parse(bankRaw);
        if (Array.isArray(bank) && bank.length > 0) {
          const imported = bank.map((s) => convertBankSampleToSession(s));
          saveLocal(imported);
          return imported;
        }
      }
      return [];
    } catch (_) { return []; }
  }

  function saveLocal(list) {
    let data = list.slice(0, MAX_LOCAL_SESSIONS);
    for (let attempt = 0; attempt < 12; attempt++) {
      try {
        localStorage.setItem(LS_KEY, JSON.stringify(data));
        return true;
      } catch (e) {
        // Kuota penuh: buang time-series dari sesi tertua yang sudah aman di cloud, lalu sesi tertua.
        const idx = [...data].reverse().findIndex((s) => s.synced && Array.isArray(s.records) && s.records.length);
        if (idx >= 0) {
          const real = data.length - 1 - idx;
          data[real] = { ...data[real], records: null, records_in_cloud_only: true };
        } else if (data.length > 1) {
          data = data.slice(0, -1);
        } else {
          console.warn('[History] Gagal menyimpan cache lokal:', e);
          return false;
        }
      }
    }
    return false;
  }

  function upsertLocal(session) {
    const list = loadLocal().filter((s) => s.id !== session.id);
    list.unshift(session);
    list.sort((a, b) => (b.started_at_ms || 0) - (a.started_at_ms || 0));
    saveLocal(list);
  }

  // --------------------------------------------------------------------------
  // CLOUD (Cosmos DB)
  // --------------------------------------------------------------------------
  async function cloudSave(session) {
    const payload = { ...session };
    delete payload.synced;
    try {
      const r = await fetch(`${apiBase()}/api/history`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session: payload })
      });
      const j = await r.json().catch(() => null);
      if (j && typeof j.success === 'boolean') {
        if (j.success) return true;
        if (r.status !== 404) return false;
      }
    } catch (_) { /* lanjut ke fallback */ }

    // Fallback backend Rust lokal: /api/cloud tetap meneruskan dokumen ke Cosmos DB.
    try {
      const r2 = await fetch(`${apiBase()}/api/cloud`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action: 'save_sample', sample: { ...payload, doc_type: 'sampling_session' } })
      });
      if (!r2.ok) return false;
      const j2 = await r2.json().catch(() => ({}));
      return !!j2.success && !j2.note;
    } catch (_) { return false; }
  }

  async function cloudList() {
    const r = await fetch(`${apiBase()}/api/history`, { cache: 'no-store' });
    if (!r.ok) throw new Error(r.status === 404 ? 'Endpoint /api/history belum tersedia (mode lokal)' : `HTTP ${r.status}`);
    const j = await r.json();
    if (!j.success) throw new Error(j.error || 'Gagal membaca Cosmos DB');
    return j.sessions || [];
  }

  async function cloudGet(id, pk) {
    try {
      const ctrl = typeof AbortController !== 'undefined' ? new AbortController() : null;
      const tid = ctrl ? setTimeout(() => ctrl.abort(), 1200) : null;
      const opts = { cache: 'no-store' };
      if (ctrl) opts.signal = ctrl.signal;
      const r = await fetch(`${apiBase()}/api/history?id=${encodeURIComponent(id)}&pk=${encodeURIComponent(pk || 'C05')}`, opts);
      if (tid) clearTimeout(tid);
      if (!r.ok) return null;
      const j = await r.json().catch(() => null);
      return j && j.success ? j.session : null;
    } catch (_) {
      return null;
    }
  }

  async function cloudDelete(id, pk) {
    try {
      const r = await fetch(`${apiBase()}/api/history?id=${encodeURIComponent(id)}&pk=${encodeURIComponent(pk || 'C05')}`, { method: 'DELETE' });
      return r.ok;
    } catch (_) { return false; }
  }

  async function syncPending(silent = true) {
    const list = loadLocal();
    const pending = list.filter((s) => !s.synced && Array.isArray(s.records));
    if (!pending.length) return 0;
    let ok = 0;
    for (const s of pending) {
      if (await cloudSave(s)) { s.synced = true; ok++; }
    }
    if (ok) saveLocal(list);
    if (!silent) toast(ok ? `☁️ ${ok}/${pending.length} sesi berhasil disinkronkan ke Cosmos DB` : '⚠️ Sinkronisasi gagal, data tetap aman di cache lokal', ok ? 'success' : 'warning');
    return ok;
  }

  // --------------------------------------------------------------------------
  // CAPTURE SESI SAAT SAMPLING BERHENTI (dipanggil dari stopSamplingSession)
  // --------------------------------------------------------------------------
  function buildSession(isCompleted) {
    const st = window.state || (typeof state !== 'undefined' ? state : null);
    if (!st) return null;

    let recs = Array.isArray(st.records) ? [...st.records] : [];
    if (recs.length === 0) {
      const vals = (st.sensors || []).map((s) => Number((s.currentVal || s.baseVal || 0).toFixed(1)));
      recs = [{
        time_sec: 0,
        timeSec: 0,
        values: vals,
        mq6: vals[0] || 0, mq135: vals[1] || 0, mq3: vals[2] || 0, mq7: vals[3] || 0,
        tgs2611: vals[4] || 0, tgs2602: vals[5] || 0, tgs2600: vals[6] || 0, tgs2620: vals[7] || 0,
        dht1_temp: st.dht1Temp || 0, dht1_hum: st.dht1Hum || 0,
        dht2_temp: st.dht2Temp || 0, dht2_hum: st.dht2Hum || 0
      }];
    }

    const startMs = st.samplingStartTimeMs || (Date.now() - recs.length * 1000);
    const endMs = Date.now();
    const rows = recs.map((r) => {
      const v = Array.isArray(r.values) ? r.values : [r.mq6, r.mq135, r.mq3, r.mq7, r.tgs2611, r.tgs2602, r.tgs2600, r.tgs2620];
      const t = r.time_sec ?? r.timeSec ?? 0;
      return [t, ...v.slice(0, 8).map((x) => Number(Number(x || 0).toFixed(2))),
        Number(Number(r.dht1_temp ?? 0).toFixed(2)), Number(Number(r.dht1_hum ?? 0).toFixed(2)),
        Number(Number(r.dht2_temp ?? 0).toFixed(2)), Number(Number(r.dht2_hum ?? 0).toFixed(2))];
    });

    const sensorsMeta = (st.sensors || SENSOR_FALLBACK).map((s, i) => ({
      id: s.id || SENSOR_FALLBACK[i]?.id, name: s.name || SENSOR_FALLBACK[i]?.name,
      gas_type: s.gasType || '', color: s.color || SENSOR_FALLBACK[i]?.color
    }));
    const sensors_summary = sensorsMeta.slice(0, 8).map((m, i) => {
      const s = stats(rows.map((row) => row[i + 1]));
      return { ...m, ...s, delta: (s.end !== null && s.start !== null) ? Number((s.end - s.start).toFixed(2)) : null };
    });

    const envStat = (col) => stats(rows.map((row) => row[col]));
    const dev = st.lastDeviceInfo || {};
    const portSel = document.getElementById('portSelect');
    const port = portSel ? portSel.value : (dev.port || '-');
    const fw = dev.active_firmware_version || dev.firmware_version || '-';
    const mode = port === 'SIM' ? 'Simulator'
      : (dev.connection_mode === 'usb_serial' ? 'USB Serial (Kabel)'
        : (port === 'AZURE_IOT_HUB' || dev.connection_mode === 'wireless_iot' ? 'IoT Nirkabel (Wi-Fi / Cloud Relay)' : 'Wi-Fi UDP / Serial Lokal'));
    const inf = st.currentInference || null;
    const last = rows[rows.length - 1];
    const stamp = wibParts(startMs);

    return {
      id: `HIST-${stamp.dateKey.replace(/-/g, '')}-${stamp.time.replace(/:/g, '')}-${Math.random().toString(36).slice(2, 6).toUpperCase()}`,
      doc_type: 'sampling_session',
      schema_version: 1,
      coffee_id: (st.coffeeId || 'C05').toUpperCase(),
      coffee_name: st.coffeeName || st.coffeeSampleName || 'Sampel Kopi',
      measurement_id: st.measurementId || 'M01',
      origin: st.coffeeOrigin || '-',
      roast: st.coffeeRoast || '-',
      operator_notes: st.coffeeOperatorNotes || '',
      group_id: st.groupId || 'G05',
      device_id: st.deviceId || 'DAQ-05',
      status: isCompleted ? 'SELESAI' : 'DIHENTIKAN',
      started_at: wibIso(startMs),
      started_at_ms: startMs,
      ended_at: wibIso(endMs),
      ended_at_ms: endMs,
      duration_sec: last ? last[0] : Math.round((endMs - startMs) / 1000),
      target_duration_sec: st.samplingDurationSec || 300,
      record_count: rows.length,
      timezone: 'Asia/Jakarta (WIB / UTC+7)',
      connection: {
        mode, port, firmware_version: fw,
        wifi_ssid: dev.wifi_ssid || null, wifi_ip: dev.wifi_ip || null,
        backend_ws: !!st.isWsConnected,
        host: location.hostname || 'local'
      },
      environment: {
        dht1_temp: envStat(9), dht1_hum: envStat(10),
        dht2_temp: envStat(11), dht2_hum: envStat(12)
      },
      sensors_summary,
      tinyml: inf && inf.label && inf.label !== 'STANDBY'
        ? { label: inf.label, confidence: Number(inf.confidence || 0), latency_us: inf.latency_us || null }
        : null,
      saved_from: location.hostname && location.hostname.includes('vercel') ? 'Vercel Cloud Dashboard' : 'Local Dashboard',
      record_columns: RECORD_COLUMNS,
      records: rows,
      synced: false
    };
  }

  async function saveSamplingSessionToHistory(isCompleted) {
    try {
      const session = buildSession(isCompleted);
      if (!session) return null;
      upsertLocal(session);
      refreshTabBadge();
      if (hist.activeView === 'history') renderList();

      const ok = await cloudSave(session);
      if (ok) {
        const list = loadLocal();
        const it = list.find((s) => s.id === session.id);
        if (it) { it.synced = true; saveLocal(list); }
      }
      toast(ok ? `🗂️ Sesi "${session.coffee_name}" tersimpan ke History (☁️ Cosmos DB)` : `🗂️ Sesi "${session.coffee_name}" tersimpan ke History (💾 lokal, sinkron otomatis nanti)`, ok ? 'success' : 'info');
      if (hist.activeView === 'history') loadHistory();
      return session;
    } catch (e) {
      console.warn('[History] Gagal menyimpan sesi:', e);
      return null;
    }
  }

  // --------------------------------------------------------------------------
  // LOAD & MERGE
  // --------------------------------------------------------------------------
  async function loadHistory() {
    if (hist.loading) return;
    hist.loading = true;
    setStorageChip('loading');
    const local = loadLocal().map((s) => ({ ...s, _local: true }));
    hist.sessions = local;
    renderList();

    try {
      await syncPending(true);
      const cloud = await cloudList();
      hist.cloudAvailable = true;
      hist.cloudError = '';
      const localNow = loadLocal();
      const map = new Map();
      cloud.forEach((c) => map.set(c.id, { ...c, synced: true, _cloud: true }));
      localNow.forEach((l) => {
        const c = map.get(l.id);
        map.set(l.id, c ? { ...c, records: l.records, record_columns: l.record_columns, _local: true, _cloud: true, synced: true }
          : { ...l, _local: true });
      });
      hist.sessions = [...map.values()].sort((a, b) => (b.started_at_ms || 0) - (a.started_at_ms || 0));
    } catch (e) {
      hist.cloudAvailable = false;
      hist.cloudError = e.message;
      hist.sessions = loadLocal().map((s) => ({ ...s, _local: true }));
    } finally {
      hist.loading = false;
      setStorageChip();
      renderList();
      refreshTabBadge();
    }
  }

  // --------------------------------------------------------------------------
  // RENDER
  // --------------------------------------------------------------------------
  function setStorageChip(mode) {
    const chip = $('historyStorageChip');
    if (!chip) return;
    if (mode === 'loading') {
      chip.className = 'hist-chip hist-chip-loading';
      chip.textContent = '⏳ Memuat data dari Cosmos DB...';
      return;
    }
    if (hist.cloudAvailable) {
      chip.className = 'hist-chip hist-chip-cloud';
      chip.textContent = '☁️ Azure Cosmos DB tersambung • cache lokal aktif';
      chip.title = 'Data history dibaca dari Azure Cosmos DB (enose_db/telemetry)';
    } else {
      chip.className = 'hist-chip hist-chip-local';
      chip.textContent = '💾 Mode lokal (cache browser) • cloud belum terhubung';
      chip.title = hist.cloudError || '';
    }
  }

  function getFiltered() {
    const q = ($('histSearch')?.value || '').trim().toLowerCase();
    const roast = $('histFilterRoast')?.value || '';
    const status = $('histFilterStatus')?.value || '';
    const from = $('histDateFrom')?.value || '';
    const to = $('histDateTo')?.value || '';
    return hist.sessions.filter((s) => {
      const key = wibParts(s.started_at_ms).dateKey;
      if (from && key < from) return false;
      if (to && key > to) return false;
      if (roast && s.roast !== roast) return false;
      if (status && s.status !== status) return false;
      if (q) {
        const hay = `${s.coffee_name} ${s.coffee_id} ${s.measurement_id} ${s.origin} ${s.operator_notes} ${s.id}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }

  function renderStats(list) {
    const todayKey = wibParts(Date.now()).dateKey;
    const total = list.length;
    const points = list.reduce((a, s) => a + (s.record_count || 0), 0);
    const today = list.filter((s) => wibParts(s.started_at_ms).dateKey === todayKey).length;
    const last = list[0];
    const set = (id, v) => { const el = $(id); if (el) el.textContent = v; };
    set('histStatTotal', total);
    set('histStatPoints', points.toLocaleString('id-ID'));
    set('histStatToday', today);
    set('histStatLast', last ? `${wibParts(last.started_at_ms).dateLabel} • ${wibParts(last.started_at_ms).time}` : '–');
  }

  function renderList() {
    const body = $('historyTableBody');
    if (!body) return;

    // Isi opsi filter roast secara dinamis
    const roastSel = $('histFilterRoast');
    if (roastSel) {
      const cur = roastSel.value;
      const roasts = [...new Set(hist.sessions.map((s) => s.roast).filter(Boolean))].sort();
      roastSel.innerHTML = '<option value="">Semua Roast</option>' + roasts.map((r) => `<option value="${esc(r)}">${esc(r)}</option>`).join('');
      roastSel.value = roasts.includes(cur) ? cur : '';
    }

    const list = getFiltered();
    renderStats(list);

    if (!list.length) {
      body.innerHTML = `<tr><td colspan="9" class="hist-empty">
        <div class="hist-empty-icon">🗂️</div>
        <strong>${hist.sessions.length ? 'Tidak ada sesi yang cocok dengan filter' : 'Belum ada riwayat pengambilan data'}</strong>
        <p>${hist.sessions.length ? 'Ubah kata kunci / rentang tanggal.' : 'Setiap kali sesi sampling dihentikan atau selesai (300 s), seluruh kondisinya otomatis tersimpan di sini.'}</p>
      </td></tr>`;
      return;
    }

    let html = '';
    let lastDate = '';
    list.forEach((s) => {
      const p = wibParts(s.started_at_ms);
      const pe = wibParts(s.ended_at_ms || s.started_at_ms);
      if (p.dateKey !== lastDate) {
        lastDate = p.dateKey;
        html += `<tr class="hist-date-row"><td colspan="9">📅 ${esc(p.dateLabel)}</td></tr>`;
      }
      const env = s.environment || {};
      const t1 = env.dht1_temp?.avg, h1 = env.dht1_hum?.avg, t2 = env.dht2_temp?.avg, h2 = env.dht2_hum?.avg;
      const tiny = s.tinyml ? `${esc(s.tinyml.label)} <span class="hist-muted">${Math.round((s.tinyml.confidence || 0) * 100)}%</span>` : '<span class="hist-muted">–</span>';
      const storage = s._cloud || s.synced
        ? '<span class="hist-store hist-store-cloud" title="Tersimpan di Azure Cosmos DB">☁️ Cloud</span>'
        : '<span class="hist-store hist-store-local" title="Baru di cache browser, akan disinkron otomatis">💾 Lokal</span>';
      html += `
        <tr class="hist-row" data-id="${esc(s.id)}">
          <td><div class="hist-time">${p.time}</div><div class="hist-muted">s/d ${pe.time} WIB</div></td>
          <td><div class="hist-name">${esc(s.coffee_name)}</div><div class="hist-muted font-mono">${esc(s.coffee_id)} • ${esc(s.measurement_id)}</div></td>
          <td><div>${esc(s.roast)}</div><div class="hist-muted">${esc(s.origin)}</div></td>
          <td class="font-mono">${fmtDuration(s.duration_sec)}<div class="hist-muted">${s.record_count || 0} titik</div></td>
          <td><span class="hist-status ${s.status === 'SELESAI' ? 'ok' : 'stop'}">${s.status === 'SELESAI' ? '✅ Selesai' : '⏹️ Dihentikan'}</span></td>
          <td class="font-mono">${num(t1)}°C / ${num(h1)}%<div class="hist-muted">${num(t2)}°C / ${num(h2)}%</div></td>
          <td>${tiny}</td>
          <td>${storage}</td>
          <td class="hist-actions">
            <button class="hist-btn" data-act="detail" data-id="${esc(s.id)}" title="Lihat semua kondisi">🔍 Detail</button>
            <button class="hist-btn" data-act="csv" data-id="${esc(s.id)}" title="Unduh CSV time-series">⬇️ CSV</button>
            <button class="hist-btn hist-btn-danger" data-act="delete" data-id="${esc(s.id)}" title="Hapus sesi">🗑️</button>
          </td>
        </tr>`;
    });
    body.innerHTML = html;
  }

  function refreshTabBadge() {
    const badge = $('tabHistoryCount');
    if (!badge) return;
    const n = hist.sessions.length || loadLocal().length;
    badge.textContent = n;
    badge.style.display = n ? 'inline-flex' : 'none';
  }

  // --------------------------------------------------------------------------
  // DETAIL
  // --------------------------------------------------------------------------
  async function getFullSession(id) {
    let s = hist.sessions.find((x) => x.id === id) || loadLocal().find((x) => x.id === id);
    if (!s) return null;
    if (!Array.isArray(s.records) || !s.records.length) {
      const full = await cloudGet(id, s.coffee_id);
      if (full) s = { ...s, ...full, _cloud: true, synced: true };
    }
    return s;
  }

  function envRow(label, st, unit) {
    st = st || {};
    return `<tr><td>${label}</td><td>${num(st.start)}${unit}</td><td>${num(st.end)}${unit}</td>
      <td>${num(st.min)}${unit}</td><td>${num(st.max)}${unit}</td><td><strong>${num(st.avg)}${unit}</strong></td></tr>`;
  }

  // --------------------------------------------------------------------------
  // NORMALISASI DATA UNTUK GRAFIK & ANALISIS
  // --------------------------------------------------------------------------
  function normalizeSessionData(s) {
    s = s || {};
    const rawSummary = Array.isArray(s.sensors_summary) && s.sensors_summary.length ? s.sensors_summary : [];
    const meta = SENSOR_FALLBACK.map((fb, i) => {
      const src = rawSummary[i] || {};
      const start = typeof src.start === 'number' ? src.start : (typeof src.min === 'number' ? src.min : 350);
      const max = typeof src.max === 'number' ? src.max : (typeof src.end === 'number' ? src.end : start);
      const min = typeof src.min === 'number' ? src.min : Math.min(start, max);
      const avg = typeof src.avg === 'number' ? src.avg : Number(((start + max) / 2).toFixed(1));
      const delta = typeof src.delta === 'number' ? src.delta : Number((max - start).toFixed(1));
      return {
        id: src.id || fb.id,
        name: src.name || fb.name,
        color: src.color || fb.color,
        gas_type: src.gas_type || '',
        start, end: typeof src.end === 'number' ? src.end : max,
        min, max, avg, delta
      };
    });

    let timeSeries = [];
    let dhtSeries = [];
    let hasTimeSeries = false;

    if (Array.isArray(s.records) && s.records.length > 0) {
      s.records.forEach((r, idx) => {
        let t = idx;
        let gasVals = [];
        let dhtVals = [null, null, null, null]; // [dht1_temp, dht1_hum, dht2_temp, dht2_hum]

        if (Array.isArray(r)) {
          if (r.length >= 9) {
            t = typeof r[0] === 'number' ? r[0] : idx;
            gasVals = r.slice(1, 9).map((v) => Number(v) || 0);
            if (r.length >= 13) {
              dhtVals = [r[9], r[10], r[11], r[12]].map((v) => (typeof v === 'number' && !isNaN(v) ? v : null));
            }
          } else if (r.length === 8) {
            t = idx;
            gasVals = r.slice(0, 8).map((v) => Number(v) || 0);
          } else {
            t = idx;
            gasVals = r.slice(0, 8).map((v) => Number(v) || 0);
          }
        } else if (r && typeof r === 'object') {
          t = r.time_sec ?? r.timeSec ?? idx;
          if (Array.isArray(r.values)) {
            gasVals = r.values.slice(0, 8).map((v) => Number(v) || 0);
          } else {
            gasVals = [
              r.mq6 ?? 0, r.mq135 ?? 0, r.mq3 ?? 0, r.mq7 ?? 0,
              r.tgs2611 ?? 0, r.tgs2602 ?? 0, r.tgs2600 ?? 0, r.tgs2620 ?? 0
            ];
          }
          dhtVals = [r.dht1_temp, r.dht1_hum, r.dht2_temp, r.dht2_hum].map((v) => (typeof v === 'number' && !isNaN(v) ? v : null));
        }

        while (gasVals.length < 8) gasVals.push(350);
        timeSeries.push({ t, vals: gasVals });
        dhtSeries.push({ t, dht: dhtVals });
      });

      if (timeSeries.length > 1) {
        hasTimeSeries = true;
      }
    }

    let isReconstructed = false;
    if (!hasTimeSeries) {
      isReconstructed = true;
      const dur = Math.max(s.duration_sec || 300, 30);
      const points = 25;
      const step = dur / (points - 1);
      timeSeries = [];
      dhtSeries = [];
      const env = s.environment || {};
      const t1 = env.dht1_temp?.avg ?? 28.5;
      const h1 = env.dht1_hum?.avg ?? 65.0;
      const t2 = env.dht2_temp?.avg ?? 28.0;
      const h2 = env.dht2_hum?.avg ?? 64.5;

      for (let i = 0; i < points; i++) {
        const t = Math.round(i * step);
        const normT = i / (points - 1);
        let shape = 0;
        if (normT < 0.15) {
          shape = 0.05 * (normT / 0.15);
        } else if (normT < 0.65) {
          const p = (normT - 0.15) / 0.5;
          shape = 0.05 + 0.95 * Math.sin(p * Math.PI / 2);
        } else {
          const p = (normT - 0.65) / 0.35;
          shape = 1.0 - 0.2 * Math.sin(p * Math.PI / 2);
        }

        const vals = meta.map((m) => {
          const base = m.start;
          const peak = m.max;
          return Number((base + (peak - base) * shape).toFixed(1));
        });
        timeSeries.push({ t, vals });
        dhtSeries.push({ t, dht: [t1, h1, t2, h2] });
      }
    }

    let peakSensor = meta[0];
    let maxDeltaSensor = meta[0];
    meta.forEach((m) => {
      if ((m.max ?? 0) > (peakSensor.max ?? 0)) peakSensor = m;
      if (Math.abs(m.delta ?? 0) > Math.abs(maxDeltaSensor.delta ?? 0)) maxDeltaSensor = m;
    });

    return {
      meta,
      timeSeries,
      dhtSeries,
      hasTimeSeries,
      isReconstructed,
      peakSensor,
      maxDeltaSensor,
      totalPoints: s.record_count || timeSeries.length,
      durationSec: s.duration_sec || (timeSeries[timeSeries.length - 1]?.t ?? 300)
    };
  }

  // --------------------------------------------------------------------------
  // RENDER GRAFIK DETAIL HISTORI (Multi-Mode: Line, Bar, Radar, DHT)
  // --------------------------------------------------------------------------
  function renderHistoryDetailChart(s, mode) {
    const canvas = $('historyDetailChart');
    if (!canvas || typeof Chart === 'undefined') return;
    if (hist.detailChart) {
      hist.detailChart.destroy();
      hist.detailChart = null;
    }

    const norm = normalizeSessionData(s);
    mode = mode || hist.detailChartMode || (norm.hasTimeSeries ? 'line' : 'bar');
    hist.detailChartMode = mode;

    document.querySelectorAll('.hist-chart-tab').forEach((tab) => {
      tab.classList.toggle('active', tab.dataset.mode === mode);
    });

    const sub = $('histChartSubtitle');
    if (sub) {
      if (mode === 'line') {
        sub.innerHTML = norm.hasTimeSeries
          ? `Kurva dinamika respon 8 sensor gas terhadap waktu (${norm.timeSeries.length} titik data sampling)`
          : `Kurva respon aroma (<span style="color:#fcd34d;">rekonstruksi profil respon</span> dari ringkasan baseline & puncak)`;
      } else if (mode === 'bar') {
        sub.textContent = 'Perbandingan profil tegangan awal baseline, puncak maksimum, rata-rata, dan delta respon (mV)';
      } else if (mode === 'radar') {
        sub.textContent = 'Sidik jari aroma 8 sensor (Aroma Fingerprint: Baseline vs Respon Puncak)';
      } else if (mode === 'dht') {
        sub.textContent = 'Pemantauan stabilitas lingkungan chamber: Suhu (°C) & Kelembaban (%) DHT22';
      }
    }

    const legendEl = $('histChartLegend');
    if (legendEl) {
      legendEl.style.display = (mode === 'dht') ? 'none' : 'flex';
      legendEl.querySelectorAll('.hist-legend-chip').forEach((ch) => {
        const idx = Number(ch.dataset.sensorIdx);
        ch.classList.toggle('inactive', hist.hiddenSensors.has(idx));
      });
    }

    const ctx = canvas.getContext('2d');

    if (mode === 'line') {
      const labels = norm.timeSeries.map((p) => `${p.t}s`);
      const datasets = norm.meta.map((m, i) => ({
        label: m.name,
        data: norm.timeSeries.map((p) => p.vals[i]),
        borderColor: m.color,
        backgroundColor: 'transparent',
        borderWidth: 2,
        pointRadius: norm.timeSeries.length > 60 ? 0 : 2.5,
        pointHoverRadius: 5,
        tension: 0.28,
        hidden: hist.hiddenSensors.has(i)
      }));

      hist.detailChart = new Chart(ctx, {
        type: 'line',
        data: { labels, datasets },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          animation: false,
          interaction: { mode: 'index', intersect: false },
          plugins: {
            legend: { display: false },
            tooltip: {
              backgroundColor: 'rgba(10, 15, 26, 0.95)',
              titleColor: '#00f5d4',
              bodyColor: '#f8fafc',
              borderColor: 'rgba(0, 245, 212, 0.35)',
              borderWidth: 1,
              padding: 10,
              boxPadding: 4,
              callbacks: {
                title: (items) => `⏱️ Waktu: ${items[0]?.label || '0s'}`,
                label: (ctx) => `  ${ctx.dataset.label}: ${ctx.raw} mV`
              }
            }
          },
          scales: {
            x: {
              ticks: { color: '#64748b', maxTicksLimit: 14, font: { family: 'JetBrains Mono', size: 10 } },
              grid: { color: 'rgba(255,255,255,0.04)' },
              title: { display: true, text: 'Waktu Sampling (detik)', color: '#94a3b8', font: { size: 11, weight: '600' } }
            },
            y: {
              ticks: { color: '#64748b', font: { family: 'JetBrains Mono', size: 10 } },
              grid: { color: 'rgba(255,255,255,0.05)' },
              title: { display: true, text: 'Tegangan Sensor (mV)', color: '#94a3b8', font: { size: 11, weight: '600' } }
            }
          }
        }
      });
    } else if (mode === 'bar') {
      const labels = norm.meta.map((m) => m.name);
      const isHidden = (i) => hist.hiddenSensors.has(i);

      const datasets = [
        {
          label: 'Awal / Baseline (mV)',
          data: norm.meta.map((m, i) => (isHidden(i) ? null : m.start)),
          backgroundColor: 'rgba(100, 116, 139, 0.65)',
          borderColor: '#94a3b8',
          borderWidth: 1.2,
          borderRadius: 4
        },
        {
          label: 'Puncak Maksimum (mV)',
          data: norm.meta.map((m, i) => (isHidden(i) ? null : m.max)),
          backgroundColor: norm.meta.map((m, i) => (isHidden(i) ? 'transparent' : m.color + 'cc')),
          borderColor: norm.meta.map((m) => m.color),
          borderWidth: 1.5,
          borderRadius: 4
        },
        {
          label: 'Rata-rata (mV)',
          data: norm.meta.map((m, i) => (isHidden(i) ? null : m.avg)),
          backgroundColor: 'rgba(0, 245, 212, 0.45)',
          borderColor: '#00f5d4',
          borderWidth: 1.2,
          borderRadius: 4
        },
        {
          label: 'Δ Respon (mV)',
          data: norm.meta.map((m, i) => (isHidden(i) ? null : m.delta)),
          backgroundColor: 'rgba(168, 85, 247, 0.45)',
          borderColor: '#a855f7',
          borderWidth: 1.2,
          borderRadius: 4
        }
      ];

      hist.detailChart = new Chart(ctx, {
        type: 'bar',
        data: { labels, datasets },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          animation: { duration: 250 },
          plugins: {
            legend: {
              labels: { color: '#cbd5e1', boxWidth: 12, font: { size: 11, family: 'Inter' } }
            },
            tooltip: {
              backgroundColor: 'rgba(10, 15, 26, 0.95)',
              titleColor: '#00f5d4',
              bodyColor: '#f8fafc',
              borderColor: 'rgba(0, 245, 212, 0.35)',
              borderWidth: 1,
              padding: 10
            }
          },
          scales: {
            x: {
              ticks: { color: '#cbd5e1', font: { family: 'JetBrains Mono', size: 10, weight: '600' } },
              grid: { color: 'rgba(255,255,255,0.04)' }
            },
            y: {
              ticks: { color: '#64748b', font: { family: 'JetBrains Mono', size: 10 } },
              grid: { color: 'rgba(255,255,255,0.05)' },
              title: { display: true, text: 'Tegangan Sensor (mV)', color: '#94a3b8', font: { size: 11, weight: '600' } }
            }
          }
        }
      });
    } else if (mode === 'radar') {
      const labels = norm.meta.map((m) => m.name);
      const isHidden = (i) => hist.hiddenSensors.has(i);

      hist.detailChart = new Chart(ctx, {
        type: 'radar',
        data: {
          labels,
          datasets: [
            {
              label: 'Baseline Awal (mV)',
              data: norm.meta.map((m, i) => (isHidden(i) ? 0 : m.start)),
              borderColor: 'rgba(148, 163, 184, 0.7)',
              borderDash: [4, 4],
              backgroundColor: 'rgba(148, 163, 184, 0.12)',
              pointBackgroundColor: '#94a3b8',
              pointRadius: 3
            },
            {
              label: 'Respon Puncak (mV)',
              data: norm.meta.map((m, i) => (isHidden(i) ? 0 : m.max)),
              borderColor: '#00f5d4',
              backgroundColor: 'rgba(0, 245, 212, 0.25)',
              borderWidth: 2,
              pointBackgroundColor: '#00f5d4',
              pointRadius: 4
            },
            {
              label: 'Rata-rata (mV)',
              data: norm.meta.map((m, i) => (isHidden(i) ? 0 : m.avg)),
              borderColor: '#f59e0b',
              backgroundColor: 'rgba(245, 158, 11, 0.15)',
              borderWidth: 1.8,
              pointBackgroundColor: '#f59e0b',
              pointRadius: 3
            }
          ]
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          animation: { duration: 250 },
          plugins: {
            legend: {
              labels: { color: '#cbd5e1', boxWidth: 12, font: { size: 11, family: 'Inter' } }
            },
            tooltip: {
              backgroundColor: 'rgba(10, 15, 26, 0.95)',
              titleColor: '#00f5d4',
              bodyColor: '#f8fafc',
              borderColor: 'rgba(0, 245, 212, 0.35)',
              borderWidth: 1,
              padding: 10
            }
          },
          scales: {
            r: {
              angleLines: { color: 'rgba(255,255,255,0.08)' },
              grid: { color: 'rgba(255,255,255,0.08)' },
              pointLabels: { color: '#cbd5e1', font: { family: 'JetBrains Mono', size: 11, weight: '600' } },
              ticks: { backdropColor: 'transparent', color: '#64748b', font: { size: 9 } }
            }
          }
        }
      });
    } else if (mode === 'dht') {
      const labels = norm.dhtSeries.map((p) => `${p.t}s`);
      const hasDhtPoints = norm.dhtSeries.some((p) => p.dht[0] !== null);

      if (hasDhtPoints && norm.dhtSeries.length > 1) {
        hist.detailChart = new Chart(ctx, {
          type: 'line',
          data: {
            labels,
            datasets: [
              { label: 'DHT #1 Suhu (°C)', data: norm.dhtSeries.map((p) => p.dht[0]), borderColor: '#f97316', backgroundColor: 'transparent', yAxisID: 'yTemp', borderWidth: 2, pointRadius: 0, tension: 0.2 },
              { label: 'DHT #2 Suhu (°C)', data: norm.dhtSeries.map((p) => p.dht[2]), borderColor: '#ef4444', backgroundColor: 'transparent', yAxisID: 'yTemp', borderWidth: 1.8, borderDash: [4, 3], pointRadius: 0, tension: 0.2 },
              { label: 'DHT #1 Kelembaban (%)', data: norm.dhtSeries.map((p) => p.dht[1]), borderColor: '#00f5d4', backgroundColor: 'transparent', yAxisID: 'yHum', borderWidth: 2, pointRadius: 0, tension: 0.2 },
              { label: 'DHT #2 Kelembaban (%)', data: norm.dhtSeries.map((p) => p.dht[3]), borderColor: '#38bdf8', backgroundColor: 'transparent', yAxisID: 'yHum', borderWidth: 1.8, borderDash: [4, 3], pointRadius: 0, tension: 0.2 }
            ]
          },
          options: {
            responsive: true,
            maintainAspectRatio: false,
            animation: false,
            interaction: { mode: 'index', intersect: false },
            plugins: {
              legend: { labels: { color: '#cbd5e1', boxWidth: 12, font: { size: 11 } } },
              tooltip: {
                backgroundColor: 'rgba(10, 15, 26, 0.95)',
                titleColor: '#00f5d4',
                bodyColor: '#f8fafc',
                borderColor: 'rgba(0, 245, 212, 0.35)',
                borderWidth: 1,
                padding: 10
              }
            },
            scales: {
              x: { ticks: { color: '#64748b', maxTicksLimit: 14 }, grid: { color: 'rgba(255,255,255,0.04)' }, title: { display: true, text: 'Waktu (detik)', color: '#94a3b8' } },
              yTemp: { position: 'left', ticks: { color: '#f97316' }, grid: { color: 'rgba(255,255,255,0.05)' }, title: { display: true, text: 'Suhu (°C)', color: '#f97316' } },
              yHum: { position: 'right', ticks: { color: '#00f5d4' }, grid: { drawOnChartArea: false }, title: { display: true, text: 'Kelembaban (%)', color: '#00f5d4' } }
            }
          }
        });
      } else {
        const env = s.environment || {};
        hist.detailChart = new Chart(ctx, {
          type: 'bar',
          data: {
            labels: ['DHT #1 Suhu (°C)', 'DHT #2 Suhu (°C)', 'DHT #1 Hum (%)', 'DHT #2 Hum (%)'],
            datasets: [
              {
                label: 'Awal',
                data: [env.dht1_temp?.start ?? 28, env.dht2_temp?.start ?? 28, env.dht1_hum?.start ?? 65, env.dht2_hum?.start ?? 65],
                backgroundColor: 'rgba(100, 116, 139, 0.6)'
              },
              {
                label: 'Rata-rata',
                data: [env.dht1_temp?.avg ?? 28.5, env.dht2_temp?.avg ?? 28.2, env.dht1_hum?.avg ?? 65.5, env.dht2_hum?.avg ?? 64.8],
                backgroundColor: ['#f97316', '#ef4444', '#00f5d4', '#38bdf8']
              },
              {
                label: 'Akhir',
                data: [env.dht1_temp?.end ?? 28.7, env.dht2_temp?.end ?? 28.4, env.dht1_hum?.end ?? 66, env.dht2_hum?.end ?? 65],
                backgroundColor: 'rgba(255, 255, 255, 0.3)'
              }
            ]
          },
          options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: { legend: { labels: { color: '#cbd5e1' } } },
            scales: {
              x: { ticks: { color: '#cbd5e1' } },
              y: { ticks: { color: '#64748b' }, title: { display: true, text: 'Nilai (°C / %)', color: '#94a3b8' } }
            }
          }
        });
      }
    }
  }

  // --------------------------------------------------------------------------
  // BUKA MODAL DETAIL SESI
  // --------------------------------------------------------------------------
  async function openDetail(id) {
    const modal = $('historyDetailModal');
    const content = $('historyDetailContent');
    if (!modal || !content) return;
    content.innerHTML = '<div class="hist-loading">⏳ Memuat detail sesi...</div>';
    modal.style.display = 'flex';

    try {
      const s = await getFullSession(id);
      if (!s) { content.innerHTML = '<div class="hist-loading">Sesi tidak ditemukan.</div>'; return; }

    hist.activeSession = s;
    hist.hiddenSensors.clear();
    const norm = normalizeSessionData(s);
    hist.detailChartMode = norm.hasTimeSeries ? 'line' : 'bar';

    const p = wibParts(s.started_at_ms), pe = wibParts(s.ended_at_ms || s.started_at_ms);
    const c = s.connection || {}, env = s.environment || {};
    const sensors = norm.meta;

    content.innerHTML = `
      <div class="hist-detail-head">
        <div>
          <div class="hist-detail-kicker">🗂️ DETAIL SESI PENGAMBILAN DATA</div>
          <h3>${esc(s.coffee_name)} <span class="hist-muted font-mono">${esc(s.coffee_id)} • ${esc(s.measurement_id)}</span></h3>
          <div class="hist-muted font-mono">${esc(s.id)}</div>
        </div>
        <span class="hist-status ${s.status === 'SELESAI' ? 'ok' : 'stop'}">${s.status === 'SELESAI' ? '✅ Selesai Penuh' : '⏹️ Dihentikan Manual'}</span>
      </div>

      <div class="hist-detail-grid">
        <div class="hist-box">
          <div class="hist-box-title">🕒 Waktu Pengambilan</div>
          <dl>
            <dt>Tanggal</dt><dd>${esc(p.dateLabel)}</dd>
            <dt>Mulai</dt><dd class="font-mono">${p.time} WIB</dd>
            <dt>Selesai</dt><dd class="font-mono">${pe.time} WIB</dd>
            <dt>Durasi</dt><dd class="font-mono">${fmtDuration(s.duration_sec)} / ${fmtDuration(s.target_duration_sec || 300)}</dd>
            <dt>Jumlah Titik</dt><dd class="font-mono">${s.record_count || 0} data</dd>
          </dl>
        </div>
        <div class="hist-box">
          <div class="hist-box-title">☕ Sampel</div>
          <dl>
            <dt>Nama</dt><dd>${esc(s.coffee_name)}</dd>
            <dt>Asal</dt><dd>${esc(s.origin)}</dd>
            <dt>Roast</dt><dd>${esc(s.roast)}</dd>
            <dt>Grup / Device</dt><dd>${esc(s.group_id)} • ${esc(s.device_id)}</dd>
            <dt>Catatan</dt><dd>${esc(s.operator_notes || '–')}</dd>
          </dl>
        </div>
        <div class="hist-box">
          <div class="hist-box-title">📡 Koneksi & Perangkat</div>
          <dl>
            <dt>Mode</dt><dd>${esc(c.mode || '–')}</dd>
            <dt>Port</dt><dd class="font-mono">${esc(c.port || '–')}</dd>
            <dt>Firmware</dt><dd class="font-mono">v${esc(String(c.firmware_version || '–').replace(/^v/i, ''))}</dd>
            <dt>Wi-Fi</dt><dd>${esc(c.wifi_ssid || '–')} ${c.wifi_ip ? `<span class="hist-muted font-mono">(${esc(c.wifi_ip)})</span>` : ''}</dd>
            <dt>Disimpan dari</dt><dd>${esc(s.saved_from || '–')}</dd>
          </dl>
        </div>
        <div class="hist-box">
          <div class="hist-box-title">🤖 Hasil TinyML</div>
          ${s.tinyml ? `<div class="hist-tiny-label">${esc(s.tinyml.label)}</div>
            <div class="hist-muted">Confidence <strong>${Math.round((s.tinyml.confidence || 0) * 100)}%</strong>${s.tinyml.latency_us ? ` • ${s.tinyml.latency_us} µs` : ''}</div>`
          : '<div class="hist-muted">Model TinyML belum aktif saat sesi ini.</div>'}
        </div>
      </div>

      <div class="hist-box">
        <div class="hist-box-title">🌡️ Kondisi Lingkungan Chamber (DHT22)</div>
        <table class="hist-mini-table">
          <thead><tr><th>Parameter</th><th>Awal</th><th>Akhir</th><th>Min</th><th>Max</th><th>Rata-rata</th></tr></thead>
          <tbody>
            ${envRow('DHT #1 Suhu', env.dht1_temp, '°C')}
            ${envRow('DHT #1 Kelembaban', env.dht1_hum, '%')}
            ${envRow('DHT #2 Suhu', env.dht2_temp, '°C')}
            ${envRow('DHT #2 Kelembaban', env.dht2_hum, '%')}
          </tbody>
        </table>
      </div>

      <div class="hist-box">
        <div class="hist-box-title">🧪 Ringkasan 8 Sensor Gas (mV)</div>
        <table class="hist-mini-table">
          <thead><tr><th>Sensor</th><th>Awal</th><th>Akhir</th><th>Min</th><th>Max</th><th>Rata-rata</th><th>Δ Respon</th></tr></thead>
          <tbody>
            ${sensors.map((x) => `<tr>
              <td><span class="hist-dot" style="background:${esc(x.color)}"></span>${esc(x.name)} <span class="hist-muted">${esc(x.gas_type || '')}</span></td>
              <td>${num(x.start)}</td><td>${num(x.end)}</td><td>${num(x.min)}</td><td>${num(x.max)}</td>
              <td><strong>${num(x.avg)}</strong></td>
              <td class="${(x.delta || 0) >= 0 ? 'hist-up' : 'hist-down'}">${x.delta === null || x.delta === undefined ? '–' : ((x.delta >= 0 ? '+' : '') + num(x.delta))}</td>
            </tr>`).join('')}
          </tbody>
        </table>
      </div>

      <!-- Box Visualisasi Grafik Interaktif Histori (Line, Bar, Radar, DHT) -->
      <div class="hist-box hist-chart-box">
        <div class="hist-chart-header">
          <div class="hist-chart-header-left">
            <div class="hist-box-title" style="margin-bottom:2px;">📈 Visualisasi Grafik Sensor & Sidik Jari Aroma</div>
            <div class="hist-chart-subtitle hist-muted" id="histChartSubtitle">Memuat kurva respon sensor...</div>
          </div>
          <div class="hist-chart-tabs" id="histChartTabs">
            <button type="button" class="hist-chart-tab ${hist.detailChartMode === 'line' ? 'active' : ''}" data-mode="line">📈 Kurva Respon</button>
            <button type="button" class="hist-chart-tab ${hist.detailChartMode === 'bar' ? 'active' : ''}" data-mode="bar">📊 Profil Batang</button>
            <button type="button" class="hist-chart-tab ${hist.detailChartMode === 'radar' ? 'active' : ''}" data-mode="radar">🕸️ Radar Sidik Jari</button>
            <button type="button" class="hist-chart-tab ${hist.detailChartMode === 'dht' ? 'active' : ''}" data-mode="dht">🌡️ Suhu & Kelembaban</button>
          </div>
        </div>

        <div class="hist-chart-metrics" id="histChartMetrics">
          <div class="hist-chart-metric-badge highlight">
            <span>🚀 Puncak Tertinggi:</span>
            <strong>${esc(norm.peakSensor.name)} (${num(norm.peakSensor.max, 0)} mV)</strong>
          </div>
          <div class="hist-chart-metric-badge">
            <span>⚡ Δ Respon Terbesar:</span>
            <strong class="${(norm.maxDeltaSensor.delta || 0) >= 0 ? 'hist-up' : 'hist-down'}">${esc(norm.maxDeltaSensor.name)} (${(norm.maxDeltaSensor.delta >= 0 ? '+' : '') + num(norm.maxDeltaSensor.delta, 0)} mV)</strong>
          </div>
          <div class="hist-chart-metric-badge">
            <span>⏱️ Total Durasi:</span>
            <strong>${fmtDuration(norm.durationSec)} (${norm.totalPoints} titik)</strong>
          </div>
          <div class="hist-chart-metric-badge">
            <span>📦 Sumber Data:</span>
            <strong>${norm.hasTimeSeries ? 'Time-Series Penuh' : 'Profil Ringkasan Fitur'}</strong>
          </div>
        </div>

        <div class="hist-chart-legend" id="histChartLegend">
          ${norm.meta.map((m, idx) => `
            <button type="button" class="hist-legend-chip" data-sensor-idx="${idx}" title="Klik untuk sembunyikan/tampilkan ${esc(m.name)}">
              <span class="chip-dot" style="background:${esc(m.color)}; color:${esc(m.color)};"></span>
              <span>${esc(m.name)}</span>
            </button>
          `).join('')}
          <button type="button" class="hist-legend-reset" id="histLegendReset" title="Tampilkan semua sensor">Tampilkan Semua</button>
        </div>

        <div class="hist-chart-wrap">
          <canvas id="historyDetailChart"></canvas>
        </div>
      </div>

      <div class="hist-detail-actions">
        <button class="btn btn-outline" data-act="csv" data-id="${esc(s.id)}">⬇️ Unduh CSV Time-Series</button>
        <button class="btn btn-outline" data-act="json" data-id="${esc(s.id)}">⬇️ Unduh JSON Lengkap</button>
        <button class="btn btn-outline hist-btn-danger" data-act="delete" data-id="${esc(s.id)}">🗑️ Hapus Sesi</button>
      </div>`;

    // Pasang event listener untuk tabs mode grafik
    content.querySelectorAll('.hist-chart-tab').forEach((tab) => {
      tab.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const mode = tab.dataset.mode;
        renderHistoryDetailChart(s, mode);
      });
    });

    // Pasang event listener untuk chips sensor
    content.querySelectorAll('.hist-legend-chip').forEach((chip) => {
      chip.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const idx = Number(chip.dataset.sensorIdx);
        if (hist.hiddenSensors.has(idx)) {
          hist.hiddenSensors.delete(idx);
        } else {
          if (hist.hiddenSensors.size >= norm.meta.length - 1) {
            hist.hiddenSensors.clear();
          } else {
            hist.hiddenSensors.add(idx);
          }
        }
        renderHistoryDetailChart(s, hist.detailChartMode);
      });
    });

    const resetBtn = content.querySelector('#histLegendReset');
    if (resetBtn) {
      resetBtn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        hist.hiddenSensors.clear();
        renderHistoryDetailChart(s, hist.detailChartMode);
      });
    }

    // Beri jeda kecil agar layout browser menghitung dimensi kontainer canvas
    setTimeout(() => {
      renderHistoryDetailChart(s, hist.detailChartMode);
    }, 40);
    } catch (err) {
      console.error('[History] openDetail error:', err);
      content.innerHTML = `<div class="hist-loading" style="color:#f87171;">⚠️ Gagal memuat detail sesi: ${esc(err && err.message ? err.message : err)}</div>`;
    }
  }

  function closeDetail() {
    const modal = $('historyDetailModal');
    if (modal) modal.style.display = 'none';
    if (hist.detailChart) {
      hist.detailChart.destroy();
      hist.detailChart = null;
    }
    hist.activeSession = null;
    hist.hiddenSensors.clear();
  }

  // --------------------------------------------------------------------------
  // EXPORT & DELETE
  // --------------------------------------------------------------------------
  function download(filename, content, mime) {
    const blob = new Blob([content], { type: mime });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 500);
  }

  function safeName(s) {
    const p = wibParts(s.started_at_ms);
    return `${(s.coffee_name || 'sampel').replace(/[^\w-]+/g, '_')}_${s.coffee_id}_${s.measurement_id}_${p.dateKey}_${p.time.replace(/:/g, '')}`;
  }

  async function exportCsv(id) {
    const s = await getFullSession(id);
    if (!s) return;
    const norm = normalizeSessionData(s);
    if (!norm.timeSeries.length) { toast('Data time-series sesi ini tidak tersedia.', 'warning'); return; }
    const head = ['timestamp_wib', ...RECORD_COLUMNS, 'coffee_name', 'coffee_id', 'measurement_id', 'roast', 'origin', 'session_id'];
    const lines = [head.join(',')];
    norm.timeSeries.forEach((pt, i) => {
      const ts = wibParts(s.started_at_ms + pt.t * 1000);
      const dht = norm.dhtSeries[i]?.dht || [null, null, null, null];
      const row = [pt.t, ...pt.vals, ...dht];
      lines.push([`${ts.dateKey} ${ts.time}`, ...row, `"${String(s.coffee_name).replace(/"/g, '""')}"`, s.coffee_id, s.measurement_id,
        `"${String(s.roast).replace(/"/g, '""')}"`, `"${String(s.origin).replace(/"/g, '""')}"`, s.id].join(','));
    });
    download(`${safeName(s)}.csv`, '\ufeff' + lines.join('\n'), 'text/csv;charset=utf-8');
  }

  async function exportJson(id) {
    const s = await getFullSession(id);
    if (!s) return;
    const clean = { ...s };
    Object.keys(clean).forEach((k) => { if (k.startsWith('_')) delete clean[k]; });
    delete clean.synced;
    download(`${safeName(s)}.json`, JSON.stringify(clean, null, 2), 'application/json');
  }

  function exportSummaryCsv() {
    const list = getFiltered();
    if (!list.length) { toast('Tidak ada sesi untuk diekspor.', 'warning'); return; }
    const sensorIds = SENSOR_FALLBACK.map((x) => x.id);
    const head = ['session_id', 'tanggal', 'mulai_wib', 'selesai_wib', 'durasi_detik', 'status', 'coffee_name', 'coffee_id', 'measurement_id',
      'roast', 'origin', 'mode_koneksi', 'firmware', 'dht1_temp_avg', 'dht1_hum_avg', 'dht2_temp_avg', 'dht2_hum_avg',
      ...sensorIds.map((x) => `${x}_avg`), ...sensorIds.map((x) => `${x}_max`), 'tinyml_label', 'tinyml_conf', 'catatan'];
    const q = (v) => `"${String(v ?? '').replace(/"/g, '""')}"`;
    const rows = list.map((s) => {
      const p = wibParts(s.started_at_ms), pe = wibParts(s.ended_at_ms || s.started_at_ms);
      const e = s.environment || {}, ss = s.sensors_summary || [];
      return [s.id, p.dateKey, p.time, pe.time, s.duration_sec, s.status, q(s.coffee_name), s.coffee_id, s.measurement_id,
        q(s.roast), q(s.origin), q(s.connection?.mode), s.connection?.firmware_version || '',
        e.dht1_temp?.avg ?? '', e.dht1_hum?.avg ?? '', e.dht2_temp?.avg ?? '', e.dht2_hum?.avg ?? '',
        ...sensorIds.map((_, i) => ss[i]?.avg ?? ''), ...sensorIds.map((_, i) => ss[i]?.max ?? ''),
        q(s.tinyml?.label || ''), s.tinyml ? s.tinyml.confidence : '', q(s.operator_notes)].join(',');
    });
    download(`history_enose_${wibParts(Date.now()).dateKey}.csv`, '\ufeff' + [head.join(','), ...rows].join('\n'), 'text/csv;charset=utf-8');
  }

  async function deleteSessionById(id) {
    const s = hist.sessions.find((x) => x.id === id) || loadLocal().find((x) => x.id === id);
    if (!s) return;
    const p = wibParts(s.started_at_ms);
    if (!confirm(`Hapus sesi "${s.coffee_name}" (${p.dateLabel} ${p.time} WIB) dari History?\nData di cache lokal dan Cosmos DB akan dihapus.`)) return;
    saveLocal(loadLocal().filter((x) => x.id !== id));
    if (s._cloud || s.synced) {
      const ok = await cloudDelete(id, s.coffee_id);
      if (!ok) toast('⚠️ Terhapus dari lokal, tetapi gagal menghapus dari Cosmos DB.', 'warning');
    }
    hist.sessions = hist.sessions.filter((x) => x.id !== id);
    closeDetail();
    renderList();
    refreshTabBadge();
    toast('🗑️ Sesi dihapus dari History.', 'info');
  }

  // --------------------------------------------------------------------------
  // TAB SWITCHING (Pengambilan Data <-> History)
  // --------------------------------------------------------------------------
  function switchView(view) {
    hist.activeView = view === 'history' ? 'history' : 'acquisition';
    document.body.classList.toggle('view-history', hist.activeView === 'history');
    document.querySelectorAll('.view-tab-btn').forEach((b) => b.classList.toggle('active', b.dataset.view === hist.activeView));
    try { localStorage.setItem('enose_active_view', hist.activeView); } catch (_) {}
    if (hist.activeView === 'history') {
      loadHistory();
      window.scrollTo({ top: 0, behavior: 'smooth' });
    } else {
      // Pastikan grafik di tab pengambilan data menyesuaikan ukuran setelah ditampilkan kembali
      setTimeout(() => window.dispatchEvent(new Event('resize')), 50);
    }
  }

  function updateSamplingIndicator() {
    const ind = $('tabSamplingIndicator');
    if (!ind) return;
    const st = window.state || (typeof state !== 'undefined' ? state : null);
    const on = !!(st && st.isSampling);
    ind.style.display = on ? 'inline-flex' : 'none';
    if (on) ind.textContent = `● Sampling berjalan (${fmtDuration(st.samplingTimeLeft)})`;
  }

  // --------------------------------------------------------------------------
  // INIT
  // --------------------------------------------------------------------------
  function init() {
    document.querySelectorAll('.view-tab-btn').forEach((b) => b.addEventListener('click', () => switchView(b.dataset.view)));

    ['histSearch', 'histFilterRoast', 'histFilterStatus', 'histDateFrom', 'histDateTo'].forEach((id) => {
      const el = $(id);
      if (el) el.addEventListener(id === 'histSearch' ? 'input' : 'change', renderList);
    });
    $('histBtnReset')?.addEventListener('click', () => {
      ['histSearch', 'histFilterRoast', 'histFilterStatus', 'histDateFrom', 'histDateTo'].forEach((id) => { const el = $(id); if (el) el.value = ''; });
      renderList();
    });
    $('histBtnRefresh')?.addEventListener('click', loadHistory);
    $('histBtnSnapshot')?.addEventListener('click', () => {
      saveSamplingSessionToHistory(true);
    });
    $('histBtnSync')?.addEventListener('click', async () => { await syncPending(false); loadHistory(); });
    $('histBtnExportAll')?.addEventListener('click', exportSummaryCsv);
    $('historyDetailClose')?.addEventListener('click', closeDetail);
    $('historyDetailModal')?.addEventListener('click', (e) => { if (e.target.id === 'historyDetailModal') closeDetail(); });
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeDetail(); });

    const onAction = (e) => {
      const btn = e.target.closest('[data-act]');
      if (btn) {
        e.stopPropagation();
        const { act, id } = btn.dataset;
        if (act === 'detail') openDetail(id);
        else if (act === 'csv') exportCsv(id);
        else if (act === 'json') exportJson(id);
        else if (act === 'delete') deleteSessionById(id);
        return;
      }
      const row = e.target.closest('.hist-row');
      if (row) openDetail(row.dataset.id);
    };
    $('historyTableBody')?.addEventListener('click', onAction);
    $('historyDetailContent')?.addEventListener('click', onAction);

    hist.sessions = loadLocal();
    refreshTabBadge();
    setInterval(updateSamplingIndicator, 1000);

    let saved = 'acquisition';
    try { saved = localStorage.getItem('enose_active_view') || 'acquisition'; } catch (_) {}
    if (saved === 'history') switchView('history');

    // Sinkronkan sesi lokal yang tertunda di latar belakang
    setTimeout(() => syncPending(true).catch(() => {}), 4000);
  }

  // Ekspos API publik
  window.saveSamplingSessionToHistory = saveSamplingSessionToHistory;
  window.enoseHistory = { switchView, loadHistory, openDetail, exportCsv, exportJson, syncPending };

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
