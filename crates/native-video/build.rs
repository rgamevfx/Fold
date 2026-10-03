fn main() {
    println!("cargo:rerun-if-env-changed=FOLD_VIDEO_ROOT");
    #[cfg(feature = "helper")]
    build_helper();
}
#[cfg(feature = "helper")]
fn build_helper() {
    use std::path::PathBuf;
    let root = PathBuf::from(std::env::var_os("FOLD_VIDEO_ROOT")
        .expect("helper requires FOLD_VIDEO_ROOT from scripts/build_video_runtime.py; system FFmpeg is forbidden"));
    let manifest = std::fs::read_to_string(root.join("share/fold-video/runtime.json"))
        .expect("missing pinned runtime manifest");
    assert!(
        manifest.contains("LGPL-2.1-or-later") && manifest.contains("6.1.1"),
        "unapproved FFmpeg runtime"
    );
    let bindings = bindgen::Builder::default()
        .header_contents("wrapper.h", "#include <libavformat/avformat.h>\n#include <libavcodec/avcodec.h>\n#include <libavutil/hwcontext.h>\n#include <libavutil/hwcontext_cuda.h>\n#include <cuda.h>\n")
        .clang_arg(format!("-I{}", root.join("include").display()))
        .allowlist_function("av.*").allowlist_function("cu.*")
        .allowlist_type("AV.*").allowlist_type("CU.*")
        .allowlist_var("AV.*").allowlist_var("CUDA.*")
        .derive_default(true).generate_comments(false)
        .generate().expect("native decoder bindings");
    bindings
        .write_to_file(PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("ffi.rs"))
        .unwrap();
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib").display()
    );
    for lib in ["avformat", "avcodec", "avutil", "cuda"] {
        println!("cargo:rustc-link-lib={lib}");
    }
    // Package layout: bin/fold-video-helper, lib/libav*.so.*; never an absolute build rpath.
    println!("cargo:rustc-link-arg-bin=fold-video-helper=-Wl,-rpath,$ORIGIN/../lib");
}
