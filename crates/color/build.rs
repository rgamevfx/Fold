use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for input in [
        "scripts/build_color_runtime.py",
        "crates/color/native/bridge.cpp",
        "resources/color",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(input).display());
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "Cross-compiling the OCIO runtime is not supported yet"
    );
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "linux",
        "The OCIO runtime currently supports Linux"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    let cache = profile.join("fold-color");
    let status = Command::new("python3")
        .arg(root.join("scripts/build_color_runtime.py"))
        .arg("--ensure")
        .arg(&cache)
        .status()
        .expect("Color runtime setup requires Python 3, CMake, Ninja and a C++17 compiler");
    assert!(
        status.success(),
        "Color runtime setup failed; see the native build output above. Install Python 3, CMake, Ninja and a C++17 compiler; the first build also needs network access."
    );
    let package = fs::read_to_string(cache.join("package-path")).expect("built color package");
    for file in [
        "lib/fold/libfold_ocio.so",
        "lib/fold/libOpenColorIO.so.2.4",
        "share/fold/color/config.ocio",
        "share/fold/color/manifest.json",
    ] {
        println!("cargo:rerun-if-changed={package}/{file}");
    }
    println!("cargo:rustc-env=FOLD_BUILT_COLOR_ROOT={package}");
    println!("cargo:rustc-env=FOLD_COLOR_PROFILE={}", profile.display());
}
