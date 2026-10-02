use fold_media::{
    Cancel,
    ingest::{SourceProfile, inspect_linked},
    thumbnail::{SIZE, thumbnail},
};
#[test]
fn ppm_thumbnail_is_bounded_srgb_and_rejects_stale_or_unverified_sources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("swatch.ppm");
    std::fs::write(&path, b"P6\n1 1\n255\n\x80\x40\x20").unwrap();
    let source = inspect_linked(&path, &Cancel::default()).unwrap();
    let pixels = thumbnail(
        &path,
        &source.fingerprint,
        &source.metadata.profile,
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(pixels.len(), (SIZE[0] * SIZE[1] * 4) as usize);
    let center = ((36 * SIZE[0] + 64) * 4) as usize;
    assert_eq!(&pixels[center..center + 4], &[128, 64, 32, 255]);
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(
        thumbnail(
            &path,
            &source.fingerprint,
            &source.metadata.profile,
            &cancel
        )
        .is_err()
    );
    assert!(
        thumbnail(
            &path,
            &source.fingerprint,
            &SourceProfile::Ppm { dimensions: [2, 2] },
            &Cancel::default()
        )
        .is_err()
    );
    std::fs::write(&path, b"P6\n1 1\n255\n\xff\x00\x00").unwrap();
    assert!(
        thumbnail(
            &path,
            &source.fingerprint,
            &source.metadata.profile,
            &Cancel::default()
        )
        .is_err()
    );
    std::fs::remove_file(&path).unwrap();
    assert!(
        thumbnail(
            &path,
            &source.fingerprint,
            &source.metadata.profile,
            &Cancel::default()
        )
        .is_err()
    );
}
#[test]
fn wave_preview_has_fixed_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-t",
                "0.1",
                "-c:a",
                "pcm_s16le"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let source = inspect_linked(&path, &Cancel::default()).unwrap();
    let pixels = thumbnail(
        &path,
        &source.fingerprint,
        &source.metadata.profile,
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(pixels.len(), 128 * 72 * 4);
    assert!(pixels.chunks_exact(4).any(|p| p[1] > p[0]));
}
