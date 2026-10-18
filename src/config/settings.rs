use serde::Deserialize;
use std::path::PathBuf;
use tracing::warn;

use crate::{masking::MaskingStrategy, ms_to_frames};

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

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(err) => return Err(err.to_string()),
        };
        let params: Self = toml::from_str(&content).expect("Ошибка при разборе конфигурации");

        Ok(params)
    }

    pub fn check(&self) -> Result<(), String> {
        // Чанк короче одного сэмпла — чанкер не сможет нарезать поток
        if ms_to_frames(self.framing.chunk_size_ms, self.audio.sample_rate) == 0 {
            return Err(format!(
                "framing.chunk_size_ms слишком мал для частоты {} Гц (сейчас {})",
                self.audio.sample_rate, self.framing.chunk_size_ms
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
    pub language: Language,
    pub partial_results: bool,
    pub min_confidence: f32,
    pub case_sensitive: bool,
}

#[derive(Deserialize)]
pub enum Language {
    EN,
    RU,
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

#[derive(Deserialize)]
pub struct LoggingSettings {
    pub level: String,
    pub file_path: String,
}

#[derive(Deserialize)]
pub struct MetricsSettings {
    pub enabled: bool,
    pub export_interval_ms: u64,
    pub metrics_file: String,
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
