//! The shelf and scene editor share the same intent-based action catalog.
use super::state::State;
use crate::authoring::scene::{self, assets};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::imgui::Ui;
pub(super) fn draw(ui: &Ui, state: &mut State, host: &mut dyn DesktopClient) {
    if let Some(_menu) = ui.begin_menu("Create") {
        if ui.menu_item("Badge / label") {
            state.change_scene(host, |m| assets::badge(m).map(Some));
        }
        for (label, kind) in [
            ("Text", "fold.motion.text"),
            ("Rectangle", "fold.motion.rectangle"),
            ("Ellipse", "fold.motion.ellipse"),
            ("Path", "fold.motion.path"),
        ] {
            if ui.menu_item(label) {
                state.change_scene(host, |m| scene::create_object(m, kind).map(Some));
            }
        }
        if ui.menu_item("Group") {
            state.change_scene(host, |m| scene::create_group(m).map(Some));
        }
        if ui.menu_item("Custom generator") {
            state.change_scene(host, |m| scene::create_procedural(m).map(Some));
        }
    }
    let selected = state
        .selected
        .first()
        .and_then(|id| {
            state
                .motion
                .as_ref()?
                .scene
                .as_ref()?
                .objects
                .iter()
                .find(|o| o.owns(*id))
        })
        .cloned();
    let Some(object) = selected else {
        ui.text_disabled("Select an object to arrange or animate");
        return;
    };
    let _disabled = ui.begin_disabled_with_cond(object.locked);
    if let Some(_menu) = ui.begin_menu("Arrange") {
        if ui.menu_item("Duplicate") {
            state.change_scene(host, |m| assets::duplicate(m, object.id).map(Some));
        }
        if let Some(_menu) = ui.begin_menu("Along path") {
            let paths: Vec<_> = state
                .motion
                .as_ref()
                .unwrap()
                .scene
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .filter(|o| {
                    o.id != object.id
                        && state
                            .motion
                            .as_ref()
                            .unwrap()
                            .graph
                            .node(o.source.node)
                            .is_ok_and(|n| n.kind == "fold.motion.path")
                })
                .map(|o| (o.id, o.name.clone()))
                .collect();
            if paths.is_empty() {
                ui.text_disabled("Draw a path first");
            }
            for (id, name) in paths {
                if ui.menu_item(name) {
                    state.change_scene(host, |m| assets::along_path(m, object.id, id).map(Some));
                }
            }
        }
        if ui.menu_item("Radial array") {
            state.change_scene(host, |m| {
                let group = scene::new_modifier_definition(m, true)?;
                scene::add_modifier(m, object.id, group).map(Some)
            });
        }
    }
    if let Some(_menu) = ui.begin_menu("Animate") {
        if state
            .motion
            .as_ref()
            .is_some_and(|m| assets::is_duplicator(m, object.id))
            && ui.menu_item("Position Oscillator")
        {
            state.change_scene(host, |m| {
                assets::attach_oscillator(m, object.id, "offset_y")?;
                Ok(Some(object.id))
            });
        }
        if ui.menu_item("Oscillate") {
            state.change_scene(host, |m| assets::oscillate(m, object.id).map(Some));
        }
    }
    if let Some(_menu) = ui.begin_menu("Style") {
        if state
            .motion
            .as_ref()
            .is_some_and(|m| assets::is_duplicator(m, object.id))
            && ui.menu_item("Color Ramp across copies")
        {
            state.change_scene(host, |m| {
                assets::attach_color_ramp(m, object.id, "color")?;
                Ok(Some(object.id))
            });
        }
        for (label, kind) in [
            ("Fill", "fold.motion.fill"),
            ("Stroke", "fold.motion.stroke"),
        ] {
            if ui.menu_item(label) {
                state.change_scene(host, |m| assets::apply_style(m, object.id, kind).map(Some));
            }
        }
    }
    if let Some(_menu) = ui.begin_menu("Connect") {
        let choices: Vec<_> = state
            .motion
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .filter(|o| o.id != object.id)
            .map(|o| (o.id, o.name.clone()))
            .collect();
        let time = host
            .state()
            .navigation
            .last()
            .map(|l| l.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        for kind in crate::scene::ConstraintKind::ALL {
            if let Some(_menu) = ui.begin_menu(kind.label()) {
                for (id, name) in &choices {
                    if ui.menu_item(format!("{name}##{id:?}")) {
                        state.change_scene(host, |m| {
                            scene::add_constraint(m, object.id, *id, kind, time, true)?;
                            Ok(Some(object.id))
                        });
                    }
                }
            }
        }
        if let Some(_menu) = ui.begin_menu("Mask with") {
            for (id, name) in &choices {
                if ui.menu_item(format!("{name}##{id:?}")) {
                    state.change_scene(host, |m| {
                        let o = m
                            .scene
                            .as_mut()
                            .unwrap()
                            .objects
                            .iter_mut()
                            .find(|o| o.id == object.id)
                            .unwrap();
                        o.masks
                            .push(crate::scene::Mask::new(*id, crate::scene::MaskSpace::World));
                        Ok(Some(object.id))
                    });
                }
            }
        }
    }
    if let Some(_menu) = ui.begin_menu("Custom tools") {
        if ui.menu_item("New modifier") {
            state.change_scene(host, |m| {
                let definition = scene::new_modifier_definition(m, false)?;
                scene::add_modifier(m, object.id, definition).map(Some)
            });
        }
        let definitions: Vec<_> = state
            .motion
            .as_ref()
            .unwrap()
            .groups
            .iter()
            .filter(|(_, g)| {
                g.inputs.iter().any(|p| p.id == "content") && g.outputs.contains_key("content")
            })
            .map(|(id, g)| (*id, g.name.clone()))
            .collect();
        for (id, name) in definitions {
            if ui.menu_item(format!("{name}##{id:?}")) {
                state.change_scene(host, |m| scene::add_modifier(m, object.id, id).map(Some));
            }
        }
    }
}
