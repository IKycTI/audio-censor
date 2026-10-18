use serde::Deserialize;
use std::path::PathBuf;
use tracing::warn;

use crate::{masking::MaskingStrategy, ms_to_frames, pipeline::OUTPUT_TARGET_FILL_MS};

#[derive(Deserialize)]
pub struct Settings {
    pub audio: AudioSettings,
    pub buffer: BufferSettings,
    pub decision: DecisionSettings,
    pub framing: FramingSettings,
    pub recognition: RecognitionSettings,
    pub masking: MaskingSettings,
    pub control: ControlSettings,
    pub logging: LoggingSettings,
    pub metrics: MetricsSettings,
}

const CONFIG_PATH: &str = "config/default.toml";

impl Settings {
    pub fn new(config_path: Option<PathBuf>) -> Result<Self, String> {
        let path = config_path.unwrap_or_else(|| PathBuf::from(CONFIG_PATH));

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("не удалось прочитать {}: {}", path.display(), e))?;

        toml::from_str(&content).map_err(|e| format!("ошибка разбора {}: {}", path.display(), e))
    }

    pub fn check(&self) -> Result<(), String> {
        // Чанк короче одного сэмпла — чанкер не сможет нарезать поток
        if ms_to_frames(self.framing.chunk_size_ms, self.audio.sample_rate) == 0 {
            return Err(format!(
                "framing.chunk_size_ms слишком мал для частоты {} Гц (сейчас {})",
                self.audio.sample_rate, self.framing.chunk_size_ms
            ));
        }

        if !(0.0..=1.0).contains(&self.recognition.min_confidence) {
            return Err(format!(
                "recognition.min_confidence должно быть в диапазоне 0.0–1.0 (сейчас {})",
                self.recognition.min_confidence
            ));
        }

        // Параметры маскирования проверяются все, а не только для выбранной стратегии:
        // иначе ошибка всплывёт только при смене strategy
        let masking = &self.masking;
        if !(0.0..=1.0).contains(&masking.amplitude) {
            return Err(format!(
                "masking.amplitude должно быть в диапазоне 0.0–1.0 (сейчас {}); \
                 больше 1 бип обрезается и превращается в меандр",
                masking.amplitude
            ));
        }
        if masking.attenuation_db > 0.0 || masking.attenuation_db.is_nan() {
            return Err(format!(
                "masking.attenuation_db должно быть не больше 0 (сейчас {}); \
                 положительное значение усиливает звук, а не приглушает",
                masking.attenuation_db
            ));
        }
        let nyquist = self.audio.sample_rate as f32 / 2.0;
        if !(masking.beep_frequency_hz > 0.0 && masking.beep_frequency_hz < nyquist) {
            return Err(format!(
                "masking.beep_frequency_hz должно быть в диапазоне (0, {}) Гц (сейчас {})",
                nyquist, masking.beep_frequency_hz
            ));
        }

        // Выходной буфер должен вмещать цель компенсации дрейфа с запасом в обе стороны
        if (self.buffer.ring_capacity_ms as u64) < 2 * OUTPUT_TARGET_FILL_MS {
            return Err(format!(
                "buffer.ring_capacity_ms должно быть не меньше {} (сейчас {})",
                2 * OUTPUT_TARGET_FILL_MS,
                self.buffer.ring_capacity_ms
            ));
        }

        // Слово должно успеть распознаться, пока его звук лежит в задержке
        if self.buffer.delay_ms <= self.framing.chunk_size_ms {
            warn!(
                "Задержка не больше размера чанка ({} <= {}): слова будут опаздывать, \
                 следите за метрикой late_intervals",
                self.buffer.delay_ms, self.framing.chunk_size_ms
            );
        }

        Ok(())
    }
}

#[derive(Deserialize)]
pub struct AudioSettings {
    pub input_device: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(with = "SampleFormatDef")]
    pub sample_format: cpal::SampleFormat,
}

#[derive(Deserialize)]
pub struct BufferSettings {
    pub ring_capacity_ms: usize,
    pub delay_ms: u64,
}

#[derive(Deserialize)]
pub struct DecisionSettings {
    pub words_dict_path: String,
    pub max_distance: u64,
}

#[derive(Deserialize)]
pub struct FramingSettings {
    pub chunk_size_ms: u64,
}

#[derive(Deserialize)]
pub struct RecognitionSettings {
    pub model_path: String,
    // Частичные гипотезы со словами и таймстемпами: слово ловится, пока фраза ещё идёт
    pub partial_results: bool,
    pub min_confidence: f32,
}

#[derive(Deserialize)]
pub struct MaskingSettings {
    pub strategy: MaskingStrategy,
    pub beep_frequency_hz: f32,
    pub amplitude: f32,
    pub attenuation_db: f32,
    pub padding_ms: u64,
}

#[derive(Deserialize)]
pub struct ControlSettings {
    pub enabled: bool,
    pub socket_path: String,
}

const SOCKET_NAME: &str = "audio-censor.sock";

impl ControlSettings {
    /// Путь к control-сокету: из конфига, а если там пусто — `$XDG_RUNTIME_DIR/audio-censor.sock`.
    /// XDG_RUNTIME_DIR — личный каталог пользователя с правами 0700, в отличие от общего /tmp
    pub fn resolved_socket_path(&self) -> Result<PathBuf, String> {
        if !self.socket_path.is_empty() {
            return Ok(PathBuf::from(&self.socket_path));
        }
        std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join(SOCKET_NAME))
            .ok_or_else(|| {
                "XDG_RUNTIME_DIR не задан — укажите control.socket_path в конфиге".to_string()
            })
    }
}

#[derive(Deserialize)]
pub struct LoggingSettings {
    pub level: String,
    // Каталог логов: файл на сеанс `<время запуска>.log`; пусто — вывод в stdout
    pub dir: String,
}

#[derive(Deserialize)]
pub struct MetricsSettings {
    pub enabled: bool,
    pub export_interval_ms: u64,
    // Каталог метрик: `<время запуска>.metrics.json` и сводка sessions.jsonl; пусто — не сохранять
    pub dir: String,
}

#[derive(serde::Deserialize)]
#[serde(remote = "cpal::SampleFormat")]
enum SampleFormatDef {
    I8,
    I16,
    I24,
    I32,
    I64,
    U8,
    U16,
    U24,
    U32,
    U64,
    F32,
    F64,
    DsdU8,
    DsdU16,
    DsdU32,
}
