use super::*;
use crate::sdk::typography::{TextRole, push_role};

#[test]
fn appearance_changes_reach_existing_and_new_graph_consumers() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = Typography::install(&mut context);
    let existing = fonts.clone();
    let mut settings = Settings::new(fonts.clone(), None);
    let mut value = Appearance::default();
    value.application_text = 20.;
    value.graph.labels = 24.;
    value.graph.titles = 30.;
    value.colors.text = [0.8, 0.9, 1.];
    settings.edit(value);
    settings.prepare_frame(&mut context);
    assert_eq!(existing.appearance(), value);
    assert_eq!(fonts.clone().appearance(), value);
    assert_eq!(context.style().font_size_base(), 20.);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([800., 600.]);
    context.io_mut().set_delta_time(1. / 60.);
    let ui = context.frame();
    ui.window("roles").build(|| {
        assert_eq!(ui.style_color(imgui::StyleColor::Text), [0.8, 0.9, 1., 1.]);
        let _role = push_role(ui, TextRole::Title, existing.appearance().graph);
        assert_eq!(ui.current_font_size(), 30.);
    });
    drop(context.render_legacy());
    let mut invalid = value;
    invalid.graph.labels = f32::NAN;
    assert!(fonts.set_appearance(invalid).is_err());
    assert_eq!(fonts.appearance(), value);
    settings.edit(Appearance::default());
    assert_eq!(existing.appearance(), Appearance::default());
}

#[test]
fn settings_draw_at_normal_and_narrow_sizes_without_changes() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut settings = Settings::new(fonts.clone(), None);
    settings.open = true;
    for width in [640., 440.] {
        for _ in 0..3 {
            settings.prepare_frame(&mut context);
            let ui = context.frame();
            ui.window("Settings##fold.settings")
                .size([width, 480.], imgui::Condition::Always)
                .build(|| {});
            settings.draw(ui);
            assert!(context.render_legacy().total_vtx_count() > 100);
        }
    }
    assert_eq!(fonts.appearance(), Appearance::default());
    assert!(!settings.edited);
}
