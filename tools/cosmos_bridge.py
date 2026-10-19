"""
Azure Cosmos DB Bridge Service for Electronic Nose Dashboard
Menghubungkan Dashboard E-Nose Robusta langsung ke Azure Cosmos DB (Akun B).
"""

import sys
import json
import os
import datetime
from http.server import HTTPServer, BaseHTTPRequestHandler
from azure.cosmos import CosmosClient, exceptions

ENDPOINT = os.getenv("AZURE_COSMOS_ENDPOINT", "https://enose.documents.azure.com:443/")
KEY = os.getenv("AZURE_COSMOS_KEY", "")
DATABASE_NAME = os.getenv("AZURE_COSMOS_DATABASE", "enose_db")
CONTAINER_NAME = os.getenv("AZURE_COSMOS_CONTAINER", "telemetry")
PORT = 5005

cosmos_client = None
cosmos_container = None

def init_cosmos():
    global cosmos_client, cosmos_container
    try:
        cosmos_client = CosmosClient(ENDPOINT, credential=KEY)
        db = cosmos_client.get_database_client(DATABASE_NAME)
        cosmos_container = db.get_container_client(CONTAINER_NAME)
        # Test read
        cosmos_container.read()
        print(f"[COSMOS] Connected successfully to {ENDPOINT} ({DATABASE_NAME}/{CONTAINER_NAME})")
        return True
    except Exception as e:
        print(f"[COSMOS ERROR] Failed to connect: {e}", file=sys.stderr)
        return False

