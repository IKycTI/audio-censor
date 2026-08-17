use std::default::Default;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub dropped_input_samples: AtomicU64, // capture: буфер был полон
    pub dropped_output_samples: AtomicU64, // orchestrator: не успели записать в output buffer
    pub underrun_samples: AtomicU64,      // playback: не хватило данных, отдана тишина
    pub words_recognized: AtomicU64,      // сколько слов всего распознано
    pub words_censored: AtomicU64,        // сколько слов реально попало под цензуру
    pub recognizer_errors: AtomicU64,     // сбои accept_waveform/result
}

impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            dropped_input_samples: self.dropped_input_samples.load(Ordering::Relaxed),
            dropped_output_samples: self.dropped_output_samples.load(Ordering::Relaxed),
            underrun_samples: self.underrun_samples.load(Ordering::Relaxed),
            words_recognized: self.words_recognized.load(Ordering::Relaxed),
            words_censored: self.words_censored.load(Ordering::Relaxed),
            recognizer_errors: self.recognizer_errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub dropped_input_samples: u64,
    pub dropped_output_samples: u64,
    pub underrun_samples: u64,
    pub words_recognized: u64,
    pub words_censored: u64,
    pub recognizer_errors: u64,
}
