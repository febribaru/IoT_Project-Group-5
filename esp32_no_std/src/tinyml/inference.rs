//! Evaluasi Inferensi On-Device Mutu Kopi Robusta & Deteksi Anomali

use crate::state::SensorData;
use crate::models::model_data::*;
use super::model::TinyClassifier;

#[derive(Debug, Clone, Copy)]
pub struct InferenceResult {
    pub label: &'static str,
    pub confidence: f32,
    pub class_idx: usize,
    pub probs: [f32; NUM_CLASSES],
    pub latency_us: u32,
}

pub struct CoffeeQualityEngine;

impl CoffeeQualityEngine {
    pub fn infer(s: &SensorData) -> InferenceResult {
        let t_start = crate::timing::micros();
        let raw = [s.mq7, s.mq3, s.mq8];
        let probs = TinyClassifier::forward(&raw);

        let mut best_idx = 0;
        let mut best_prob = probs[0];
        let mut c = 1;
        while c < NUM_CLASSES {
            if probs[c] > best_prob {
                best_prob = probs[c];
                best_idx = c;
            }
            c += 1;
        }
        let t_end = crate::timing::micros();
        let latency = (t_end.saturating_sub(t_start)).max(1) as u32;

        InferenceResult {
            label: CLASS_LABELS[best_idx],
            confidence: best_prob,
            class_idx: best_idx,
            probs,
            latency_us: latency,
        }
    }

    pub fn get_class_label(idx: usize) -> &'static str {
        if idx < NUM_CLASSES {
            CLASS_LABELS[idx]
        } else {
            "Unknown"
        }
    }
}
