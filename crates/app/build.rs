//! Build identity without hashing a large executable on the first render.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
fn files(path: &Path, result: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", path.display());
    let mut entries: Vec<_> = fs::read_dir(path)
        .expect("build inputs")
        .map(|entry| entry.expect("build entry").path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            files(&entry, result);
        } else if entry.is_file() {
            result.push(entry);
        }
    }
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let mut inputs = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for directory in ["crates", "resources", "skills/fold"] {
        files(&root.join(directory), &mut inputs);
    }
    inputs.sort();
    let mut hash = Sha256::new();
    for input in inputs {
        println!("cargo:rerun-if-changed={}", input.display());
        let name = input.strip_prefix(&root).unwrap().to_string_lossy();
        let bytes = fs::read(&input).expect("read build input");
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    println!("cargo:rustc-env=FOLD_SOURCE_SHA256={:x}", hash.finalize());
    println!(
        "cargo:rustc-env=FOLD_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
    println!(
        "cargo:rustc-env=FOLD_BUILD_TARGET={}",
        env::var("TARGET").unwrap()
    );
}
