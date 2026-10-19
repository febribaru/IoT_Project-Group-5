/**
 * Vercel Serverless: HISTORY SESI PENGAMBILAN DATA (Azure Cosmos DB)
 * ---------------------------------------------------------------------------
 * Menyimpan & membaca riwayat sesi sampling E-Nose (doc_type = 'sampling_session')
 * di container yang sama dengan telemetri (enose_db / telemetry, partition key /coffee_id).
 *
 *   GET    /api/history                 -> daftar ringkasan semua sesi (tanpa time-series)
 *   GET    /api/history?id=..&pk=..     -> detail 1 sesi lengkap (termasuk records)
 *   POST   /api/history  {session}      -> simpan / perbarui 1 sesi (upsert)
 *   DELETE /api/history?id=..&pk=..     -> hapus 1 sesi
 */

const https = require('https');
const crypto = require('crypto');

const COSMOS_HOST = process.env.AZURE_COSMOS_HOST || 'enose.documents.azure.com';
const COSMOS_KEY = process.env.AZURE_COSMOS_KEY || '';
const COSMOS_DB = process.env.AZURE_COSMOS_DATABASE || 'enose_db';
const COSMOS_COLL = process.env.AZURE_COSMOS_CONTAINER || 'telemetry';
const COLL_RID = `dbs/${COSMOS_DB}/colls/${COSMOS_COLL}`;

const DOC_TYPE = 'sampling_session';
const MAX_PAGES = 20;

function cosmosRequest({ method, path, headers = {}, body = null }) {
  return new Promise((resolve, reject) => {
    const bodyStr = body === null ? null : (typeof body === 'string' ? body : JSON.stringify(body));
    const req = https.request({
      protocol: 'https:',
      hostname: COSMOS_HOST,
      port: 443,
      path,
      method,
      headers: {
        ...headers,
        ...(bodyStr !== null ? { 'Content-Length': Buffer.byteLength(bodyStr) } : {})
      }
    }, (res) => {
      const chunks = [];
      res.on('data', (d) => chunks.push(d));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf-8');
        let json = null;
        try { json = JSON.parse(text); } catch (_) {}
        resolve({ status: res.statusCode, headers: res.headers, text, json });
      });
    });
    req.on('error', reject);
    req.setTimeout(9000, () => req.destroy(new Error('Cosmos DB request timeout')));
    if (bodyStr !== null) req.write(bodyStr);
    req.end();
  });
}

function authHeaders(verb, resourceType, resourceId) {
  const dateUtc = new Date().toUTCString();
  const key = Buffer.from(COSMOS_KEY, 'base64');
  const text = `${verb.toLowerCase()}\n${resourceType.toLowerCase()}\n${resourceId}\n${dateUtc.toLowerCase()}\n\n`;
  const sig = crypto.createHmac('sha256', key).update(text, 'utf8').digest('base64');
  return {
    'Authorization': encodeURIComponent(`type=master&ver=1.0&sig=${sig}`),
    'x-ms-date': dateUtc,
    'x-ms-version': '2018-12-31'
  };
}

/** Ambil ringkasan semua sesi (cross-partition, tanpa ORDER BY agar didukung gateway REST). */
async function listSessions() {
  const query = {
    query: `SELECT c.id, c.coffee_id, c.coffee_name, c.measurement_id, c.origin, c.roast,
                   c.operator_notes, c.group_id, c.device_id, c.status, c.started_at, c.started_at_ms,
                   c.ended_at, c.ended_at_ms, c.duration_sec, c.target_duration_sec, c.record_count,
                   c.connection, c.environment, c.sensors_summary, c.tinyml, c.saved_from
            FROM c WHERE c.doc_type = @type`,
    parameters: [{ name: '@type', value: DOC_TYPE }]
  };

  const sessions = [];
  let continuation = null;
  for (let page = 0; page < MAX_PAGES; page++) {
    const headers = {
      ...authHeaders('post', 'docs', COLL_RID),
      'Content-Type': 'application/query+json',
      'x-ms-documentdb-isquery': 'True',
      'x-ms-documentdb-query-enablecrosspartition': 'True',
      'x-ms-max-item-count': '200'
    };
    if (continuation) headers['x-ms-continuation'] = continuation;

    const r = await cosmosRequest({ method: 'POST', path: `/${COLL_RID}/docs`, headers, body: query });
    if (r.status < 200 || r.status >= 300) {
      throw new Error(`Cosmos query HTTP ${r.status}: ${(r.json && r.json.message) || r.text.slice(0, 200)}`);
    }
    (r.json && r.json.Documents ? r.json.Documents : []).forEach((d) => sessions.push(d));
    continuation = r.headers['x-ms-continuation'];
    if (!continuation) break;
  }

  sessions.sort((a, b) => (b.started_at_ms || 0) - (a.started_at_ms || 0));
  return sessions;
}

