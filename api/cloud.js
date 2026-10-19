/**
 * Vercel Serverless Cloud Bridge for Electronic Nose Robusta Dashboard
 * Menghubungkan Dashboard di Vercel langsung ke:
 * 1. ESP32-S3 via HTTPS Direct REST Relay (Port /api/cloud)
 * 2. Azure Cosmos DB (enose_db / telemetry) via REST API (HMAC-SHA256 Master Key)
 * 3. ThingsBoard Cloud Relay (Token: cbq9l18oexktzzucit15)
 */

const http = require('http');
const https = require('https');
const crypto = require('crypto');

const TB_TOKEN = process.env.THINGSBOARD_TOKEN || 'cbq9l18oexktzzucit15';
const TB_IP = '54.208.96.163';
const TB_HOST = 'thingsboard.cloud';

const COSMOS_HOST = process.env.AZURE_COSMOS_HOST || 'enose.documents.azure.com';
const COSMOS_KEY = process.env.AZURE_COSMOS_KEY || '';
const COSMOS_DB = process.env.AZURE_COSMOS_DATABASE || 'enose_db';
const COSMOS_COLL = process.env.AZURE_COSMOS_CONTAINER || 'telemetry';

// Global In-Memory Cache (berjalan antar request saat serverless instance warm)
let memLiveTelemetry = null;
let memLastTelemetryMs = 0;
let memActiveCmd = 'IDLE';
let memActiveCmdSeq = 0;

function httpRequest(options, bodyStr = null) {
  return new Promise((resolve, reject) => {
    const lib = options.protocol === 'https:' ? https : http;
    const req = lib.request(options, (res) => {
      let chunks = [];
      res.on('data', (d) => chunks.push(d));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf-8');
        let json = null;
        try { json = JSON.parse(text); } catch (_) {}
        resolve({ status: res.statusCode, text, json });
      });
    });
    req.on('error', reject);
    req.setTimeout(6000, () => {
      req.destroy(new Error('Request timeout'));
    });
    if (bodyStr) req.write(bodyStr);
    req.end();
  });
}

function getCosmosAuthToken(verb, resourceType, resourceId, dateUtc) {
  const key = Buffer.from(COSMOS_KEY, 'base64');
  const text = `${verb.toLowerCase()}\n${resourceType.toLowerCase()}\n${resourceId}\n${dateUtc.toLowerCase()}\n\n`;
  const sig = crypto.createHmac('sha256', key).update(text, 'utf8').digest('base64');
  return encodeURIComponent(`type=master&ver=1.0&sig=${sig}`);
}

async function upsertCosmosDoc(doc) {
  // Format waktu khusus Waktu Indonesia Barat (WIB / UTC+7)
  const d = doc.created_at ? new Date(doc.created_at) : new Date();
  const validDate = isNaN(d.getTime()) ? new Date() : d;
  const wibTime = new Date(validDate.getTime() + (7 * 60 * 60 * 1000));
  const pad = (n) => String(n).padStart(2, '0');
  const wibIso = `${wibTime.getUTCFullYear()}-${pad(wibTime.getUTCMonth()+1)}-${pad(wibTime.getUTCDate())}T${pad(wibTime.getUTCHours())}:${pad(wibTime.getUTCMinutes())}:${pad(wibTime.getUTCSeconds())}.${String(wibTime.getUTCMilliseconds()).padStart(3,'0')}+07:00`;
  doc.created_at = wibIso;
  doc.created_at_wib = `${wibTime.getUTCFullYear()}-${pad(wibTime.getUTCMonth()+1)}-${pad(wibTime.getUTCDate())} ${pad(wibTime.getUTCHours())}:${pad(wibTime.getUTCMinutes())}:${pad(wibTime.getUTCSeconds())} WIB`;
  doc.timezone = 'Asia/Jakarta (WIB / UTC+7)';

  const dateUtc = new Date().toUTCString();
  const resourceId = `dbs/${COSMOS_DB}/colls/${COSMOS_COLL}`;
  const auth = getCosmosAuthToken('post', 'docs', resourceId, dateUtc);
  const bodyStr = JSON.stringify(doc);

  return httpRequest({
    protocol: 'https:',
    hostname: COSMOS_HOST,
    port: 443,
    path: `/${resourceId}/docs`,
    method: 'POST',
    headers: {
      'Authorization': auth,
      'x-ms-date': dateUtc,
      'x-ms-version': '2018-12-31',
      'x-ms-documentdb-is-upsert': 'true',
      'x-ms-documentdb-partitionkey': JSON.stringify([doc.coffee_id || 'C05']),
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(bodyStr)
    }
  }, bodyStr);
}

