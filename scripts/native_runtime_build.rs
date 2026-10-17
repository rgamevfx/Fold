//! Shared Cargo build-script entry for the native runtime producers.
use std::{env, fs, path::PathBuf, process::Command};

pub fn prepare(kind: &str) {
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "Native runtime cross-compilation is not supported"
    );
    assert_eq!(
        env::var("TARGET").unwrap(),
        "x86_64-unknown-linux-gnu",
        "Native runtimes currently support Linux x86_64"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for file in [
        "scripts/prepare_runtimes.py",
        "scripts/runtime_cache.py",
        "scripts/native_runtime_build.rs",
        "scripts/build_rife_runtime.py",
        "resources/rife",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    if kind == "motion" {
        for file in [
            "scripts/stage_motion_sdk.py",
            "scripts/build_motion_skia.py",
            "scripts/motion_linker.py",
        ] {
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    let cache = profile.parent().unwrap().join("fold-runtime-cache");
    let status = Command::new("python3")
        .arg(root.join("scripts/prepare_runtimes.py"))
        .arg(kind)
        .arg(&cache)
        .status()
        .expect("Native runtime setup requires Python 3");
    assert!(
        status.success(),
        "Failed to prepare {kind}; see the producer output above and docs/development.md for build prerequisites"
    );
    let package =
        fs::read_to_string(cache.join(format!("{kind}-path"))).expect("prepared native runtime");
    // Cargo recursively watches the immutable package, including missing or
    // replaced model/library files, so the producer can repair damaged caches.
    println!("cargo:rerun-if-changed={package}");
    let kind = kind.to_ascii_uppercase();
    println!("cargo:rustc-env=FOLD_BUILT_{kind}_ROOT={package}");
    println!("cargo:rustc-env=FOLD_{kind}_PROFILE={}", profile.display());
}
