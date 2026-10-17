use super::paint::draw as paint;
use super::{Shared, state::State};
use fold_foundation::{DocumentId, ObjectId};
use fold_motion_canvas::{
    editing::{self, Edit},
    model::{Cap, Content, ControlValue, Join},
};
use fold_ui::sdk::{
    self, EditResponse, ExtensionUi, Panel, TextDraft, UiId, animated_property::AnimatedProperty,
    imgui::TreeNodeFlags,
};
use std::collections::BTreeMap;

pub(super) struct Inspector {
    pub state: Shared,
    pub embedded: State,
    pub sources: BTreeMap<(DocumentId, ObjectId), TextDraft>,
    pub names: BTreeMap<(DocumentId, ObjectId), TextDraft>,
    pub motions: BTreeMap<
        (DocumentId, ObjectId),
        (
            fold_motion_canvas::motion::Spec,
            fold_motion_canvas::motion::Spec,
        ),
    >,
}
impl Panel for Inspector {
    fn draw_source_controls(
        &mut self,
        context: ExtensionUi<'_>,
        source: Option<sdk::SourceControls>,
    ) {
        let ExtensionUi { ui, host } = context;
        let Some(source) = source else {
            self.embedded.cancel(host);
            return;
        };
        let target = host
            .snapshot()
            .and_then(|s| s.state().documents.get(&source.document).cloned())
            .and_then(|d| fold_motion_canvas::model::Canvas::from_document(&d).ok());
        let Some(canvas) = target else {
            self.embedded.cancel(host);
            return;
        };
        let root = canvas
            .roots
            .first()
            .copied()
            .filter(|_| canvas.roots.len() == 1);
        let Some(id) =
            root.filter(|id| matches!(canvas.objects[id].content, Content::Script { .. }))
        else {
            self.embedded.cancel(host);
            return;
        };
        self.embedded
            .sync_target(host, Some(source.document), vec![id], source.time);
        let state = &mut self.embedded;
        if !source.editable || editing::editable(&canvas, id).is_err() {
            state.cancel(host);
        }
        let _scope = ui.push_id(&format!("source-controls-{}", source.document));
        let _disabled = ui.begin_disabled_with_cond(
            !source.editable
                || editing::editable(&canvas, id).is_err()
                || host.state().playing
                || state.busy(),
        );
        let Some(mut object) = state.canvas().and_then(|c| c.objects.get(&id)).cloned() else {
            return;
        };
        ui.separator();
        ui.text(&object.name);
        let mut response = EditResponse::default();
        if let Content::Script { controls, .. } = &mut object.content {
            super::controls::draw(ui, host, source.time, controls, &mut response);
        }
        respond(state, ui, host, id, &object, response);
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }

