use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion,
    authoring::{
        node,
        scene::{self, assets},
    },
    evaluation::{Evaluator, Value},
    fields::Datum,
    geometry::{Content, Geometry},
    graph::Link,
};
fn fixture() -> (Motion, fold_foundation::ObjectId, fold_foundation::ObjectId) {
    let mut m = Motion::new_scene();
    let source = assets::badge(&mut m).unwrap();
    let path = scene::create_object(&mut m, "fold.motion.path").unwrap();
    node(&mut m, path).unwrap().settings["segments"] =
        serde_json::json!([{"Move":[0.,0.]},{"Line":[100.,0.]}]);
    let generator = assets::along_path(&mut m, source, path).unwrap();
    node(&mut m, generator)
        .unwrap()
        .set("closed", Datum::Bool(true));
    (m, source, generator)
}
fn copies(m: &Motion, id: fold_foundation::ObjectId, time: Time) -> Content {
    let Value::Content(c) = Evaluator::new(m, time)
        .unwrap()
        .resolve(&Link {
            node: id,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    c
}
#[test]
fn count_redistributes_and_travel_does_not_change_spacing() {
    let (mut m, _, id) = fixture();
    node(&mut m, id).unwrap().set("count", Datum::Scalar(4.));
    for duration in [5., 10., 20.] {
        node(&mut m, id)
            .unwrap()
            .set("duration", Datum::Scalar(duration));
        node(&mut m, id).unwrap().set("travel", Datum::Bool(true));
        let c = copies(&m, id, Time::new(1, 1).unwrap());
        assert_eq!(c.len(), 4);
        for pair in c.windows(2) {
            assert!(
                ((pair[1].transform[4] - pair[0].transform[4]).rem_euclid(100.) - 25.).abs() < 1e-9
            );
        }
    }
    node(&mut m, id).unwrap().set("count", Datum::Scalar(10.));
    assert_eq!(copies(&m, id, Time::ZERO).len(), 10);
    let a = copies(&m, id, Time::new(3, 2).unwrap());
    let _ = copies(&m, id, Time::ZERO);
    let b = copies(&m, id, Time::new(3, 2).unwrap());
    assert_eq!(
        a.iter().map(|e| (e.id, e.transform)).collect::<Vec<_>>(),
        b.iter().map(|e| (e.id, e.transform)).collect::<Vec<_>>()
    );
}
#[test]
fn copies_keep_path_attributes_and_optional_color_targets_only_text() {
    let (mut m, _, id) = fixture();
    node(&mut m, id).unwrap().set("enabled", Datum::Bool(true));
    let c = copies(&m, id, Time::ZERO);
    assert!(c.iter().all(|e| e.attributes.contains_key("color")));
    fn colors(c: &Content, paths: &mut Vec<[f64; 4]>, glyphs: &mut Vec<[f64; 4]>) {
        for e in c.iter() {
            match &e.geometry {
                Geometry::Group(c) | Geometry::Instance(c) => colors(c, paths, glyphs),
                Geometry::Path(_) => paths.push(e.fill.unwrap()),
                Geometry::Glyph { .. } => glyphs.push(e.fill.unwrap()),
                _ => {}
            }
        }
    }
    let mut paths = vec![];
    let mut glyphs = vec![];
    colors(&c, &mut paths, &mut glyphs);
    assert!(paths.iter().all(|c| *c == [0.065, 0.085, 0.14, 1.]));
    assert!(glyphs.windows(2).any(|p| p[0] != p[1]));
    let path = assets::reference(&m, id, "path").unwrap();
    node(&mut m, path).unwrap().settings["attributes"] = serde_json::json!({});
    node(&mut m, id).unwrap().set("enabled", Datum::Bool(false));
    assert_eq!(copies(&m, id, Time::ZERO).len(), 10);
}
#[test]
fn source_placement_does_not_offset_copies_and_background_follows_text() {
    let (mut m, source, id) = fixture();
    let before = copies(&m, id, Time::ZERO);
    let tr = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == source)
        .unwrap()
        .transform
        .unwrap();
    node(&mut m, tr)
        .unwrap()
        .set("translation", Datum::Vector([800., 300.]));
    let after = copies(&m, id, Time::ZERO);
    let a = fold_motion::geometry::drawings(&before, [1.; 2]).unwrap();
    let b = fold_motion::geometry::drawings(&after, [1.; 2]).unwrap();
    assert_eq!(
        a.iter().map(|d| d.transform).collect::<Vec<_>>(),
        b.iter().map(|d| d.transform).collect::<Vec<_>>()
    );
    let text = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.parent == Some(source) && o.name == "Text")
        .unwrap()
        .id;
    node(&mut m, text)
        .unwrap()
        .set("text", Datum::Text("LONGER LABEL".into()));
    let after = fold_motion::geometry::drawings(&copies(&m, id, Time::ZERO), [1.; 2]).unwrap();
    assert_ne!(a[0].path, after[0].path);
}
#[test]
fn assets_round_trip_and_reject_reference_cycles() {
    let (mut m, _, id) = fixture();
    let document = m.document(DocumentId::new()).unwrap();
    assert_eq!(Motion::from_document(&document).unwrap(), m);
    assets::set_reference(&mut m, id, "source", id, true).unwrap();
    assert!(m.validate().unwrap_err().contains("cyclic"));
}
