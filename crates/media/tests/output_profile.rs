use fold_media::{Cancel, Encoder, VideoInfo};

#[test]
fn authored_outputs_are_not_restricted_to_the_mp4_profile() {
    let output = VideoInfo {
        width: 101,
        height: 99,
        rate: [24, 1],
        frames: 20_000,
    };
    output.validate().unwrap();
    assert_eq!(
        output.frame_at(output.time(19_999).unwrap()).unwrap(),
        19_999
    );
    let extreme = VideoInfo {
        rate: [1, u32::MAX],
        frames: u32::MAX,
        ..output
    };
    assert!(extreme.validate().is_err());
    assert!(extreme.time(u32::MAX).is_err());
}

#[test]
fn unsupported_delivery_is_rejected_before_starting_a_codec() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("unsupported.mp4");
    for output in [
        VideoInfo {
            width: 101,
            height: 100,
            rate: [24, 1],
            frames: 10,
        },
        VideoInfo {
            width: 100,
            height: 100,
            rate: [24, 1],
            frames: 20_000,
        },
    ] {
        let result = Encoder::new(&destination, &output, output.frames, Cancel::default());
        assert!(matches!(result, Err(error) if error.contains("MP4/H.264")));
        assert!(!destination.exists());
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
