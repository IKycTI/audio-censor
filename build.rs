use std::env;
use std::fs;
use std::path::Path;

const VOSK_LIB: &str = "lib/libvosk.so";

fn main() {
    println!("cargo:rustc-link-search=./lib");
    println!("cargo:rustc-link-lib=vosk");
    // Бинарник ищет libvosk.so рядом с собой: запуск без cargo run и без LD_LIBRARY_PATH
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");

    println!("cargo:rerun-if-changed={VOSK_LIB}");

    let src = Path::new(VOSK_LIB);
    let src_meta = fs::metadata(src).unwrap_or_else(|e| {
        panic!("Не найден {VOSK_LIB} ({e}): скачайте libvosk.so и положите в каталог lib/")
    });

    let out_dir = env::var("OUT_DIR").expect("cargo не передал OUT_DIR");
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out → target/<profile>
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("неожиданная структура OUT_DIR");
    let dest = target_dir.join("libvosk.so");

    // Копируем и при отсутствии, и при обновлении библиотеки: иначе после замены
    // lib/libvosk.so бинарник продолжал бы грузить старую копию
    let up_to_date = fs::metadata(&dest).is_ok_and(|dest_meta| {
        dest_meta.len() == src_meta.len()
            && match (dest_meta.modified(), src_meta.modified()) {
                (Ok(dest_time), Ok(src_time)) => dest_time >= src_time,
                _ => false,
            }
    });

    if !up_to_date {
        fs::copy(src, &dest).unwrap_or_else(|e| {
            panic!(
                "Не удалось скопировать {VOSK_LIB} в {}: {e}",
                dest.display()
            )
        });
        println!("cargo:warning=Скопирован libvosk.so в {}", dest.display());
    }
}
