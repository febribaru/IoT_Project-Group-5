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

def _load_dotenv():
    env_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".env")
    if os.path.exists(env_path):
        with open(env_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    os.environ.setdefault(k.strip(), v.strip())

_load_dotenv()
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

    def do_POST(self):
        if self.path == "/api/cosmos/save_sample":
            try:
                content_len = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(content_len)
                data = json.loads(body.decode("utf-8"))

                if not cosmos_container:
                    init_cosmos()

                # Generate clean document
                now_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
                ts_int = int(datetime.datetime.now().timestamp())
                coffee_id = data.get("coffee_id") or data.get("coffeeId") or "C05"
                doc_id = data.get("id") or f"{coffee_id}-{ts_int}"

                doc = {
                    "id": doc_id,
                    "coffee_id": coffee_id,
                    "coffee_name": data.get("coffee_name") or data.get("coffeeName") or "Robusta Dampit",
                    "measurement_id": data.get("measurement_id") or data.get("measurementId") or "M01",
                    "origin": data.get("origin") or "Dampit - Malang",
                    "roast_level": data.get("roast_level") or data.get("roastLevel") or "Medium Roast",
                    "notes": data.get("notes") or data.get("coffeeOperatorNotes") or "",
                    "sensors": data.get("sensors") or {},
                    "prediction": data.get("prediction") or "FINE ROBUSTA",
                    "duration_seconds": data.get("duration_seconds") or 300,
                    "records_count": len(data.get("records", [])) if "records" in data else 0,
                    "created_at": now_utc,
                    "source": "E-Nose Web Dashboard"
                }

                created_item = cosmos_container.upsert_item(doc)
                print(f"[COSMOS SYNC] Saved sample: {doc['coffee_name']} [{doc['coffee_id']}] -> ID: {doc['id']}")

                self._set_cors(200)
                self.wfile.write(json.dumps({
                    "success": True,
                    "id": created_item.get("id"),
                    "coffee_id": created_item.get("coffee_id"),
                    "message": "Sample successfully synced to Azure Cosmos DB!"
                }).encode("utf-8"))
            except Exception as e:
                print(f"[COSMOS ERROR] Failed to save: {e}", file=sys.stderr)
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
