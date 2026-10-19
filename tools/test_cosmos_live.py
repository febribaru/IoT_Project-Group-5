import os
import datetime
import json
from azure.cosmos import CosmosClient, PartitionKey, exceptions

ENDPOINT = os.getenv("AZURE_COSMOS_ENDPOINT", "https://enose.documents.azure.com:443/")
KEY = os.getenv("AZURE_COSMOS_KEY", "")
DATABASE_NAME = os.getenv("AZURE_COSMOS_DATABASE", "enose_db")
CONTAINER_NAME = os.getenv("AZURE_COSMOS_CONTAINER", "telemetry")

def test_cosmos():
    print(f"Connecting to Cosmos DB at {ENDPOINT}...")
    try:
        client = CosmosClient(ENDPOINT, credential=KEY)
        
        # Test Database
        db = client.get_database_client(DATABASE_NAME)
        print(f"[OK] Database '{DATABASE_NAME}' found!")
        
        # Test Container
        container = db.get_container_client(CONTAINER_NAME)
        props = container.read()
        print(f"[OK] Container '{CONTAINER_NAME}' found! Partition key: {props.get('partitionKey')}")
        
        # Insert a realistic Robusta E-Nose test document
        test_item = {
            "id": f"test-sample-{int(datetime.datetime.now().timestamp())}",
            "device_id": "enose-robusta-device-01",
            "coffee_name": "Robusta Dampit",
            "coffee_id": "C05",
            "measurement_id": "M01",
            "origin": "Dampit - Malang",
            "roast_level": "Medium Roast",
            "sensors": {
                "ch1_tgs2602_mv": 1450.2,
                "ch2_mq135_mv": 2180.5,
                "ch3_mq3_mv": 890.1,
                "ch4_mq7_mv": 420.7
            },
            "temperature_c": 28.5,
            "humidity_pct": 62.0,
            "tinyml_classification": "Fine Robusta (Grade 1)",
            "tinyml_confidence": 0.94,
            "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "notes": "Pengujian Koneksi Langsung E-Nose Robusta ke Azure Cosmos DB"
        }
        
        created_item = container.upsert_item(test_item)
        print(f"[SUCCESS] Test document inserted successfully!")
        print(f"Document ID: {created_item.get('id')}")
        print(f"Partition: {created_item.get('coffee_id')}")
        
        return True
    except exceptions.CosmosHttpResponseError as e:
        print(f"[ERROR] Cosmos HTTP Error: {e.status_code} - {e.message}")
        return False
    except Exception as e:
        print(f"[ERROR] General Error: {str(e)}")
        return False

if __name__ == "__main__":
    test_cosmos()
