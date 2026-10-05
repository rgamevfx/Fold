#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn bezier_drag_creates_symmetric_tangents_and_closed_segment() {
        let points = [([0., 0.], [10., 0.]), ([100., 100.], [0., 20.])];
        let segments = path_segments(&points, true);
        assert_eq!(
            segments,
            vec![
                Segment::Move([0., 0.]),
                Segment::Cubic([10., 0.], [100., 80.], [100., 100.]),
                Segment::Cubic([100., 120.], [-10., 0.], [0., 0.]),
                Segment::Close
            ]
        );
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::super::*;
    use crate::ui::tests::Host;
    fn frame(
        context: &mut imgui::Context,
        tools: &mut Tools,
        state: &mut State,
        host: &mut Host,
        p: [f32; 2],
        button: Option<bool>,
        escape: bool,
    ) {
        context.io_mut().add_mouse_pos_event(p);
        if let Some(down) = button {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, down);
        }
        context.io_mut().add_key_event(Key::Escape, escape);
        let ui = context.frame();
        ui.window("Tools")
            .position([0., 0.], imgui::Condition::Always)
            .size([700., 500.], imgui::Condition::Always)
            .build(|| {
                tools.draw(
                    ui,
                    state,
                    host,
                    ViewerRect {
                        editable: true,
                        image_current: true,
                        canvas_origin: [8., 28.],
                        canvas_size: [684., 464.],
                        origin: [50., 70.],
                        size: [640., 360.],
                        dimensions: [1280, 720],
                    },
                );
            });
        drop(context.render_legacy());
    }
    #[test]
    fn drawn_shape_previews_commits_once_and_escape_restores_document() {
        let _lock = crate::ui::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 600.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut host = Host::new();
        let mut state = State::default();
        state.create(&mut host);
        let before = host.project.snapshot();
        let mut tools = Tools {
            tool: Tool::Rectangle,
            ..Default::default()
        };
        for _ in 0..3 {
            frame(
                &mut context,
                &mut tools,
                &mut state,
                &mut host,
                [200., 200.],
                None,
                false,
            );
        }
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [200., 200.],
            Some(true),
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [300., 250.],
            None,
            false,
        );
        assert!(state.editing);
        assert_eq!(host.project.snapshot().revision(), before.revision());
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [300., 250.],
            Some(false),
            false,
        );
        assert_eq!(
            host.project.snapshot().revision().0,
            before.revision().0 + 1
        );
        let id = state.selected[0];
        assert_eq!(
            state
                .motion
                .as_ref()
                .unwrap()
                .graph
                .node(id)
                .unwrap()
                .inputs["width"],
            crate::graph::Input::Value(Datum::Scalar(200.))
        );
        host.project.undo().unwrap();
        state.sync(&mut host);
        assert!(
            state
                .motion
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .objects
                .is_empty()
        );
        tools.tool = Tool::Ellipse;
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [200., 200.],
            Some(true),
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [300., 250.],
            None,
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [300., 250.],
            None,
            true,
        );
        assert!(!state.editing);
        assert!(!tools.busy());
        assert!(
            state
                .motion
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .objects
                .is_empty()
        );
    }
    #[test]
    fn pen_drag_builds_curve_as_one_edit_and_cancels_without_a_partial_object() {
        let _lock = crate::ui::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 600.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut host = Host::new();
        let mut state = State::default();
        state.create(&mut host);
        let before = host.project.snapshot();
        let mut tools = Tools {
            tool: Tool::Pen,
            ..Default::default()
        };
        for _ in 0..3 {
            frame(
                &mut context,
                &mut tools,
                &mut state,
                &mut host,
                [200., 200.],
                None,
                false,
            );
        }
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [200., 200.],
            Some(true),
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [240., 200.],
            None,
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [240., 200.],
            Some(false),
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [350., 300.],
            Some(true),
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [350., 340.],
            None,
            false,
        );
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [350., 340.],
            Some(false),
            false,
        );
        assert_eq!(host.project.snapshot().revision(), before.revision());
        context.io_mut().add_key_event(Key::Enter, true);
        frame(
            &mut context,
            &mut tools,
            &mut state,
            &mut host,
            [350., 340.],
            None,
            false,
        );
        assert_eq!(
            host.project.snapshot().revision().0,
            before.revision().0 + 1
        );
        let path = state
            .motion
            .as_ref()
            .unwrap()
            .graph
            .node(state.selected[0])
            .unwrap()
            .settings::<crate::nodes::content::path::Settings>()
            .unwrap();
        assert_eq!(path.segments.len(), 2);
        assert_eq!(path.segments[0], Segment::Move([300., 260.]));
        let Segment::Cubic(a, b, c) = path.segments[1] else {
            panic!("Expected authored cubic");
        };
        for (actual, expected) in a
            .into_iter()
            .chain(b)
            .chain(c)
            .zip([380., 260., 600., 380., 600., 460.])
        {
            assert!((actual - expected).abs() < 1e-9);
        }
        host.project.undo().unwrap();
        state.sync(&mut host);
        assert!(
            state
                .motion
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .objects
                .is_empty()
        );
    }
}
