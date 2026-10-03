use super::*;

#[test]
fn verified_copies_are_shared_and_held_bytes_survive_unlink() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.mp4");
    let info = crate::VideoInfo {
        width: 4,
        height: 4,
        rate: [24, 1],
        frames: 1,
    };
    let mut encoder = crate::Encoder::new(&path, &info, 1, Cancel::default()).unwrap();
    encoder.write_rgb(&[128; 4 * 4 * 3]).unwrap();
    encoder.finish().unwrap();
    let source = video::inspect(&path, &Cancel::default()).unwrap();
    let a = acquire(&source, &Cancel::default()).unwrap();
    let b = acquire(&source, &Cancel::default()).unwrap();
    assert!(
        Arc::ptr_eq(&a, &b),
        "workers must not retain duplicate whole-file copies"
    );
    let copy = a.file.path().to_owned();
    drop(a);
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        video::fingerprint(&copy, &Cancel::default()).unwrap(),
        source.fingerprint
    );
    assert!(
        acquire(&source, &Cancel::default()).is_err(),
        "fresh workers reject missing linked media"
    );
    drop(b);
    assert!(
        !copy.exists(),
        "the last immutable-source lease owns scratch cleanup"
    );
}
