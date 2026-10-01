use super::interaction::*;
#[test]
fn zoom_keeps_pointer_time_and_handles_are_scale_aware() {
    let mut view = View {
        first: 100.0,
        pixels_per_frame: 5.0,
        scroll_y: 0.0,
    };
    let before = view.frame_at(400.0, 100.0);
    view.zoom(2.0, 400.0, 100.0);
    assert_eq!(view.frame_at(400.0, 100.0), before);
    let rect = Rect {
        min: [100.0, 0.0],
        max: [300.0, 50.0],
    };
    assert_eq!(handle(rect, [102.0, 20.0], 1.0), Handle::Left);
    assert_eq!(handle(rect, [298.0, 20.0], 1.0), Handle::Right);
    assert_eq!(handle(rect, [150.0, 20.0], 1.0), Handle::Move);
}
