//! Node-free scene authoring with animation in the same surface.
use super::{animation::Animation, state::Shared};
use crate::authoring::scene as a;
use fold_ui::sdk::{
    ExtensionUi,
    toolbar::{icon_menu, menu_button},
};

pub(super) fn draw(shared: &Shared, animation: &mut Animation, context: ExtensionUi<'_>) {
    let ExtensionUi { ui, host } = context;
    {
        let mut state = shared.borrow_mut();
        state.sync(host);
        if let Some(_menu) = menu_button(ui, "MoGraph", "Motion document") {
            if ui.menu_item("New motion document") {
                state.create(host);
            }
            if ui.menu_item("Show output") {
                state.view(host);
            }
            if ui.menu_item("Render visible scene") {
                state.change_scene(host, |m| {
                    if let Some(scene) = &mut m.scene {
                        scene.output = None;
                    }
                    Ok(None)
                });
            }
        }
        if state.motion.is_none() {
            ui.text_disabled("Create or open a Motion document.");
            return;
        }
        ui.same_line();
        if let Some(_menu) = menu_button(ui, "Tools", "Create, arrange, animate, style and connect")
        {
            super::tool_actions::draw(ui, &mut state, host);
        }
        let selected = state.selected.first().copied();
        let object = state
            .motion
            .as_ref()
            .and_then(|m| m.scene.as_ref())
            .and_then(|s| {
                s.objects
                    .iter()
                    .find(|o| selected.is_some_and(|id| o.owns(id)))
            })
            .cloned();
        ui.same_line();
        let object_menu = if ui.content_region_avail()[0] < 75. {
            icon_menu(ui, "object-actions", "Object organization and modifiers")
        } else {
            menu_button(ui, "Object", "Object organization and modifiers")
        };
        if let Some(_menu) = object_menu {
            if let Some(object) = object {
                if ui.menu_item("Use as composition output") {
                    state.change_scene(host, |m| {
                        m.scene.as_mut().unwrap().output = Some(object.id);
                        Ok(Some(object.id))
                    });
                }
                if ui.menu_item(if object.visible {
                    "Hide from render"
                } else {
                    "Show in render"
                }) {
                    state.change_scene(host, |m| {
                        m.scene
                            .as_mut()
                            .unwrap()
                            .objects
                            .iter_mut()
                            .find(|o| o.id == object.id)
                            .unwrap()
                            .visible = !object.visible;
                        Ok(Some(object.id))
                    });
                }
                if ui.menu_item(if object.locked { "Unlock" } else { "Lock" }) {
                    state.change_scene(host, |m| {
                        m.scene
                            .as_mut()
                            .unwrap()
                            .objects
                            .iter_mut()
                            .find(|o| o.id == object.id)
                            .unwrap()
                            .locked = !object.locked;
                        Ok(Some(object.id))
                    });
                }
                let _disabled = ui.begin_disabled_with_cond(object.locked);
                if let Some(_menu) = ui.begin_menu("Move into group") {
                    let groups: Vec<_> = state
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
                                    .is_ok_and(|n| n.kind == "fold.motion.scene_children")
                        })
                        .map(|o| (o.id, o.name.clone()))
                        .collect();
                    let time = host
                        .state()
                        .navigation
                        .last()
                        .map(|n| n.time)
                        .unwrap_or(fold_foundation::Time::ZERO);
                    if ui.menu_item("Scene root") {
                        state.change_scene(host, |m| {
                            a::place_object(m, object.id, None, None, time)?;
                            Ok(Some(object.id))
                        });
                    }
                    for (id, label) in groups {
                        let _id = ui.push_id(&format!("{id:?}"));
                        if ui.menu_item(label) {
                            state.change_scene(host, |m| {
                                a::place_object(m, object.id, Some(id), None, time)?;
                                Ok(Some(object.id))
                            });
                        }
                    }
                }
                for (label, delta) in [("Move up", -1isize), ("Move down", 1)] {
                    if ui.menu_item(label) {
                        state.change_scene(host, |m| {
                            let objects = &mut m.scene.as_mut().unwrap().objects;
                            let index = objects.iter().position(|o| o.id == object.id).unwrap();
                            let siblings: Vec<_> = objects
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| o.parent == object.parent)
                                .map(|(i, _)| i)
                                .collect();
                            let position = siblings.iter().position(|&i| i == index).unwrap();
                            let to = siblings[position
                                .saturating_add_signed(delta)
                                .min(siblings.len() - 1)];
                            objects.swap(index, to);
                            Ok(Some(object.id))
                        });
                    }
                }
                if ui.menu_item("Remove from scene") {
                    state.change_scene(host, |m| {
                        m.scene
                            .as_mut()
                            .unwrap()
                            .objects
                            .retain(|o| o.id != object.id);
                        Ok(None)
                    });
                    if state.error.is_empty() {
                        state.selected.clear();
                        host.command(fold_platform::desktop::DesktopCommand::Select(
                            fold_platform::desktop::Selection {
                                document: state.document,
                                objects: vec![],
                            },
                        ));
                    }
                }
            } else {
                ui.text_disabled("Select an object.");
            }
        }
        if state.motion.as_ref().is_some_and(|m| m.scene.is_none()) {
            ui.same_line();
            if ui.button("Organize as scene") {
                state.change_scene(host, |m| {
                    a::ensure_scene(m)?;
                    Ok(None)
                });
            }
        }
    }
    animation.draw(shared, ExtensionUi { ui, host });
}