class CosmosBridgeHandler(BaseHTTPRequestHandler):
    def _set_cors(self, status=200):
        self.send_response(status)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        self.send_header("Content-Type", "application/json")
        self.end_headers()

    def do_OPTIONS(self):
        self._set_cors(200)

    def do_GET(self):
        if self.path == "/health" or self.path == "/api/cosmos/status":
            is_connected = cosmos_container is not None
            payload = {
                "connected": is_connected,
                "endpoint": ENDPOINT,
                "database": DATABASE_NAME,
                "container": CONTAINER_NAME,
                "service": "Electronic Nose Robusta Cosmos Bridge v1.0",
                "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat()
            }
            self._set_cors(200)
            self.wfile.write(json.dumps(payload).encode("utf-8"))
        elif self.path.startswith("/api/cosmos/samples"):
            try:
                if not cosmos_container:
                    init_cosmos()
                query = "SELECT TOP 10 * FROM c ORDER BY c._ts DESC"
                items = list(cosmos_container.query_items(query=query, enable_cross_partition_query=True))
                self._set_cors(200)
                self.wfile.write(json.dumps({"success": True, "count": len(items), "data": items}).encode("utf-8"))
            except Exception as e:
                self._set_cors(500)
                self.wfile.write(json.dumps({"success": False, "error": str(e)}).encode("utf-8"))
        else:
            self._set_cors(404)
            self.wfile.write(json.dumps({"error": "Not Found"}).encode("utf-8"))

    def _format_sample_doc(self, data):
        now_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
        ts_int = int(datetime.datetime.now().timestamp())
        coffee_id = data.get("coffee_id") or data.get("coffeeId") or "C05"
        doc_id = data.get("id") or f"{coffee_id}-{ts_int}"

        sensors_in = data.get("sensors") or {}
        dht22_in = data.get("dht22") or {}

        # Pastikan DHT22 terpetakan lengkap (Chamber & Ambien)
        dht1_temp = float(dht22_in.get("chamber_temp_c") or dht22_in.get("chamber_temp") or sensors_in.get("dht1_temp") or 0.0)
        dht1_hum  = float(dht22_in.get("chamber_hum_pct") or dht22_in.get("chamber_hum") or sensors_in.get("dht1_hum") or 0.0)
        dht2_temp = float(dht22_in.get("ambient_temp_c") or dht22_in.get("ambient_temp") or sensors_in.get("dht2_temp") or 0.0)
        dht2_hum  = float(dht22_in.get("ambient_hum_pct") or dht22_in.get("ambient_hum") or sensors_in.get("dht2_hum") or 0.0)

        sensors_payload = {
            # 8 Gas Sensors (Full Array ADS1 & ADS2)
            "mq6": float(sensors_in.get("mq6", 0.0)),
            "mq135": float(sensors_in.get("mq135", 0.0)),
            "mq7": float(sensors_in.get("mq7", 0.0)),
            "mq3": float(sensors_in.get("mq3", 0.0)),
            "tgs2602": float(sensors_in.get("tgs2602", 0.0)),
            "tgs2620": float(sensors_in.get("tgs2620", 0.0)),
            "tgs2611": float(sensors_in.get("tgs2611", 0.0)),
            "tgs2600": float(sensors_in.get("tgs2600", 0.0)),
            # Legacy Channel Aliases untuk kompatibilitas skema lama
            "ch1_tgs2602": float(sensors_in.get("ch1_tgs2602", sensors_in.get("tgs2602", 0.0))),
            "ch2_mq135": float(sensors_in.get("ch2_mq135", sensors_in.get("mq135", 0.0))),
            "ch3_mq3": float(sensors_in.get("ch3_mq3", sensors_in.get("mq3", 0.0))),
            "ch4_mq7": float(sensors_in.get("ch4_mq7", sensors_in.get("mq7", 0.0))),
            # DHT22 Dual-Probe Sensors
            "dht1_temp": dht1_temp,
            "dht1_hum": dht1_hum,
            "dht2_temp": dht2_temp,
            "dht2_hum": dht2_hum
        }

        records_count = len(data.get("records", [])) if "records" in data else (data.get("records_count") or data.get("recordCount") or 0)

        doc = {
            "id": doc_id,
            "coffee_id": coffee_id,
            "coffee_name": data.get("coffee_name") or data.get("coffeeName") or "Robusta Toraja",
            "measurement_id": data.get("measurement_id") or data.get("measurementId") or "M01",
            "origin": data.get("origin") or "Toraja - Sulawesi",
            "roast_level": data.get("roast_level") or data.get("roastLevel") or data.get("roast") or "Medium Roast",
            "notes": data.get("notes") or data.get("coffeeOperatorNotes") or data.get("operatorNotes") or "",
            "sensors": sensors_payload,
            "dht22": {
                "chamber_temperature_c": dht1_temp,
                "chamber_humidity_pct": dht1_hum,
                "ambient_temperature_c": dht2_temp,
                "ambient_humidity_pct": dht2_hum
            },
            "features": data.get("features") or {},
            "prediction": data.get("prediction") or "ROBUSTA (TORAJA)",
            "duration_seconds": data.get("duration_seconds") or 300,
            "records_count": records_count,
            "raw_points": data.get("rawPoints") or data.get("raw_points") or [],
            "created_at": data.get("created_at") or data.get("timestamp") or now_utc,
            "source": "E-Nose Web Dashboard"
        }
        return doc

    def do_POST(self):
        if self.path == "/api/cosmos/save_sample":
            try:
                content_len = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(content_len)
                data = json.loads(body.decode("utf-8"))

                if not cosmos_container:
                    init_cosmos()

                doc = self._format_sample_doc(data)
                created_item = cosmos_container.upsert_item(doc)
                print(f"[COSMOS SYNC] Saved sample: {doc['coffee_name']} [{doc['coffee_id']}] -> ID: {doc['id']}")

                self._set_cors(200)
                self.wfile.write(json.dumps({
                    "success": True,
                    "id": created_item.get("id"),
                    "coffee_id": created_item.get("coffee_id"),
                    "message": "Sample successfully synced to Azure Cosmos DB!",
                    "doc": doc
                }).encode("utf-8"))
            except Exception as e:
                print(f"[COSMOS ERROR] Failed to save: {e}", file=sys.stderr)
                self._set_cors(500)
                self.wfile.write(json.dumps({"success": False, "error": str(e)}).encode("utf-8"))

        elif self.path == "/api/cosmos/batch_sync":
            try:
                content_len = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(content_len)
                data = json.loads(body.decode("utf-8"))

                if not cosmos_container:
                    init_cosmos()

                samples = data.get("samples") or []
                synced_ids = []
                errors = []

                for s in samples:
                    try:
                        doc = self._format_sample_doc(s)
                        created_item = cosmos_container.upsert_item(doc)
                        synced_ids.append(created_item.get("id"))
                        print(f"[COSMOS BATCH] Synced: {doc['coffee_name']} [{doc['coffee_id']}] -> ID: {doc['id']}")
                    except Exception as item_err:
                        errors.append(f"Sample {s.get('id', 'unknown')}: {str(item_err)}")

                self._set_cors(200)
                self.wfile.write(json.dumps({
                    "success": len(errors) == 0,
                    "count": len(synced_ids),
                    "synced_ids": synced_ids,
                    "errors": errors,
                    "message": f"Berhasil sinkronisasi {len(synced_ids)}/{len(samples)} sampel ke Azure Cosmos DB!"
                }).encode("utf-8"))
            except Exception as e:
                print(f"[COSMOS BATCH ERROR] {e}", file=sys.stderr)
                self._set_cors(500)
                self.wfile.write(json.dumps({"success": False, "error": str(e)}).encode("utf-8"))
        else:
            self._set_cors(404)
            self.wfile.write(json.dumps({"error": "Not Found"}).encode("utf-8"))

def run_server():
    init_cosmos()
    server_address = ("127.0.0.1", PORT)
    httpd = HTTPServer(server_address, CosmosBridgeHandler)
    print(f"===========================================================")
    print(f" Electronic Nose Robusta - Azure Cosmos DB Bridge Service ")
    print(f" Listening on http://127.0.0.1:{PORT}")
    print(f" Connected to Azure: {ENDPOINT} ({DATABASE_NAME}/{CONTAINER_NAME})")
    print(f"===========================================================")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping Cosmos DB Bridge Server...")
        httpd.server_close()

if __name__ == "__main__":
    run_server()
