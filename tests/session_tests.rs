use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use audio_censor::control::{ControlCommand, ControlHandle};
use audio_censor::metrics::{Metrics, MetricsReporter, ReportConfig};
use audio_censor::session::Session;

// ── Имя сеанса ─────────────────────────────────────────────────────

#[test]
fn session_id_and_timestamp_are_utc() {
    // Эталон — datetime.fromtimestamp(..., timezone.utc) в Python
    let s = Session::at(1760794674);
    assert_eq!(s.id, "2025-10-18_13-37-54");
    assert_eq!(s.started_at, "2025-10-18T13:37:54Z");
}

#[test]
fn session_dates_match_reference() {
    for (secs, expected) in [
        (0, "1970-01-01_00-00-00"),
        (951_782_400, "2000-02-29_00-00-00"), // високосный год, кратный 400
        (1_709_164_800, "2024-02-29_00-00-00"),
        (1_709_251_199, "2024-02-29_23-59-59"),
        (4_102_444_799, "2099-12-31_23-59-59"),
        (253_402_300_799, "9999-12-31_23-59-59"),
    ] {
        assert_eq!(Session::at(secs).id, expected, "{secs}");
    }
}

#[test]
fn session_id_is_safe_for_file_names() {
    let id = Session::start().id;
    assert!(
        !id.contains(':') && !id.contains(' ') && !id.contains('/'),
        "{id}"
    );
}

// ── Сохранение метрик ──────────────────────────────────────────────

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "audio-censor-session-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config() -> ReportConfig {
    ReportConfig {
        sample_rate: 44100,
        channels: 2,
        sample_format: "F32".into(),
        delay_ms: 500,
        chunk_size_ms: 100,
        partial_results: true,
        min_confidence: 0.0,
        max_distance: 1,
        strategy: "Beep".into(),
        padding_ms: 50,
    }
}

fn reporter(dir: &TempDir) -> MetricsReporter {
    // Каталога ещё нет — save создаёт его сам
    MetricsReporter::new(dir.0.join("logs"), Session::at(1760794674), config())
}

fn read_json(path: &PathBuf) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn metrics_file_is_named_after_session_and_has_context() {
    let dir = TempDir::new("context");
    let reporter = reporter(&dir);
    let metrics = Metrics::default();
    metrics.late_intervals.fetch_add(7, Ordering::Relaxed);

    let path = reporter.save(&metrics, true).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        "2025-10-18_13-37-54.metrics.json"
    );

    let json = read_json(&path);
    assert_eq!(json["session"], "2025-10-18_13-37-54");
    assert_eq!(json["started_at"], "2025-10-18T13:37:54Z");
    assert_eq!(json["final"], true);
    assert_eq!(json["config"]["delay_ms"], 500);
    assert_eq!(json["config"]["strategy"], "Beep");
    assert_eq!(json["metrics"]["late_intervals"], 7);
}

#[test]
fn intermediate_snapshot_is_not_added_to_sessions_summary() {
    let dir = TempDir::new("intermediate");
    let reporter = reporter(&dir);

    let path = reporter.save(&Metrics::default(), false).unwrap();
    assert_eq!(read_json(&path)["final"], false);
    assert!(!reporter.sessions_path().exists());
}

#[test]
fn final_report_overwrites_snapshot_and_appends_summary_line() {
    let dir = TempDir::new("final");
    let reporter = reporter(&dir);
    let metrics = Metrics::default();

    reporter.save(&metrics, false).unwrap();
    metrics.words_censored.fetch_add(3, Ordering::Relaxed);
    let path = reporter.save(&metrics, true).unwrap();

    let json = read_json(&path);
    assert_eq!(json["final"], true);
    assert_eq!(json["metrics"]["words_censored"], 3);

    let summary = std::fs::read_to_string(reporter.sessions_path()).unwrap();
    assert_eq!(summary.lines().count(), 1);
    let line: serde_json::Value = serde_json::from_str(summary.lines().next().unwrap()).unwrap();
    assert_eq!(line["session"], "2025-10-18_13-37-54");
    assert_eq!(line["metrics"]["words_censored"], 3);
}

#[test]
fn sessions_summary_accumulates_runs() {
    let dir = TempDir::new("accumulate");
    let logs = dir.0.join("logs");
    for secs in [1760794674, 1760798274] {
        MetricsReporter::new(&logs, Session::at(secs), config())
            .save(&Metrics::default(), true)
            .unwrap();
    }

    let summary = std::fs::read_to_string(logs.join("sessions.jsonl")).unwrap();
    let sessions: Vec<String> = summary
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["session"].to_string())
        .collect();
    assert_eq!(
        sessions,
        vec!["\"2025-10-18_13-37-54\"", "\"2025-10-18_14-37-54\""]
    );
    assert!(logs.join("2025-10-18_13-37-54.metrics.json").exists());
    assert!(logs.join("2025-10-18_14-37-54.metrics.json").exists());
}

// ── Команда save-metrics ───────────────────────────────────────────

fn handle(reporter: Option<MetricsReporter>) -> ControlHandle {
    ControlHandle::new(
        Arc::new(AtomicBool::new(true)),
        Arc::new(Metrics::default()),
        Arc::new(AtomicBool::new(false)),
        reporter,
    )
}

#[test]
fn save_metrics_command_writes_intermediate_snapshot() {
    let dir = TempDir::new("command");
    let reporter = reporter(&dir);
    let path = reporter.metrics_path();

    let response = handle(Some(reporter.clone())).execute(ControlCommand::SaveMetrics);
    assert!(response.starts_with("OK"), "{response}");
    assert_eq!(read_json(&path)["final"], false);
    assert!(!reporter.sessions_path().exists());
}

#[test]
fn save_metrics_command_reports_disabled_metrics() {
    let response = handle(None).execute(ControlCommand::SaveMetrics);
    assert!(response.starts_with("ERROR"), "{response}");
}
