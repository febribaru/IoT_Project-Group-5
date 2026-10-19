//! Operasi Matriks TinyML On-Device Forward Pass (Neural Network MLP 3-Input -> 32-Hidden -> 5-Output)
//! Didesain khusus untuk ESP32-S3 Bare-Metal no_std tanpa alokasi memori dinamis (Heapless).

use crate::models::model_data::*;

pub struct TinyClassifier;

impl TinyClassifier {
    /// Menghitung probabilitas inferensi mutu kopi Robusta dari nilai sensor gas
    pub fn forward(features: &[f32; NUM_FEATURES]) -> [f32; NUM_CLASSES] {
        // 1. Z-Score Normalization: (x - mean) / std
        let mut norm = [0.0f32; NUM_FEATURES];
        let mut i = 0;
        while i < NUM_FEATURES {
            norm[i] = (features[i] - FEATURE_MEANS[i]) / FEATURE_STDS[i];
            i += 1;
        }

        // 2. Lapisan Tersembunyi (Hidden Layer: 32 neuron dengan aktivasi ReLU)
        let mut h = [0.0f32; HIDDEN_DIM];
        let mut j = 0;
        while j < HIDDEN_DIM {
            let mut sum = B1[j];
            let mut f = 0;
            while f < NUM_FEATURES {
                sum += norm[f] * W1[f][j];
                f += 1;
            }
            // ReLU: max(0, sum)
            h[j] = if sum > 0.0 { sum } else { 0.0 };
            j += 1;
        }

        // 3. Lapisan Keluaran (Output Layer: 5 logits)
        let mut logits = [0.0f32; NUM_CLASSES];
        let mut c = 0;
        while c < NUM_CLASSES {
            let mut sum = B2[c];
            let mut k = 0;
            while k < HIDDEN_DIM {
                sum += h[k] * W2[k][c];
                k += 1;
            }
            logits[c] = sum;
            c += 1;
        }

        // 4. Softmax Numerik Stabil (Mencegah overflow eksponensial)
        // Cari nilai logit maksimum
        let mut max_l = logits[0];
        let mut c = 1;
        while c < NUM_CLASSES {
            if logits[c] > max_l {
                max_l = logits[c];
            }
            c += 1;
        }

        // Hitung exp(logit - max_l) dan akumulasikan sum_exp
        let mut exps = [0.0f32; NUM_CLASSES];
        let mut sum_exp = 0.0f32;
        let mut c = 0;
        while c < NUM_CLASSES {
            exps[c] = libm::expf(logits[c] - max_l);
            sum_exp += exps[c];
            c += 1;
        }

        // Normalisasi probabilitas
        let mut probs = [1.0 / (NUM_CLASSES as f32); NUM_CLASSES];
        if sum_exp > 0.0 {
            let mut c = 0;
            while c < NUM_CLASSES {
                probs[c] = exps[c] / sum_exp;
                c += 1;
            }
        }
        probs
    }
}
