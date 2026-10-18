use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::Settings;
use crate::metrics::{Metrics, MetricsSnapshot};
use crate::session::Session;

const SESSIONS_FILE: &str = "sessions.jsonl";

/// Параметры конфига, от которых зависят метрики: без них файл метрик
/// через неделю не отнести ни к какому опыту
#[derive(Debug, Clone, Serialize)]
pub struct ReportConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub delay_ms: u64,
    pub chunk_size_ms: u64,
    pub partial_results: bool,
    pub min_confidence: f32,
    pub max_distance: u64,
    pub strategy: String,
    pub padding_ms: u64,
}

impl ReportConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        ReportConfig {
            sample_rate: settings.audio.sample_rate,
            channels: settings.audio.channels,
            sample_format: format!("{:?}", settings.audio.sample_format),
            delay_ms: settings.buffer.delay_ms,
            chunk_size_ms: settings.framing.chunk_size_ms,
            partial_results: settings.recognition.partial_results,
            min_confidence: settings.recognition.min_confidence,
            max_distance: settings.decision.max_distance,
            strategy: format!("{:?}", settings.masking.strategy),
            padding_ms: settings.masking.padding_ms,
        }
    }
}

#[derive(Serialize)]
struct SessionReport<'a> {
    session: &'a str,
    started_at: &'a str,
    /// false — промежуточный снимок (команда save-metrics), true — итог при остановке
    #[serde(rename = "final")]
    is_final: bool,
    config: &'a ReportConfig,
    metrics: MetricsSnapshot,
}

/// Сохранение метрик сеанса: `<dir>/<сеанс>.metrics.json` и при остановке —
/// строка в `<dir>/sessions.jsonl`, сводке всех сеансов для сравнения опытов
#[derive(Debug, Clone)]
pub struct MetricsReporter {
    dir: PathBuf,
    session: Session,
    config: ReportConfig,
}

impl MetricsReporter {
    pub fn new(dir: impl Into<PathBuf>, session: Session, config: ReportConfig) -> Self {
        MetricsReporter {
            dir: dir.into(),
            session,
            config,
        }
    }

    pub fn metrics_path(&self) -> PathBuf {
        self.dir.join(format!("{}.metrics.json", self.session.id))
    }

    pub fn sessions_path(&self) -> PathBuf {
        self.dir.join(SESSIONS_FILE)
    }

    /// Записать снимок метрик. Итоговый (`is_final`) ещё и дописывается в sessions.jsonl
    pub fn save(&self, metrics: &Metrics, is_final: bool) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("не удалось создать {}: {}", self.dir.display(), e))?;

        let report = SessionReport {
            session: &self.session.id,
            started_at: &self.session.started_at,
            is_final,
            config: &self.config,
            metrics: metrics.snapshot(),
        };

        let path = self.metrics_path();
        let pretty = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        write_atomically(&path, &pretty)?;

        if is_final {
            let line = serde_json::to_string(&report).map_err(|e| e.to_string())?;
            let sessions = self.sessions_path();
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&sessions)
                .map_err(|e| format!("не удалось открыть {}: {}", sessions.display(), e))?;
            writeln!(file, "{line}")
                .map_err(|e| format!("не удалось дописать {}: {}", sessions.display(), e))?;
        }

        Ok(path)
    }
}

/// Пишем во временный файл и переименовываем: rename атомарен в пределах ФС,
/// поэтому внешний читатель никогда не увидит наполовину записанный JSON
fn write_atomically(path: &Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)
        .map_err(|e| format!("не удалось записать {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("не удалось записать {}: {}", path.display(), e))
}
