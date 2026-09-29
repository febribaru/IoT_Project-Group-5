//! MQTT Client Module (rumqttc)
//! Mendengarkan data telemetri nirkabel dari mikrokontroler ESP32 atau broker lokal.

use std::time::Duration;
use tokio::sync::broadcast;
use rumqttc::{AsyncClient, MqttOptions, QoS, Event, Packet};
use crate::config::AppConfig;
use crate::payload::{EspTelemetryMessage, SensorTelemetry, TelemetryPacket, classify_coffee_aroma};

pub async fn run_mqtt_worker(
    config: AppConfig,
    tx_telemetry: broadcast::Sender<TelemetryPacket>,
) {
    let broker_host = config.mqtt_broker_host;
    let broker_port = config.mqtt_broker_port;
    let topic = config.mqtt_topic_telemetry;

    tracing::info!("Menginisialisasi MQTT client ke {}:{} [Topic: {}]...", broker_host, broker_port, topic);

    let mut mqttoptions = MqttOptions::new("enose-backend-bridge", &broker_host, broker_port);
    mqttoptions.set_keep_alive(Duration::from_secs(15));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Err(e) = client.subscribe(&topic, QoS::AtLeastOnce).await {
            tracing::warn!("Belum dapat subscribe ke MQTT topic (broker offline): {:?}", e);
        }
    });

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                if let Ok(msg) = serde_json::from_slice::<EspTelemetryMessage>(&publish.payload) {
                    let (dht1_temp, dht1_hum, dht2_temp, dht2_hum) = msg.dht_values();
                    let s = msg.sensors;
                    let sensor_data = SensorTelemetry {
                        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        time_sec: (msg.status.elapsed_time % 300) as f64,
                        firmware_version: Some("1.0.6".to_string()),
                        active_firmware_version: Some("1.0.6".to_string()),
                        mq6: s.mq6 as f64,
                        mq135: s.mq135 as f64,
                        mq3: s.mq3 as f64,
                        mq7: s.mq7 as f64,
                        tgs2611: s.tgs2611 as f64,
                        tgs2602: s.tgs2602 as f64,
                        tgs2600: s.tgs2600 as f64,
                        tgs2620: s.tgs2620 as f64,
                        mq2: s.tgs2620 as f64,
                        dht1_temp: dht1_temp as f64,
                        dht1_hum: dht1_hum as f64,
                        dht2_temp: dht2_temp as f64,
                        dht2_hum: dht2_hum as f64,
                        chamber_temp: Some(dht1_temp as f64),
                        chamber_humidity: Some(dht1_hum as f64),
                        raw_esp: Some(s),
                        system_status: Some(msg.status),
                        cloud_payload: None,
                    };
                    let prediction = if let Some(tinyml) = &msg.tinyml {
                        crate::payload::prediction_from_tinyml(tinyml)
                    } else {
                        classify_coffee_aroma(&sensor_data)
                    };
                    let _ = tx_telemetry.send(TelemetryPacket {
                        sensor_data: Some(sensor_data),
                        prediction: Some(prediction),
                        tinyml: msg.tinyml,
                        ota_event: None,
                    });
                }
            }
            Ok(_) => {}
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

