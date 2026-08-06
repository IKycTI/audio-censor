use serde::Deserialize;

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

impl Settings {
    pub fn new(config_path: &str) -> Self {
        let content = std::fs::read_to_string(config_path).expect(&format!(
            "Файл конфигурации по пути {} не обнаружен",
            config_path
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
    pub channels: u8,
    pub sample_format: String,
}

#[derive(Deserialize)]
pub struct BufferSettings {
    pub input_ring_capacity_ms: usize,
    pub output_ring_capacity_ms: usize,
    pub delay_ms: usize,
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
    pub max_distance: usize,
}

#[derive(Deserialize)]
pub struct FramingSettings {
    pub chunk_size_ms: usize,
    pub overlap_ms: usize,
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
    pub padding_ms: usize,
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
    pub export_interval_ms: usize,
}
