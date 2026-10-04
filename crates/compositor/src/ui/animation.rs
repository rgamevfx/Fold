use super::state::Shared;
use crate::animation::path;
use fold_platform::desktop::{DesktopCommand, Selection, TransportAction};
use fold_ui::sdk::{
    ExtensionUi,
    animation_editor::{Channel, Context, Editor},
};

pub(super) fn draw(editor: &mut Editor, shared: &Shared, context: ExtensionUi<'_>) {
    let ExtensionUi { ui, host } = context;
    let mut state = shared.borrow_mut();
    state.sync(host);
    let Some(document) = state.document else {
        return;
    };
    let Some(graph) = state.graph.as_ref() else {
        return;
    };
    let info = graph.info.clone();
    let mut channels = vec![];
    for (index, node) in graph.nodes.iter().enumerate() {
        for property in node.parameters.animated_properties() {
            for (i, component) in property.channels.iter().enumerate() {
                let path = path(property.id, i);
                if let Some(curve) = node.animation.get(&path) {
                    channels.push(Channel {
                        object: node.id,
                        node_label: format!("{} {}", node.parameters.operator().id, index + 1),
                        property: property.id.into(),
                        property_label: property.label.into(),
                        component: (*component).into(),
                        path,
                        curve: curve.clone(),
                    });
                }
            }
        }
    }
    let selected = state.selected.clone();
    let time = host
        .state()
        .navigation
        .last()
        .map(|v| v.time)
        .unwrap_or(fold_foundation::Time::ZERO);
    let response = editor.draw(
        ui,
        &mut channels,
        Context {
            document,
            generation: state.generation,
            time,
            rate: info.rate,
            frames: info.frames,
            nodes: &selected,
            auto_key: &mut state.auto_key,
        },
    );
    if let Some(node) = response.select_node {
        state.selected = vec![node];
        host.command(DesktopCommand::Select(Selection {
            document: Some(document),
            objects: vec![node],
        }));
    }
    if let Some(time) = response.seek {
        // The shell routes this context's seek to its explicitly associated viewer.
        host.command(DesktopCommand::Transport(TransportAction::Jump(
            (fold_ui::sdk::time_view::frame(time, info.rate).round() as u32)
                .min(info.frames.saturating_sub(1)),
        )));
    }
    if response.edit.cancelled {
        state.cancel(host);
    } else {
        if response.edit.changed {
            if let Some(graph) = state.graph.as_mut() {
                for channel in channels {
                    if let Ok(node) = graph.node_mut(channel.object) {
                        if channel.curve.keys.is_empty() {
                            node.remove_animation_channel(&channel.path, time);
                        } else {
                            node.animation.insert(channel.path, channel.curve);
                        }
                    }
                }
            }
            state.preview(host);
        }
        if response.edit.finished && state.editing {
            state.commit(host);
        }
    }
    if !state.error.is_empty() {
        ui.text_wrapped(&state.error);
    }
}
