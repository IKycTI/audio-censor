use std::collections::BTreeMap;
use std::default::Default;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub stream_duration_ms: AtomicU64, // сколько звука принято с начала сеанса
    pub dropped_input_samples: AtomicU64, // capture: буфер был полон
    pub dropped_output_samples: AtomicU64, // orchestrator: не успели записать в output buffer
    pub underrun_samples: AtomicU64, // playback: не хватило данных, отдана тишина (без старта и завершения)
    pub words_recognized: AtomicU64, // сколько слов всего распознано
    pub words_censored: AtomicU64,   // сколько слов реально попало под цензуру
    pub recognizer_errors: AtomicU64, // сбои accept_waveform/result
    pub late_intervals: AtomicU64,   // опаздывающие интервалы
    pub partial_intervals: AtomicU64, // из них найдено по частичной гипотезе, до конца фразы
    pub unconfirmed_partial_intervals: AtomicU64, // частичная находка, которой нет в финале — ложное срабатывание
    pub drift_dropped_frames: AtomicU64,          // дрейф часов: выброшено кадров на выходе
    pub drift_inserted_frames: AtomicU64,         // дрейф часов: продублировано кадров на выходе
    pub output_prefills: AtomicU64, // выходной буфер был пуст и дополнен тишиной: 1 — старт, больше — срывы
    pub output_prefill_frames: AtomicU64, // сколько кадров тишины добавлено при этом
    // Задержка распознавания запрещённых слов: от конца слова до регистрации интервала, мс потока
    recognition_latency_count: AtomicU64,
    recognition_latency_sum_ms: AtomicU64,
    recognition_latency_max_ms: AtomicU64,
    // Время обработки одного чанка в потоке распознавания (Vosk + принятие решения), мкс.
    // Должно быть меньше длительности чанка, иначе чанки копятся в очереди
    chunk_processing_count: AtomicU64,
    chunk_processing_sum_us: AtomicU64,
    chunk_processing_max_us: AtomicU64,
    // Заглушённые слова по окончательным результатам: запись словаря → распознанная
    // форма → сколько раз. Пишет только поток распознавания, не аудио-callback,
    // поэтому мьютекс здесь допустим
    censored_words: Mutex<BTreeMap<String, BTreeMap<String, u64>>>,
}

impl Metrics {
    /// Учесть задержку распознавания одного запрещённого слова
    pub fn record_recognition_latency(&self, latency_ms: u64) {
        self.recognition_latency_count
            .fetch_add(1, Ordering::Relaxed);
        self.recognition_latency_sum_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.recognition_latency_max_ms
            .fetch_max(latency_ms, Ordering::Relaxed);
    }

    /// Учесть время обработки одного чанка
    pub fn record_chunk_processing(&self, processing_us: u64) {
        self.chunk_processing_count.fetch_add(1, Ordering::Relaxed);
        self.chunk_processing_sum_us
            .fetch_add(processing_us, Ordering::Relaxed);
        self.chunk_processing_max_us
            .fetch_max(processing_us, Ordering::Relaxed);
    }

    /// Учесть заглушённое слово: `entry` — запись словаря, `recognized` — что распознал Vosk
    pub fn record_censored_word(&self, entry: &str, recognized: &str) {
        let mut words = self
            .censored_words
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *words
            .entry(entry.to_string())
            .or_default()
            .entry(recognized.to_string())
            .or_default() += 1;
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let latency_count = self.recognition_latency_count.load(Ordering::Relaxed);
        let latency_sum = self.recognition_latency_sum_ms.load(Ordering::Relaxed);
        let chunk_count = self.chunk_processing_count.load(Ordering::Relaxed);
        let chunk_sum = self.chunk_processing_sum_us.load(Ordering::Relaxed);
        MetricsSnapshot {
            stream_duration_ms: self.stream_duration_ms.load(Ordering::Relaxed),
            dropped_input_samples: self.dropped_input_samples.load(Ordering::Relaxed),
            dropped_output_samples: self.dropped_output_samples.load(Ordering::Relaxed),
            underrun_samples: self.underrun_samples.load(Ordering::Relaxed),
            words_recognized: self.words_recognized.load(Ordering::Relaxed),
            words_censored: self.words_censored.load(Ordering::Relaxed),
            recognizer_errors: self.recognizer_errors.load(Ordering::Relaxed),
            late_intervals: self.late_intervals.load(Ordering::Relaxed),
            partial_intervals: self.partial_intervals.load(Ordering::Relaxed),
            unconfirmed_partial_intervals: self
                .unconfirmed_partial_intervals
                .load(Ordering::Relaxed),
            drift_dropped_frames: self.drift_dropped_frames.load(Ordering::Relaxed),
            drift_inserted_frames: self.drift_inserted_frames.load(Ordering::Relaxed),
            output_prefills: self.output_prefills.load(Ordering::Relaxed),
            output_prefill_frames: self.output_prefill_frames.load(Ordering::Relaxed),
            recognition_latency_samples: latency_count,
            // Пока нет ни одного слова — null, а не 0: ноль выглядел бы как мгновенное распознавание
            recognition_latency_avg_ms: (latency_count > 0).then(|| latency_sum / latency_count),
            recognition_latency_max_ms: (latency_count > 0)
                .then(|| self.recognition_latency_max_ms.load(Ordering::Relaxed)),
            chunks_processed: chunk_count,
            chunk_processing_avg_us: (chunk_count > 0).then(|| chunk_sum / chunk_count),
            chunk_processing_max_us: (chunk_count > 0)
                .then(|| self.chunk_processing_max_us.load(Ordering::Relaxed)),
            censored_words: self
                .censored_words
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .map(|(entry, forms)| {
                    let stats = CensoredWordStats {
                        count: forms.values().sum(),
                        recognized_as: forms.clone(),
                    };
                    (entry.clone(), stats)
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub stream_duration_ms: u64,
    pub dropped_input_samples: u64,
    pub dropped_output_samples: u64,
    pub underrun_samples: u64,
    pub words_recognized: u64,
    pub words_censored: u64,
    pub recognizer_errors: u64,
    pub late_intervals: u64,
    pub partial_intervals: u64,
    pub unconfirmed_partial_intervals: u64,
    pub drift_dropped_frames: u64,
    pub drift_inserted_frames: u64,
    pub output_prefills: u64,
    pub output_prefill_frames: u64,
    pub recognition_latency_samples: u64,
    pub recognition_latency_avg_ms: Option<u64>,
    pub recognition_latency_max_ms: Option<u64>,
    pub chunks_processed: u64,
    pub chunk_processing_avg_us: Option<u64>,
    pub chunk_processing_max_us: Option<u64>,
    pub censored_words: BTreeMap<String, CensoredWordStats>,
}

/// Сколько раз сработала запись словаря и какими распознанными словами
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CensoredWordStats {
    pub count: u64,
    pub recognized_as: BTreeMap<String, u64>,
}
