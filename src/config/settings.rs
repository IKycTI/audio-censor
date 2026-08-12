use serde::Deserialize;
use std::path::PathBuf;

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
    pub fn new(config_path: Option<PathBuf>) -> Self {
        let path = config_path.unwrap_or_else(|| PathBuf::from(CONFIG_PATH));
        let path_str = path.to_str().expect("Не удалось получить строку пути");

        let content = std::fs::read_to_string(&path).expect(&format!(
            "Файл конфигурации по пути {} не обнаружен",
            path_str
        ));
        let params: Self = toml::from_str(&content).expect("Ошибка при разборе конфигурации");
        params
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
    // pub input_ring_capacity_ms: u64,
    // pub output_ring_capacity_ms: u64,
    pub ring_capacity_ms: usize,
    pub delay_ms: u64,
    pub overflow_policy: OverflowPolicy,
}

#[derive(Deserialize)]
pub enum OverflowPolicy {
    DropOldest,
    DropNewest,
    Block,
}

#[derive(Deserialize)]
pub struct DecisionSettings {
    pub words_dict_path: String,
    pub max_distance: u64,
}

#[derive(Deserialize)]
pub struct FramingSettings {
    pub chunk_size_ms: u64,
    pub overlap_ms: u64,
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
    pub beep_frequency_hz: u32,
    pub attenuation_db: f32,
    pub padding_ms: u64,
}

#[derive(Deserialize)]
pub enum MaskingStrategy {
    Beep,
    Silence,
    MuteAttenuate,
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
