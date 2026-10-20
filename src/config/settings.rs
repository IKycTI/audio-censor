use serde::Deserialize;
use std::path::{Path, PathBuf};
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

/// Каталог программы в пользовательском каталоге конфигураций
const APP_DIR: &str = "audio-censor";
/// Конфиг пользователя: `~/.config/audio-censor/config.toml`
const USER_CONFIG: &str = "config.toml";
/// Конфиг по умолчанию — из поставки: от текущего каталога или рядом с бинарником
const DEFAULT_CONFIG: &str = "config/default.toml";

/// Путь к конфигу: заданный через `-c` — как есть, иначе первый существующий из
/// [`config_candidates`]
pub fn locate_config(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let candidates = config_candidates();
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or_else(|| {
            let list: Vec<String> = candidates
                .iter()
                .map(|p| format!("  {}", p.display()))
                .collect();
            format!(
                "конфиг не найден, проверено:\n{}\nукажите путь через -c",
                list.join("\n")
            )
        })
}

/// Где ищется конфиг без `-c`, по порядку:
/// 1. пользовательский — `$XDG_CONFIG_HOME/audio-censor/config.toml` (обычно `~/.config/...`),
///    на Windows `%APPDATA%\audio-censor\config.toml`;
/// 2. `config/default.toml` от текущего каталога — запуск из корня репозитория (`cargo run`);
/// 3. `config/default.toml` рядом с бинарником — распакованная поставка, запуск откуда угодно
pub fn config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = user_config_dir() {
        candidates.push(dir.join(APP_DIR).join(USER_CONFIG));
    }
    candidates.push(PathBuf::from(DEFAULT_CONFIG));
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join(DEFAULT_CONFIG));
    }
    candidates
}

/// Каталог пользовательских конфигураций по XDG: `$XDG_CONFIG_HOME`, иначе `~/.config`.
/// Относительный XDG_CONFIG_HOME по спецификации игнорируется.
/// TODO (macOS): принято `~/Library/Application Support`
#[cfg(unix)]
fn user_config_dir() -> Option<PathBuf> {
    let non_empty = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    non_empty("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| non_empty("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// `%APPDATA%` — роуминговый профиль пользователя, туда на Windows кладут настройки
#[cfg(windows)]
fn user_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Settings {
    /// Загрузить конфиг по пути (найти его — [`locate_config`])
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("не удалось прочитать {}: {}", path.display(), e))?;

        let mut settings: Settings = toml::from_str(&content)
            .map_err(|e| format!("ошибка разбора {}: {}", path.display(), e))?;
        settings.resolve_paths(path.parent().unwrap_or(Path::new("")))?;
        Ok(settings)
    }

    /// Относительные пути из конфига — от каталога конфиг-файла, а не от текущего каталога:
    /// иначе при автозапуске (systemd, Планировщик заданий Windows) они зависели бы от того,
    /// откуда запустили демон
    fn resolve_paths(&mut self, config_dir: &Path) -> Result<(), String> {
        for value in [
            &mut self.decision.words_dict_path,
            &mut self.recognition.model_path,
            &mut self.control.socket_path,
            &mut self.logging.dir,
            &mut self.metrics.dir,
        ] {
            resolve_path(config_dir, value)?;
        }
        Ok(())
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

/// Пустая строка («выключено» / «по умолчанию») и абсолютные пути не меняются
fn resolve_path(config_dir: &Path, value: &mut String) -> Result<(), String> {
    if value.is_empty() || Path::new(value.as_str()).is_absolute() {
        return Ok(());
    }
    *value = config_dir
        .join(value.as_str())
        .into_os_string()
        .into_string()
        .map_err(|p| format!("путь {} не в UTF-8", p.to_string_lossy()))?;
    Ok(())
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

#[cfg(unix)]
const SOCKET_NAME: &str = "audio-censor.sock";
#[cfg(windows)]
const PIPE_PREFIX: &str = r"\\.\pipe\audio-censor";

impl ControlSettings {
    /// Путь к control-сокету: из конфига, а если там пусто — путь по умолчанию для ОС
    pub fn resolved_socket_path(&self) -> Result<PathBuf, String> {
        if !self.socket_path.is_empty() {
            return Ok(PathBuf::from(&self.socket_path));
        }
        default_socket_path()
    }
}

/// `$XDG_RUNTIME_DIR/audio-censor.sock`. XDG_RUNTIME_DIR — личный каталог пользователя
/// с правами 0700, в отличие от общего /tmp.
/// TODO (macOS): там XDG_RUNTIME_DIR нет — брать $TMPDIR
#[cfg(unix)]
fn default_socket_path() -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(SOCKET_NAME))
        .ok_or_else(|| {
            "XDG_RUNTIME_DIR не задан — укажите control.socket_path в конфиге".to_string()
        })
}

/// `\\.\pipe\audio-censor-<пользователь>`. Каналы видны всей машине, поэтому в имени
/// пользователь: у каждого свой демон, и они не мешают друг другу
#[cfg(windows)]
fn default_socket_path() -> Result<PathBuf, String> {
    let name = match std::env::var("USERNAME") {
        Ok(user) if !user.is_empty() => format!("{PIPE_PREFIX}-{user}"),
        _ => PIPE_PREFIX.to_string(),
    };
    Ok(PathBuf::from(name))
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