module.exports = async (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');

  if (req.method === 'OPTIONS') {
    return res.status(200).end();
  }

  try {
    if (req.method === 'GET') {
      // Cek apakah request khusus untuk sinkronisasi OTA ThingsBoard
      if (req.url && (req.url.includes('action=ota_status') || req.url.includes('action=ota_sync'))) {
        const tbRes = await httpRequest({
          protocol: 'https:',
          hostname: TB_HOST,
          port: 443,
          path: `/api/v1/${TB_TOKEN}/attributes?sharedKeys=fw_title,fw_version,fw_size,fw_tag&clientKeys=cloud_cmd,cmd_seq,ota_trigger`,
          method: 'GET'
        }).catch(() => null);

        return res.status(200).json({
          success: true,
          shared: tbRes?.json?.shared || {},
          client: tbRes?.json?.client || {},
          source: 'thingsboard_cloud'
        });
      }

      let liveTelemetry = null;
      let fwVer = '1.2.0';
      let isEspActive = false;
      let activeMode = 'standby';

      // 1. Cek telemetri live dari In-Memory Serverless Cache (paling cepat, update <= 12 detik)
      const nowMs = Date.now();
      if (memLiveTelemetry && (nowMs - memLastTelemetryMs <= 12000)) {
        isEspActive = true;
        liveTelemetry = memLiveTelemetry;
        fwVer = memLiveTelemetry.firmware_version || fwVer;
        activeMode = 'direct_vercel_relay';
      }

      // 2. Fallback ke Azure Cosmos DB (dokumen LIVE-ESP32-RELAY)
      if (!isEspActive && COSMOS_KEY) {
        const dateUtc = new Date().toUTCString();
        const resourceId = `dbs/${COSMOS_DB}/colls/${COSMOS_COLL}/docs/LIVE-ESP32-RELAY`;
        const auth = getCosmosAuthToken('get', 'docs', resourceId, dateUtc);

        const cosmosRes = await httpRequest({
          protocol: 'https:',
          hostname: COSMOS_HOST,
          port: 443,
          path: `/${resourceId}`,
          method: 'GET',
          headers: {
            'Authorization': auth,
            'x-ms-date': dateUtc,
            'x-ms-version': '2018-12-31',
            'x-ms-documentdb-partitionkey': JSON.stringify(['RELAY'])
          }
        }).catch(() => ({ status: 404, json: null }));

        const relayDoc = (cosmosRes && cosmosRes.status === 200 && cosmosRes.json) ? cosmosRes.json : null;
        const nowSec = Math.floor(Date.now() / 1000);
        const docTsSec = relayDoc?._ts || 0;
        if (docTsSec > 0 && (nowSec - docTsSec <= 15)) {
          isEspActive = true;
          liveTelemetry = relayDoc.live_telemetry || liveTelemetry;
          fwVer = relayDoc.fw_version || fwVer;
          activeMode = 'azure_cosmos_relay';
        }
      }

      // 3. Fallback ke ThingsBoard Cloud
      if (!isEspActive) {
        const tbRes = await httpRequest({
          protocol: 'https:',
          hostname: TB_HOST,
          port: 443,
          path: `/api/v1/${TB_TOKEN}/attributes?clientKeys=live_telemetry,status,fw_version`,
          method: 'GET'
        }).catch(() => null);

        let clientAttrs = tbRes?.json?.client || {};
        if (clientAttrs.live_telemetry && clientAttrs.live_telemetry.sensors) {
          isEspActive = true;
          liveTelemetry = clientAttrs.live_telemetry;
          fwVer = clientAttrs.fw_version || fwVer;
          activeMode = 'thingsboard_cloud_relay';
        }
      }

      return res.status(200).json({
        success: true,
        connected: true,
        esp_connected: isEspActive,
        mode: activeMode,
        live_telemetry: liveTelemetry,
        fw_version: fwVer,
        status: isEspActive ? 'ONLINE' : 'OFFLINE',
        timestamp: new Date().toISOString()
      });
    }

    if (req.method === 'POST') {
      const body = typeof req.body === 'string' ? JSON.parse(req.body) : (req.body || {});
      const action = body.action || (body.sensors ? 'live_telemetry' : 'save_sample');

      // 0. Penerimaan Live Telemetry Langsung dari ESP32 (Jalur Utama Vercel HTTPS)
      if (action === 'live_telemetry' || action === 'telemetry') {
        const sensors = body.sensors || {};
        const st = body.status || {};
        const fwVer = body.fw_version || st.firmware_version || '1.2.0';

        memLiveTelemetry = {
          sensors: sensors,
          status: st,
          firmware_version: fwVer,
          timestamp: new Date().toISOString()
        };
        memLastTelemetryMs = Date.now();

        // Simpan snapshot status LIVE-ESP32-RELAY ke Azure Cosmos DB
        if (COSMOS_KEY) {
          upsertCosmosDoc({
            id: 'LIVE-ESP32-RELAY',
            coffee_id: 'RELAY',
            live_telemetry: memLiveTelemetry,
            fw_version: fwVer,
            status: 'ONLINE',
            updated_at: new Date().toISOString()
          }).catch(() => null);
        }

        // Kembalikan perintah terbaru ke ESP32 (jika user menekan tombol START/STOP di Web)
        return res.status(200).json({
          success: true,
          command: memActiveCmd,
          cmd_seq: memActiveCmdSeq,
          received_at: new Date().toISOString()
        });
      }

      // 1. Gerbang Utama OTA Trigger ke ThingsBoard Cloud
      if (action === 'ota_trigger') {
        const cmdSeq = Math.floor(Date.now() / 1000) % 2000000000;
        const tbPayload = JSON.stringify({
          cloud_cmd: 'OTA',
          cmd_seq: cmdSeq,
          ota_trigger: true
        });

        const tbRes = await httpRequest({
          protocol: 'https:',
          hostname: TB_HOST,
          port: 443,
          path: `/api/v1/${TB_TOKEN}/attributes`,
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(tbPayload)
          }
        }, tbPayload).catch((err) => ({ status: 500, error: err.message }));

        return res.status(200).json({
          success: tbRes?.status >= 200 && tbRes?.status < 300,
          cmd_seq: cmdSeq,
          message: '⚡ Perintah OTA Trigger berhasil disiarkan ke ThingsBoard Cloud!'
        });
      }

      // 1b. Reset OTA status ke IDLE di ThingsBoard Cloud
      if (action === 'ota_reset') {
        const tbPayload = JSON.stringify({
          cloud_cmd: 'IDLE',
          ota_trigger: false
        });
        await httpRequest({
          protocol: 'https:',
          hostname: TB_HOST,
          port: 443,
          path: `/api/v1/${TB_TOKEN}/attributes`,
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(tbPayload)
          }
        }, tbPayload).catch(() => null);

        return res.status(200).json({
          success: true,
          message: 'Status OTA di-reset ke IDLE'
        });
      }

      // 2. Kirim Command (START, STOP, PURGE) ke ESP32 via Azure & Memory Relay
      if (action === 'command') {
        const cmd = (body.command || 'START').toUpperCase();
        const seq = Math.floor(Date.now() / 1000) % 2000000000;

        memActiveCmd = cmd;
        memActiveCmdSeq = seq;

        if (COSMOS_KEY) {
          await upsertCosmosDoc({
            id: 'LIVE-ESP32-CMD',
            coffee_id: 'RELAY',
            cloud_cmd: cmd,
            cmd_seq: seq,
            updated_at: new Date().toISOString()
          }).catch(() => null);
        }

        // Juga siarkan ke ThingsBoard Cloud
        const tbPayload = JSON.stringify({
          cloud_cmd: cmd,
          cmd_seq: seq
        });
        httpRequest({
          protocol: 'https:',
          hostname: TB_HOST,
          port: 443,
          path: `/api/v1/${TB_TOKEN}/attributes`,
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(tbPayload)
          }
        }, tbPayload).catch(() => null);

        return res.status(200).json({
          success: true,
          command: cmd,
          cmd_seq: seq,
          message: `Perintah '${cmd}' dikirim ke ESP32 & Azure Cloud!`
        });
      }

      // 3. Batch Sync ke Azure Cosmos DB
      if (action === 'batch_sync' && Array.isArray(body.samples)) {
        const synced = [];
        for (const s of body.samples) {
          const r = await upsertCosmosDoc(s);
          if (r.status >= 200 && r.status < 300) {
            synced.push(s.id);
          }
        }
        return res.status(200).json({
          success: true,
          count: synced.length,
          synced_ids: synced
        });
      }

      // 4. Simpan 1 Sampel ke Azure Cosmos DB
      const doc = body.sample || body;
      if (!doc.id) doc.id = `SMPL-${Date.now().toString(36).toUpperCase()}`;
      if (!doc.coffee_id) doc.coffee_id = 'C05';
      if (!doc.created_at) doc.created_at = new Date().toISOString();
      if (!doc.source) doc.source = 'E-Nose Web Dashboard (Cloud)';

      const cosmosRes = await upsertCosmosDoc(doc);
      return res.status(200).json({
        success: cosmosRes.status >= 200 && cosmosRes.status < 300,
        id: doc.id,
        coffee_id: doc.coffee_id,
        cosmos_status: cosmosRes.status
      });
    }

    return res.status(405).json({ error: 'Method Not Allowed' });
  } catch (err) {
    return res.status(500).json({ success: false, error: err.message });
  }
};
