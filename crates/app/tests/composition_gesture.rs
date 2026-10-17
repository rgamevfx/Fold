#![cfg(all(feature = "desktop", feature = "native-probe"))]

use fold_app::session::Session;
use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection, ViewLocation};
use fold_project::{EditBatch, Mutation, Project};
use fold_ui::sdk::{ExtensionUi, PanelRegistry, imgui};

#[test]
fn seeking_during_a_keyframe_drag_cancels_the_real_session_preview() {
    let document = DocumentId::new();
    let mut composition = Composition::default();
    composition.info.rate = [24, 1];
    composition.info.frames = 96;
    let mut layer = Layer::new("Animated layer", None, composition.info.clone()).unwrap();
    let object = layer.id;
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 1.);
    layer.animation.insert("scene.opacity.0".into(), curve);
    composition.layers.push(layer);
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                composition.document(document).unwrap(),
            )],
        })
        .unwrap();
    let mut host = Session::new(project);
    host.command(DesktopCommand::Navigate(ViewLocation {
        document,
        time: Time::ZERO,
        label: "Composition".into(),
    }));
    host.command(DesktopCommand::Select(Selection {
        document: Some(document),
        objects: vec![object],
    }));
    let original = host.snapshot().unwrap();
    let mut registry = PanelRegistry::new(&fold_app::packages::builtins());
    fold_composition::ui::register(&mut registry).unwrap();
    let mut panels = registry
        .probe_editor(fold_composition::COMPOSITION)
        .unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    context.io_mut().set_config_input_trickle_event_queue(false);
    let mut draw = |context: &mut imgui::Context, host: &mut Session| {
        let mut key = [0.; 2];
        let ui = context.frame();
        ui.window("Composition")
            .position([0.; 2], imgui::Condition::Always)
            .size([900., 500.], imgui::Condition::Always)
            .build(|| {
                panels.editor.draw(ExtensionUi { ui, host });
                let min = ui.item_rect_min();
                let max = ui.item_rect_max();
                let row = ui.text_line_height() + 6.;
                key = [
                    min[0] + 210. + (max[0] - min[0] - 210. - 8.) / 4.,
                    min[1] + row * 2.5,
                ];
            });
        drop(context.render_legacy());
        key
    };
    for _ in 0..3 {
        draw(&mut context, &mut host);
    }
    let key = draw(&mut context, &mut host);
    context.io_mut().add_mouse_pos_event(key);
    draw(&mut context, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    draw(&mut context, &mut host);
    context.io_mut().add_mouse_pos_event([key[0] + 40., key[1]]);
    draw(&mut context, &mut host);
    assert!(
        host.state().transient,
        "the key drag must create a real preview"
    );
    assert_eq!(host.snapshot().unwrap().state(), original.state());
    host.command(DesktopCommand::Seek(12));
    draw(&mut context, &mut host);
    assert!(
        !host.state().transient,
        "seek must cancel the keyframe preview"
    );
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    draw(&mut context, &mut host);
    assert_eq!(host.snapshot().unwrap().revision(), original.revision());
    assert_eq!(host.snapshot().unwrap().state(), original.state());

    // A new drag at the new playhead still previews and commits one Undo entry.
    context.io_mut().add_mouse_pos_event(key);
    draw(&mut context, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    draw(&mut context, &mut host);
    context.io_mut().add_mouse_pos_event([key[0] + 40., key[1]]);
    draw(&mut context, &mut host);
    assert!(host.state().transient);
    let preview = host.state().content.clone();
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    draw(&mut context, &mut host);
    assert!(!host.state().transient);
    assert_eq!(host.state().content, preview);
    let committed = host.snapshot().unwrap();
    assert_ne!(committed.revision(), original.revision());
    let edited = Composition::from_document(&committed.state().documents[&document]).unwrap();
    let original_keys = &composition.layers[0].animation["scene.opacity.0"].keys;
    let edited_keys = &edited.layers[0].animation["scene.opacity.0"].keys;
    assert_eq!(edited_keys[1].id, original_keys[1].id);
    assert!(edited_keys[1].time > original_keys[1].time);
    host.command(DesktopCommand::Undo);
    assert_eq!(
        host.snapshot().unwrap().state().documents,
        original.state().documents
    );
    host.command(DesktopCommand::Redo);
    assert_eq!(
        host.snapshot().unwrap().state().documents,
        committed.state().documents
    );
}
