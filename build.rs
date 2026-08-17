use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rustc-link-search=./lib");
    println!("cargo:rustc-link-lib=vosk");

    println!("cargo:rerun-if-changed=lib/libvosk.so");

    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir).ancestors().nth(3).unwrap();

    let dest_path = target_dir.join("libvosk.so");
    if !dest_path.exists() {
        fs::copy("lib/libvosk.so", &dest_path).unwrap();
        println!(
            "cargo:warning=Скопирован libvosk.so в {}",
            dest_path.display()
        );
    }
}
