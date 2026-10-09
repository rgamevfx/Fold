use fold_media::Cancel;
use fold_render::text::{Error, FontSource, LoadError, Service};
const FONT: &[u8] = include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf");

#[test]
fn verified_font_bytes_outlive_linked_file_and_feed_the_same_layout() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("font.ttf");
    std::fs::write(&path, FONT).unwrap();
    let packaged = FontSource::from_bytes(FONT.to_vec()).unwrap();
    let fingerprint = packaged.fingerprint();
    assert_eq!(
        fingerprint,
        "sha256:b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5"
    );
    let source = FontSource::read(&path, &fingerprint, &Cancel::default()).unwrap();
    assert_eq!(source.hash(), packaged.hash());
    assert_eq!(source.fingerprint(), fingerprint);
    assert_eq!(source.bytes(), FONT);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        FontSource::read(&path, &fingerprint, &Cancel::default()).unwrap_err(),
        LoadError::Missing
    );
    let layout = Service::from_font(source.clone(), "Noto Sans")
        .unwrap()
        .layout("MAKE IT MOVE O B 8", 140., None)
        .unwrap();
    assert!(std::ptr::eq(layout.font.as_ref(), source.bytes()));
    assert_eq!(layout.font_hash, source.hash());
    let reference = Service::from_font(packaged, "Noto Sans")
        .unwrap()
        .layout("MAKE IT MOVE O B 8", 140., None)
        .unwrap();
    assert_eq!(layout, reference);
    assert_eq!(
        Service::from_font(source, "Uninstalled substitute").err(),
        Some(Error::MissingFamily)
    );
}

#[test]
fn new_font_acquisitions_reject_changed_missing_invalid_and_oversized_sources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("font.ttf");
    let fingerprint = FontSource::from_bytes(FONT.to_vec()).unwrap().fingerprint();
    std::fs::write(&path, FONT).unwrap();
    let held = FontSource::read(&path, &fingerprint, &Cancel::default()).unwrap();
    let mut changed = FONT.to_vec();
    let end = changed.len() - 1;
    changed[end] ^= 1;
    std::fs::write(&path, changed).unwrap();
    assert_eq!(
        FontSource::read(&path, &fingerprint, &Cancel::default()).unwrap_err(),
        LoadError::Changed
    );
    assert_eq!(held.bytes(), FONT);
    for bad in ["", "sha256:123", &fingerprint.to_uppercase()] {
        assert_eq!(
            FontSource::read(&path, bad, &Cancel::default()).unwrap_err(),
            LoadError::InvalidFingerprint
        );
    }
    assert_eq!(
        FontSource::read(directory.path(), &fingerprint, &Cancel::default()).unwrap_err(),
        LoadError::InvalidFile
    );
    std::fs::File::create(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        FontSource::read(&path, &fingerprint, &Cancel::default()).unwrap_err(),
        LoadError::Budget
    );
    let cancel = Cancel::default();
    cancel.cancel();
    assert_eq!(
        FontSource::read(&path, &fingerprint, &cancel).unwrap_err(),
        LoadError::Cancelled
    );
    let invalid = FontSource::from_bytes(vec![0; 64]).unwrap();
    assert_eq!(
        Service::from_font(invalid, "Noto Sans").err(),
        Some(Error::InvalidFont)
    );
}
