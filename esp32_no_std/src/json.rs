//! JSON Telemetry Formatter (Hemat RAM Stack & Aman untuk Bare-Metal no_std)

use core::fmt::Write;
use esp_println::{print, println};
use crate::state::{SystemState, get_elapsed_time, get_inlet_time, get_outlet_time};
use crate::timing::millis;

/// Fungsi cetak float hemat RAM Stack & aman untuk bare-metal (2 angka di belakang koma)
pub fn print_sensor_float(val: f32) {
    let int_part = val as i32;
    let frac_int = (((val - int_part as f32) * 100.0).abs() + 0.5) as i32;
    let (display_int, display_frac) = if frac_int >= 100 {
        (int_part + 1, 0)
    } else {
        (int_part, frac_int)
    };
    print!("{}.{:02}", display_int, display_frac);
}

/// Tulis float sensor ke String buffer secara hemat memori
pub fn write_sensor_float<const N: usize>(buf: &mut heapless::String<N>, val: f32) {
    let int_part = val as i32;
    let frac_int = (((val - int_part as f32) * 100.0).abs() + 0.5) as i32;
    let (display_int, display_frac) = if frac_int >= 100 {
        (int_part + 1, 0)
    } else {
        (int_part, frac_int)
    };
    let _ = write!(buf, "{}.{:02}", display_int, display_frac);
}

/// Memformat paket telemetri JSON ke buffer string untuk transmisi nirkabel UDP / MQTT
pub fn format_json<const N: usize>(state: &SystemState, buf: &mut heapless::String<N>) {
    buf.clear();
    let s = &state.sensors;
    let _ = write!(buf, "{{\"sensors\":{{\"mq6\":");
    write_sensor_float(buf, s.mq6);
    let _ = write!(buf, ",\"mq8\":");
    write_sensor_float(buf, s.mq8);
    let _ = write!(buf, ",\"mq135\":");
    write_sensor_float(buf, s.mq135);
    let _ = write!(buf, ",\"mq3\":");
    write_sensor_float(buf, s.mq3);
    let _ = write!(buf, ",\"mq7\":");
    write_sensor_float(buf, s.mq7);
    let _ = write!(buf, ",\"tgs2611\":");
    write_sensor_float(buf, s.tgs2611);
    let _ = write!(buf, ",\"tgs2602\":");
    write_sensor_float(buf, s.tgs2602);
    let _ = write!(buf, ",\"tgs2600\":");
    write_sensor_float(buf, s.tgs2600);
    let _ = write!(buf, ",\"tgs2620\":");
    write_sensor_float(buf, s.tgs2620);
    let _ = write!(buf, ",\"dht1_temp\":");
    write_sensor_float(buf, s.dht1_temp);
    let _ = write!(buf, ",\"dht1_hum\":");
    write_sensor_float(buf, s.dht1_hum);
    let _ = write!(buf, ",\"dht2_temp\":");
    write_sensor_float(buf, s.dht2_temp);
    let _ = write!(buf, ",\"dht2_hum\":");
    write_sensor_float(buf, s.dht2_hum);
    let fw_ver = crate::iot::ota::CURRENT_VERSION;

    let _ = write!(
        buf,
        "}},\"status\":{{\"inlet_pump\":{},\"inlet_valve\":{},\"outlet_pump\":{},\"outlet_valve\":{},\"inlet_time\":{},\"outlet_time\":{},\"elapsed_time\":{},\"firmware_version\":\"{}\"}},\"firmware_version\":\"{}\",\"timestamp\":{}",
        state.driver1_active, state.driver1_active,
        state.driver2_active, state.driver2_active,
        get_inlet_time(state), get_outlet_time(state), get_elapsed_time(state),
        fw_ver, fw_ver,
        millis()
    );

    // TINYML ON-DEVICE INFERENCE
    if let Some(inf) = &state.last_inference {
        let _ = write!(buf, ",\"tinyml\":{{\"label\":\"{}\",\"confidence\":", inf.label);
        write_sensor_float(buf, inf.confidence);
        let _ = write!(buf, ",\"class_idx\":{},\"probs\":[", inf.class_idx);
        for (i, p) in inf.probs.iter().enumerate() {
            if i > 0 { let _ = write!(buf, ","); }
            write_sensor_float(buf, *p);
        }
        let _ = write!(buf, "],\"latency_us\":{}}}", inf.latency_us);
    }

    let _ = write!(buf, "}}");
}

/// Mengirimkan paket telemetri JSON lengkap melalui serial terminal UART
pub fn send_json(state: &SystemState) {
    let fw_ver = crate::iot::ota::CURRENT_VERSION;
    let s = &state.sensors;
    print!("{{\"sensors\":{{\"mq6\":");
    print_sensor_float(s.mq6);
    print!(",\"mq8\":");
    print_sensor_float(s.mq8);
    print!(",\"mq135\":");
    print_sensor_float(s.mq135);
    print!(",\"mq3\":");
    print_sensor_float(s.mq3);
    print!(",\"mq7\":");
    print_sensor_float(s.mq7);
    print!(",\"tgs2611\":");
    print_sensor_float(s.tgs2611);
    print!(",\"tgs2602\":");
    print_sensor_float(s.tgs2602);
    print!(",\"tgs2600\":");
    print_sensor_float(s.tgs2600);
    print!(",\"tgs2620\":");
    print_sensor_float(s.tgs2620);
    print!(",\"dht1_temp\":");
    print_sensor_float(s.dht1_temp);
    print!(",\"dht1_hum\":");
    print_sensor_float(s.dht1_hum);
    print!(",\"dht2_temp\":");
    print_sensor_float(s.dht2_temp);
    print!(",\"dht2_hum\":");
    print_sensor_float(s.dht2_hum);
    print!("}},\"status\":{{\"inlet_pump\":{},\"inlet_valve\":{},", state.driver1_active, state.driver1_active);
    print!("\"outlet_pump\":{},\"outlet_valve\":{},", state.driver2_active, state.driver2_active);
    print!("\"inlet_time\":{},\"outlet_time\":{},\"elapsed_time\":{},\"firmware_version\":\"{}\"}},", get_inlet_time(state), get_outlet_time(state), get_elapsed_time(state), fw_ver);
    print!("\"firmware_version\":\"{}\",\"timestamp\":{}", fw_ver, millis());

    // TINYML ON-DEVICE INFERENCE
    if let Some(inf) = &state.last_inference {
        print!(",\"tinyml\":{{\"label\":\"{}\",\"confidence\":", inf.label);
        print_sensor_float(inf.confidence);
        print!(",\"class_idx\":{},\"probs\":[", inf.class_idx);
        for (i, p) in inf.probs.iter().enumerate() {
            if i > 0 { print!(","); }
            print_sensor_float(*p);
        }
        print!("],\"latency_us\":{}}}", inf.latency_us);
    }
    println!("}}");
}