async function getSession(id, pk) {
  const rid = `${COLL_RID}/docs/${id}`;
  const r = await cosmosRequest({
    method: 'GET',
    path: `/${COLL_RID}/docs/${encodeURIComponent(id)}`,
    headers: { ...authHeaders('get', 'docs', rid), 'x-ms-documentdb-partitionkey': JSON.stringify([pk]) }
  });
  return r;
}

async function upsertSession(session) {
  const doc = { ...session, doc_type: DOC_TYPE };
  if (!doc.id) doc.id = `HIST-${Date.now().toString(36).toUpperCase()}`;
  if (!doc.coffee_id) doc.coffee_id = 'C05';
  doc.cloud_saved_at = new Date().toISOString();
  delete doc.synced;      // flag khusus cache browser
  delete doc._local_only;

  return cosmosRequest({
    method: 'POST',
    path: `/${COLL_RID}/docs`,
    headers: {
      ...authHeaders('post', 'docs', COLL_RID),
      'x-ms-documentdb-is-upsert': 'true',
      'x-ms-documentdb-partitionkey': JSON.stringify([doc.coffee_id]),
      'Content-Type': 'application/json'
    },
    body: doc
  });
}

async function deleteSession(id, pk) {
  const rid = `${COLL_RID}/docs/${id}`;
  return cosmosRequest({
    method: 'DELETE',
    path: `/${COLL_RID}/docs/${encodeURIComponent(id)}`,
    headers: { ...authHeaders('delete', 'docs', rid), 'x-ms-documentdb-partitionkey': JSON.stringify([pk]) }
  });
}

module.exports = async (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, DELETE, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');
  res.setHeader('Cache-Control', 'no-store');

  if (req.method === 'OPTIONS') return res.status(200).end();

  try {
    const q = req.query || {};

    if (req.method === 'GET') {
      if (q.id) {
        const r = await getSession(String(q.id), String(q.pk || 'C05'));
        if (r.status === 200 && r.json) return res.status(200).json({ success: true, session: r.json });
        return res.status(r.status === 404 ? 404 : 502).json({ success: false, error: `Cosmos HTTP ${r.status}` });
      }
      const sessions = await listSessions();
      return res.status(200).json({ success: true, storage: 'azure_cosmos_db', count: sessions.length, sessions });
    }

    if (req.method === 'POST') {
      const body = typeof req.body === 'string' ? JSON.parse(req.body) : (req.body || {});
      const session = body.session || body;
      if (!session || typeof session !== 'object') {
        return res.status(400).json({ success: false, error: 'Payload session tidak valid' });
      }
      const r = await upsertSession(session);
      const ok = r.status >= 200 && r.status < 300;
      return res.status(ok ? 200 : 502).json({
        success: ok,
        id: session.id,
        cosmos_status: r.status,
        error: ok ? undefined : ((r.json && r.json.message) || r.text.slice(0, 200))
      });
    }

    if (req.method === 'DELETE') {
      if (!q.id) return res.status(400).json({ success: false, error: 'Parameter id wajib diisi' });
      const r = await deleteSession(String(q.id), String(q.pk || 'C05'));
      const ok = (r.status >= 200 && r.status < 300) || r.status === 404;
      return res.status(ok ? 200 : 502).json({ success: ok, cosmos_status: r.status });
    }

    return res.status(405).json({ success: false, error: 'Method Not Allowed' });
  } catch (err) {
    return res.status(500).json({ success: false, error: err.message });
  }
};

