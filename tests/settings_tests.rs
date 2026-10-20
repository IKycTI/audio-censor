use std::path::{Path, PathBuf};

use audio_censor::config::{Settings, config_candidates, locate_config};

const DEFAULT_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/config/default.toml");

fn config_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("config")
}

/// Отдельный каталог на каждый тест (тесты идут параллельно), удаляется по завершении
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "audio-censor-settings-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// default.toml с заменённой строкой `key = ...` (первое вхождение)
fn default_with(key: &str, value: &str) -> String {
    let content = std::fs::read_to_string(DEFAULT_CONFIG).unwrap();
    let prefix = format!("{key} = ");
    let mut replaced = false;
    content
        .lines()
        .map(|line| {
            if !replaced && line.starts_with(&prefix) {
                replaced = true;
                format!("{prefix}{value}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Пути относительно каталога конфига ─────────────────────────────

#[test]
fn default_config_paths_are_resolved_from_config_dir() {
    let settings = Settings::load(Path::new(DEFAULT_CONFIG)).unwrap();
    let dir = config_dir();

    assert_eq!(
        PathBuf::from(&settings.decision.words_dict_path),
        dir.join("banned_words.json")
    );
    assert_eq!(
        PathBuf::from(&settings.recognition.model_path),
        dir.join("../models/vosk-model-ru-0.22")
    );
    assert_eq!(PathBuf::from(&settings.logging.dir), dir.join("../logs"));
    assert_eq!(PathBuf::from(&settings.metrics.dir), dir.join("../logs"));
}

#[test]
fn default_config_points_to_existing_dictionary() {
    // Словарь в git: если путь в default.toml разошёлся с раскладкой — демон бы не запустился
    let settings = Settings::load(Path::new(DEFAULT_CONFIG)).unwrap();
    assert!(
        Path::new(&settings.decision.words_dict_path).is_file(),
        "{}",
        settings.decision.words_dict_path
    );
}

#[test]
fn paths_do_not_depend_on_current_dir() {
    // Конфиг в другом каталоге: пути — от него, а не от каталога, где запущены тесты
    let tmp = TempDir::new("elsewhere");
    let path = tmp.0.join("my.toml");
    std::fs::write(&path, default_with("model_path", r#""model""#)).unwrap();

    let settings = Settings::load(&path).unwrap();
    assert_eq!(
        PathBuf::from(&settings.recognition.model_path),
        tmp.0.join("model")
    );
}

#[test]
fn absolute_path_is_kept() {
    let tmp = TempDir::new("absolute");
    let model = tmp.0.join("абсолютная").join("модель");
    let path = tmp.0.join("my.toml");
    // Литеральная строка TOML: обратные слэши Windows не экранируются
    let value = format!("'{}'", model.display());
    std::fs::write(&path, default_with("model_path", &value)).unwrap();

    let settings = Settings::load(&path).unwrap();
    assert_eq!(PathBuf::from(&settings.recognition.model_path), model);
}

#[test]
fn empty_paths_keep_their_meaning() {
    // Пустые значения — «в stdout», «не сохранять», «сокет по умолчанию», не каталог конфига
    let tmp = TempDir::new("empty");
    let path = tmp.0.join("my.toml");
    std::fs::write(&path, default_with("dir", r#""""#)).unwrap();

    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.logging.dir, "");
    assert_eq!(settings.control.socket_path, "");
}

// ── Поиск конфига ──────────────────────────────────────────────────

#[test]
fn explicit_config_path_is_used_as_is() {
    // Даже несуществующий: ошибка чтения покажет именно тот путь, что указан в -c
    let path = PathBuf::from("нет/такого.toml");
    assert_eq!(locate_config(Some(path.clone())).unwrap(), path);
}

#[test]
fn user_config_is_searched_before_bundled_default() {
    let candidates = config_candidates();
    let default = candidates
        .iter()
        .position(|p| p == Path::new("config/default.toml"))
        .expect("config/default.toml от текущего каталога");

    // Без HOME/APPDATA (бывает в CI) пользовательского каталога может не быть
    if let Some(user) = candidates
        .iter()
        .position(|p| p.ends_with(Path::new("audio-censor").join("config.toml")))
    {
        assert!(user < default, "{candidates:?}");
    }
    // Последний — рядом с бинарником
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    assert_eq!(
        candidates.last().unwrap(),
        &exe_dir.join("config/default.toml")
    );
}
