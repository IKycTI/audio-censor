use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Каталог с нативной библиотекой, заданный вручную (перекрывает .venv и lib/)
const VOSK_DIR_ENV: &str = "VOSK_LIB_DIR";
/// Каталог по старой схеме: библиотека, положенная руками
const LEGACY_DIR: &str = "lib";

/// Нативные файлы Vosk для целевой ОС
struct Platform {
    /// Файлы, которые нужны бинарнику рядом с собой; первый — сама libvosk
    files: &'static [&'static str],
    /// Где бинарник ищет библиотеки (rpath); None — ОС и так ищет рядом с exe
    rpath: Option<&'static str>,
}

fn platform(target_os: &str) -> Platform {
    match target_os {
        "linux" => Platform {
            files: &["libvosk.so"],
            // Бинарник ищет libvosk.so рядом с собой: запуск без cargo run и без LD_LIBRARY_PATH
            rpath: Some("$ORIGIN"),
        },
        // Windows — шаг 2, macOS — TODO (CROSSPLATFORM.md)
        other => panic!("сборка под {other} пока не поддерживается, см. CROSSPLATFORM.md"),
    }
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("cargo не передал CARGO_CFG_TARGET_OS");
    let platform = platform(&target_os);
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo не передал CARGO_MANIFEST_DIR"));

    println!("cargo:rerun-if-env-changed={VOSK_DIR_ENV}");
    // Смена версии vosk или пересоздание .venv должны пересобрать бинарник с новой библиотекой.
    // Только существующие пути: на отсутствующий cargo перезапускал бы build.rs каждый раз
    for path in ["uv.lock", ".venv/pyvenv.cfg"] {
        if manifest_dir.join(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    let lib_name = platform.files[0];
    let src_dir = find_lib_dir(&manifest_dir, lib_name);

    println!("cargo:rustc-link-search=native={}", src_dir.display());
    println!("cargo:rustc-link-lib=vosk");
    if let Some(rpath) = platform.rpath {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{rpath}");
    }

    let out_dir = env::var("OUT_DIR").expect("cargo не передал OUT_DIR");
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out → target/<profile>
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("неожиданная структура OUT_DIR");

    for file in platform.files {
        copy_if_outdated(&src_dir.join(file), &target_dir.join(file));
    }
}

/// Каталог, где лежит `lib_name`: VOSK_LIB_DIR → колесо vosk в .venv (`uv sync`) → lib/
fn find_lib_dir(manifest_dir: &Path, lib_name: &str) -> PathBuf {
    // Явно заданный каталог не подменяем молча другим: ошибка в пути должна быть видна
    if let Some(dir) = env::var_os(VOSK_DIR_ENV).filter(|d| !d.is_empty()) {
        let dir = manifest_dir.join(dir);
        if !dir.join(lib_name).is_file() {
            panic!(
                "{VOSK_DIR_ENV}={}: в каталоге нет {lib_name}",
                dir.display()
            );
        }
        return dir;
    }

    let candidates = venv_vosk_dirs(&manifest_dir.join(".venv"))
        .into_iter()
        .chain([manifest_dir.join(LEGACY_DIR)]);
    for dir in candidates {
        if dir.join(lib_name).is_file() {
            return dir;
        }
    }

    panic!(
        "Не найден {lib_name}: выполните `uv sync` (скачает колесо vosk в .venv), \
         либо положите библиотеку в {LEGACY_DIR}/, либо укажите каталог в {VOSK_DIR_ENV}"
    );
}

/// Каталоги пакета vosk в .venv: `lib/python3.*/site-packages/vosk` на Unix,
/// `Lib/site-packages/vosk` на Windows
fn venv_vosk_dirs(venv: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![venv.join("Lib").join("site-packages").join("vosk")];
    if let Ok(entries) = fs::read_dir(venv.join("lib")) {
        let mut pythons: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("python"))
            .map(|e| e.path().join("site-packages").join("vosk"))
            .collect();
        // Порядок read_dir не определён — сортируем, чтобы выбор был воспроизводимым
        pythons.sort();
        dirs.extend(pythons);
    }
    dirs
}

/// Копируем и при отсутствии, и при обновлении библиотеки: иначе после замены
/// исходной библиотеки бинарник продолжал бы грузить старую копию
fn copy_if_outdated(src: &Path, dest: &Path) {
    println!("cargo:rerun-if-changed={}", src.display());
    // И копию тоже: удалённую из target/ копию иначе никто бы не восстановил
    println!("cargo:rerun-if-changed={}", dest.display());

    let src_meta =
        fs::metadata(src).unwrap_or_else(|e| panic!("Не найден {} ({e})", src.display()));
    let up_to_date = fs::metadata(dest).is_ok_and(|dest_meta| {
        dest_meta.len() == src_meta.len()
            && match (dest_meta.modified(), src_meta.modified()) {
                (Ok(dest_time), Ok(src_time)) => dest_time >= src_time,
                _ => false,
            }
    });

    if !up_to_date {
        fs::copy(src, dest).unwrap_or_else(|e| {
            panic!(
                "Не удалось скопировать {} в {}: {e}",
                src.display(),
                dest.display()
            )
        });
        println!(
            "cargo:warning=Скопирован {} в {}",
            src.display(),
            dest.display()
        );
    }
}
