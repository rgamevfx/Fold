use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{
    design,
    editing::{self, Edit},
    model::*,
    motion, tools,
};

#[test]
fn resize_animated_design_preserves_ids_motion_and_undo_baseline() {
    let id = ObjectId::new();
    let mut object = tools::rectangle("Card", [100., 80.], [300., 180.]);
    let mut curves: [fold_animation::Curve; 6] = Default::default();
    for (curve, value) in curves.iter_mut().zip([25., -10., 0., 1., 1., 0.5]) {
        curve.insert(Time::ZERO, value);
    }
    object.motion = Some(motion::Motion {
        spec: motion::Spec {
            source: "source".into(),
            controls: Default::default(),
            sample_rate: 120,
        },
        enabled: true,
        curves,
    });
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, object.clone())].into(),
        ..Default::default()
    };
    let edits = design::transform(
        &canvas,
        &[id],
        Time::ZERO,
        tools::Transform([2., 0., 0., 2., -125., -70.]),
        false,
    )
    .unwrap();
    let after = editing::apply(&canvas, &edits).unwrap();
    assert_eq!(after.objects[&id].motion, object.motion);
    assert_eq!(after.objects[&id].transform.x.id, object.transform.x.id);
    let bounds = design::bounds(&after, Time::ZERO).unwrap()[&id];
    assert_eq!(bounds.min, [125., 70.]);
    assert_eq!(bounds.size(), [400., 200.]);
    assert_eq!(canvas.objects[&id], object);
    assert_eq!(after.objects[&id].opacity(Time::ZERO), 0.5);
}
#[test]
fn text_frame_alignment_and_layer_locks_are_shared_edits() {
    let id = ObjectId::new();
    let mut object = tools::new_object("text").unwrap();
    if let Content::Text { text, layout, .. } = &mut object.content {
        *text = "Editable text wraps inside its frame".into();
        layout.width = 250.;
        layout.alignment = TextAlignment::Center;
    }
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let b = design::bounds(&canvas, Time::ZERO).unwrap()[&id];
    assert_eq!(b.size()[0], 250.);
    assert!(b.size()[1] > 64. * 1.2);
    let edits = design::align(&canvas, &[id], Time::ZERO, design::Align::Center, false).unwrap();
    let after = editing::apply(&canvas, &edits).unwrap();
    assert_eq!(
        design::bounds(&after, Time::ZERO).unwrap()[&id].center()[0],
        960.
    );
    let locked = editing::apply(
        &after,
        &[Edit::Lock {
            object: id,
            expected: false,
            value: true,
        }],
    )
    .unwrap();
    assert!(tools::translate(&locked, &[id], Time::ZERO, [10., 0.], false).is_err());
    let unlocked = editing::apply(
        &locked,
        &[Edit::Lock {
            object: id,
            expected: true,
            value: false,
        }],
    )
    .unwrap();
    assert_eq!(unlocked, after);
}