    fn id(&self) -> &'static str {
        crate::package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(fold_motion_canvas::TYPE)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.state.borrow_mut();
        state.sync(host);
        let Some(document) = state.document else {
            return;
        };
        let Some(id) = state
            .selected
            .first()
            .copied()
            .filter(|_| state.selected.len() == 1)
        else {
            ui.text_disabled("Select an object.");
            return;
        };
        let Some(mut object) = state.canvas().and_then(|c| c.objects.get(&id)).cloned() else {
            return;
        };
        let locked = state
            .before
            .as_ref()
            .is_none_or(|c| editing::editable(c, id).is_err());
        let _scope = UiId {
            package: fold_motion_canvas::PACKAGE,
            panel: crate::package::INSPECTOR,
            document,
            object: id,
        }
        .scope(ui);
        let _disabled = ui.begin_disabled_with_cond(locked || host.state().playing || state.busy());
        let mut response = EditResponse::default();
        let name = self
            .names
            .entry((document, id))
            .or_insert_with(|| TextDraft::new(&object.name));
        name.sync(&object.name);
        ui.input_text("Name", &mut name.text).build();
        if ui.is_item_active() && ui.is_key_pressed(sdk::imgui::Key::Escape) {
            name.reset(&object.name);
        } else if ui.is_item_deactivated_after_edit() && name.text != object.name {
            if name.conflicts(&object.name) {
                state.error = "The object name changed. Your draft is retained.".into();
            } else {
                object.name = name.text.clone();
                response.changed = true;
                response.finished = true;
            }
        }
        let scalars: Vec<_> = object
            .named_controls()
            .into_iter()
            .filter(|(_, value)| !matches!(&object.content, Content::Script {controls,..} if controls.values().any(|c|matches!(&c.value,ControlValue::Number(n) if n.id==value.id))))
            .map(|(label, value)| (label.to_owned(), value.clone()))
            .collect();
        if ui.collapsing_header(
            "Properties",
            if matches!(object.content, Content::Script { .. }) {
                TreeNodeFlags::empty()
            } else {
                TreeNodeFlags::DEFAULT_OPEN
            },
        ) {
            for (label, original) in scalars {
                let range = if original.id == object.transform.opacity.id {
                    Some([0., 1.])
                } else if [
                    "Width",
                    "Height",
                    "Radius",
                    "Radius X",
                    "Radius Y",
                    "Text Size",
                    "Stroke Width",
                ]
                .contains(&label.as_str())
                {
                    Some([0., f64::MAX])
                } else {
                    None
                };
                let unit = if original.id == object.transform.rotation.id {
                    "°"
                } else if [
                    "Position X",
                    "Position Y",
                    "Anchor X",
                    "Anchor Y",
                    "Width",
                    "Height",
                    "Radius",
                    "Radius X",
                    "Radius Y",
                    "Text Size",
                    "Stroke Width",
                ]
                .contains(&label.as_str())
                {
                    "px"
                } else {
                    ""
                };
                let value = object.scalar_mut(original.id).expect("native scalar");
                super::controls::scalar(
                    ui,
                    host,
                    state.time,
                    &label,
                    value,
                    range,
                    unit,
                    &mut response,
                );
            }
        }
        if !matches!(object.content, Content::Image { .. })
            && ui.collapsing_header(
                "Paint",
                if matches!(object.content, Content::Script { .. }) {
                    TreeNodeFlags::empty()
                } else {
                    TreeNodeFlags::DEFAULT_OPEN
                },
            )
        {
            paint(
                ui,
                "Fill",
                Some(host),
                &mut object.paint.fill,
                &mut response,
            );
            paint(
                ui,
                "Stroke",
                Some(host),
                &mut object.paint.stroke,
                &mut response,
            );
            if object.paint.stroke.is_some() {
                let caps = [Cap::Butt, Cap::Round, Cap::Square];
                let mut cap = caps
                    .iter()
                    .position(|value| *value == object.paint.cap)
                    .unwrap();
                let changed = ui.combo_simple_string("Cap", &mut cap, &["Butt", "Round", "Square"]);
                response.item(ui, changed);
                object.paint.cap = caps[cap];
                let joins = [Join::Miter, Join::Round, Join::Bevel];
                let mut join = joins
                    .iter()
                    .position(|value| *value == object.paint.join)
                    .unwrap();
                let changed =
                    ui.combo_simple_string("Join", &mut join, &["Miter", "Round", "Bevel"]);
                response.item(ui, changed);
                object.paint.join = joins[join];
                if object.paint.join == Join::Miter {
                    sdk::NumericProperty {
                        id: "miter-limit",
                        label: "Miter limit",
                        unit: "",
                        speed: 0.05,
                        range: Some([1., 1000.]),
                        default: Some(4.),
                    }
                    .draw(ui, &mut object.paint.miter_limit, &mut response);
                }
            }
            if matches!(object.content, Content::Path { .. }) && object.paint.fill.is_some() {
                let changed = ui.checkbox("Even-odd fill", &mut object.paint.even_odd);
                response.item(ui, changed);
                if ui.is_item_hovered() {
                    ui.tooltip_text(
                        "Use alternating filled and empty regions where contours overlap.",
                    );
                }
            }
        }
        match &mut object.content {
            Content::Image { asset, fit, .. } => {
                super::image_control::draw(ui, host, "Image", asset, &mut response);
                let fits = [
                    fold_motion_canvas::model::ImageFit::Contain,
                    fold_motion_canvas::model::ImageFit::Cover,
                    fold_motion_canvas::model::ImageFit::Stretch,
                ];
                let mut index = fits.iter().position(|value| value == fit).unwrap();
                let changed =
                    ui.combo_simple_string("Fit", &mut index, &["Contain", "Cover", "Stretch"]);
                response.item(ui, changed);
                *fit = fits[index];
            }
            Content::Path { contours, .. } => {
                if let Some((target, point_id)) = state.point
                    && target == id
                    && let Some(point) = contours
                        .iter_mut()
                        .flat_map(|c| &mut c.points)
                        .find(|p| p.id == point_id)
                    && ui.collapsing_header("Anchor", TreeNodeFlags::DEFAULT_OPEN)
                {
                    for (role, label, base, track) in [
                        (
                            "position",
                            "Position",
                            &mut point.position,
                            &mut point.position_track,
                        ),
                        (
                            "incoming",
                            "Incoming offset",
                            &mut point.incoming,
                            &mut point.incoming_track,
                        ),
                        (
                            "outgoing",
                            "Outgoing offset",
                            &mut point.outgoing,
                            &mut point.outgoing_track,
                        ),
                    ] {
                        if let Err(error) = point_property(
                            ui,
                            &format!("{point_id}/{role}"),
                            label,
                            base,
                            track,
                            state.time,
                            host.state().auto_key,
                            &mut response,
                        ) {
                            state.error = error;
                            response.cancelled = true;
                        }
                    }
                    ui.text_disabled("Path-local coordinates; handles are offsets.");
                }
                if ui.collapsing_header("Contours", TreeNodeFlags::empty()) {
                    for (index, contour) in contours.iter_mut().enumerate() {
                        let _id = ui.push_id(&contour.id.to_string());
                        if ui.button("Reverse direction") {
                            contour.reverse();
                            response.changed = true;
                            response.finished = true;
                        }
                        if ui.is_item_hovered() {
                            ui.tooltip_text("Reverse anchor order and swap Bézier handles. This changes contour winding and can change holes with nonzero fill.");
                        }
                        let changed = ui
                            .checkbox(format!("Contour {} closed", index + 1), &mut contour.closed);
                        response.item(ui, changed);
                        if ui.is_item_hovered() {
                            ui.tooltip_text(format!(
                                "Contour {}: connect the last anchor to the first.",
                                index + 1
                            ));
                        }
                    }
                }
            }
            Content::Text {
                text,
                font_family,
                layout,
                ..
            } => {
                let changed = ui
                    .input_text_multiline(
                        "Text",
                        text,
                        [ui.content_region_avail()[0], ui.frame_height() * 4.],
                    )
                    .build();
                response.item(ui, changed);
                ui.text_disabled(format!("Font: {font_family}"));
                for (label, value) in [
                    ("Text box width", &mut layout.width),
                    ("Line spacing", &mut layout.line_height),
                    ("Tracking", &mut layout.tracking),
                ] {
                    let changed = ui.input_double(label, value);
                    response.item(ui, changed);
                }
                let options = [
                    fold_motion_canvas::model::TextAlignment::Left,
                    fold_motion_canvas::model::TextAlignment::Center,
                    fold_motion_canvas::model::TextAlignment::Right,
                ];
                let mut index = options.iter().position(|a| *a == layout.alignment).unwrap();
                if ui.combo_simple_string("Alignment", &mut index, &["Left", "Center", "Right"]) {
                    layout.alignment = options[index];
                    response.changed = true;
                    response.finished = true;
                }
            }
            Content::Script { controls, .. } => {
                super::controls::draw(ui, host, state.time, controls, &mut response);
            }
            _ => {}
        }
        if let Some(motion) = &mut object.motion {
            if ui.collapsing_header("Motion behavior", TreeNodeFlags::DEFAULT_OPEN) {
                if ui.checkbox("Enabled", &mut motion.enabled) {
                    response.changed = true;
                    response.finished = true;
                }
                let draft = self
                    .motions
                    .entry((document, id))
                    .or_insert_with(|| (motion.spec.clone(), motion.spec.clone()));
                if draft.0 != motion.spec {
                    *draft = (motion.spec.clone(), motion.spec.clone());
                }
                for (name, value) in &mut draft.1.controls {
                    ui.input_double(name, value);
                }
                if ui.collapsing_header("Behavior code", TreeNodeFlags::empty()) {
                    ui.input_text_multiline(
                        "##motion-source",
                        &mut draft.1.source,
                        [ui.content_region_avail()[0], ui.frame_height() * 8.],
                    )
                    .build();
                    let mut rate = draft.1.sample_rate as i32;
                    if ui.input_int("Samples / second", &mut rate) && (1..=480).contains(&rate) {
                        draft.1.sample_rate = rate as u32;
                    }
                }
                let changed = draft.1 != motion.spec;
                let _disabled = ui.begin_disabled_with_cond(
                    !changed || state.editing || !host.supports_extension(crate::package::ANIMATE),
                );
                if ui.button("Update motion") {
                    if let Err(error) = state.animate(host, id, draft.1.clone()) {
                        state.error = error;
                    }
                }
                drop(_disabled);
                if changed {
                    ui.same_line();
                    if ui.button("Discard changes") {
                        draft.1 = motion.spec.clone();
                    }
                }
            }
        }
        respond(&mut state, ui, host, id, &object, response);
        if let Content::Script { source, .. } = &object.content {
            if let Some(draft) = self.sources.get_mut(&(document, id)) {
                draft.sync(source);
            }
            if host.supports_extension(crate::package::CONVERT) {
                let source_dirty = self
                    .sources
                    .get(&(document, id))
                    .is_some_and(|draft| draft.text != *source);
                let disabled = ui.begin_disabled_with_cond(
                    state.editing || host.state().playing || source_dirty,
                );
                if ui.button("Convert to editable") {
                    if let Err(error) = state.convert(host, id) {
                        state.error = error;
                    }
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Capture script output at the current time as editable objects. Text becomes outlines. Undo restores the script.");
                }
                drop(disabled);
            }
            if ui.collapsing_header("Source", TreeNodeFlags::empty()) {
                let draft = self
                    .sources
                    .entry((document, id))
                    .or_insert_with(|| TextDraft::new(source));
                draft.sync(source);
                ui.input_text_multiline(
                    "##fframes-source",
                    &mut draft.text,
                    [ui.content_region_avail()[0], ui.frame_height() * 12.],
                )
                .build();
                let conflict = draft.conflicts(source);
                if conflict {
                    ui.text_wrapped("The script changed. Your draft is retained. Reload to use the current source.");
                }
                let _disabled =
                    ui.begin_disabled_with_cond(conflict || state.editing || draft.text == *source);
                if ui.button("Apply") {
                    let result = apply_source(&mut state, host, id, &draft.text);
                    if let Err(error) = result {
                        state.error = error;
                    }
                }
                drop(_disabled);
                ui.same_line();
                if ui.button("Reload") {
                    draft.reset(source);
                }
            }
        }
        drop(_disabled);
        if state.conversion_pending() {
            let _cancel_disabled = ui.begin_disabled_with_cond(state.cancelling());
            if ui.button("Cancel conversion") {
                state.cancel_conversion(host);
            }
        }
        if locked {
            ui.text_disabled("This object is locked.");
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
fn apply_source(
    state: &mut State,
    host: &mut dyn fold_platform::desktop::DesktopClient,
    id: ObjectId,
    source: &str,
) -> Result<(), String> {
    let expected = state
        .before
        .as_ref()
        .and_then(|c| c.objects.get(&id))
        .ok_or("Missing script")?
        .clone();
    let mut object = expected.clone();
    let Content::Script { source: target, .. } = &mut object.content else {
        return Err("Object is not a script".into());
    };
    *target = source.into();
    state.preview(
        host,
        vec![Edit::Replace {
            id,
            expected: Box::new(expected),
            object: Box::new(object),
        }],
    )?;
    state.commit(host)
}

fn point_property(
    ui: &fold_ui::sdk::imgui::Ui,
    id: &str,
    label: &str,
    base: &mut [f64; 2],
    track: &mut Option<fold_graph::binding::Track>,
    time: fold_foundation::Time,
    auto_key: bool,
    response: &mut EditResponse,
) -> Result<(), String> {
    let mut channels = BTreeMap::new();
    if let Some(track) = track {
        for (index, (_, curve)) in fold_graph::curves::curves(track)?.into_iter().enumerate() {
            channels.insert(format!("{id}.{index}"), curve);
        }
    }
    let original = channels.clone();
    let mut value = *base;
    AnimatedProperty {
        id,
        label,
        components: &["X", "Y"],
        unit: "px",
        range: None,
        color: None,
    }
    .draw(ui, &mut value, &mut channels, time, auto_key, response);
    if original == channels && value == *base {
        return Ok(());
    }
    let curves = [
        channels.remove(&format!("{id}.0")),
        channels.remove(&format!("{id}.1")),
    ];
    let (value, next) = edited_point_property(*base, track.as_ref(), value, curves, time)?;
    *base = value;
    *track = next;
    Ok(())
}

pub(super) fn edited_point_property(
    base: [f64; 2],
    track: Option<&fold_graph::binding::Track>,
    value: [f64; 2],
    curves: [Option<fold_animation::Curve>; 2],
    time: fold_foundation::Time,
) -> Result<([f64; 2], Option<fold_graph::binding::Track>), String> {
    use fold_graph::{
        binding::Track,
        types::{Literal, Space, Unit},
    };
    if track.is_some() && curves.iter().any(Option::is_none) {
        let Literal::Vector { value, .. } = track.unwrap().sample(time)? else {
            return Err("Point property requires a vector".into());
        };
        return Ok(([value[0], value[1]], None));
    }
    if curves.iter().all(Option::is_none) {
        return Ok((value, None));
    }
    let seed;
    let track = match track {
        Some(track) => track,
        None => {
            seed = Track::constant(
                Literal::Vector {
                    value: base.to_vec(),
                    unit: Unit::Canvas,
                    space: Space::Screen,
                },
                time,
            )?;
            &seed
        }
    };
    let edits = ["X", "Y"]
        .into_iter()
        .zip(curves)
        .filter_map(|(name, curve)| curve.map(|curve| (name.into(), curve)))
        .collect::<Vec<_>>();
    Ok((value, Some(fold_graph::curves::replace(track, &edits)?)))
}

fn respond(
    state: &mut State,
    ui: &sdk::imgui::Ui,
    host: &mut dyn fold_platform::desktop::DesktopClient,
    id: ObjectId,
    object: &fold_motion_canvas::model::Object,
    mut response: EditResponse,
) {
    let invalid_property = response.cancelled;
    response.cancel_on_escape(ui, state.editing);
    response.cancelled |= invalid_property;
    if response.began && state.editing {
        state.cancel(host);
    }
    if response.cancelled {
        state.cancel(host);
    } else {
        let result = (|| -> Result<(), String> {
            if response.changed {
                let expected = state
                    .before
                    .as_ref()
                    .and_then(|c| c.objects.get(&id))
                    .ok_or("Missing Motion object")?
                    .clone();
                state.preview(
                    host,
                    vec![Edit::Replace {
                        id,
                        expected: Box::new(expected),
                        object: Box::new(object.clone()),
                    }],
                )?;
            }
            if response.finished {
                state.commit(host)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            state.cancel(host);
            state.error = error;
        }
    }
}
