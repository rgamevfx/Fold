use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::types::{Alpha, Encoding, Literal, Space, Type, Unit};
use fold_media::Cancel;
use fold_motion_canvas::{
    PACKAGE, SCHEMA, TYPE,
    editing::{self, Edit},
    model::{Canvas, Content, ControlValue, Object, Scalar},
};
use fold_platform::{animation, browser, packages::*, parameters};
use fold_project::{Document, EditBatch, Snapshot};
use serde::{Deserialize, Serialize};

pub const EDIT: &str = "fold.motion.edit";
pub const CONVERT: &str = "fold.motion.convert";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertArgs {
    pub document: DocumentId,
    pub object: ObjectId,
    pub time: Time,
}
pub const PANEL: &str = "fold.motion.canvas.editor";
pub const INSPECTOR: &str = "fold.motion.canvas.inspector";

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub edits: Vec<Edit>,
}

/// Document state and shared artist/agent capabilities.
#[derive(Default)]
pub struct Provider {
    proposals: crate::proposals::Proposals,
}
impl Provider {
    pub fn new(conversion: crate::conversion::Service) -> Self {
        Self {
            proposals: crate::proposals::Proposals {
                conversion: Some(std::sync::Arc::new(conversion)),
            },
        }
    }
}

impl DocumentProvider for Provider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn schema(&self) -> u32 {
        SCHEMA
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Canvas::from_document(document).map(|_| ())
    }
    fn scoped_proposals_capability(
        &self,
    ) -> Option<&dyn fold_platform::proposals::ScopedProposals> {
        Some(&self.proposals)
    }
    fn authoring_capability(&self) -> Option<&dyn Authoring> {
        Some(self)
    }
    fn parameters_capability(&self) -> Option<&dyn Parameters> {
        Some(self)
    }
    fn animation_capability(&self) -> Option<&dyn Animation> {
        Some(self)
    }
    fn creation_capability(&self) -> Option<&dyn Creation> {
        Some(self)
    }
    fn relink_capability(&self) -> Option<&dyn Relink> {
        Some(self)
    }
    fn placement_capability(&self) -> Option<&dyn Placement> {
        Some(self)
    }
    fn browser_kind_capability(&self) -> Option<&dyn BrowserKind> {
        Some(self)
    }
}

impl Relink for Provider {
    fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        let canvas = Canvas::from_document(document)?;
        if !canvas.assets().contains(&asset) {
            return Err("Image asset is not referenced by this Motion document".into());
        }
        crate::image_source::dimensions(&metadata.profile).map(|_| ())
    }
}
impl Placement for Provider {
    fn placed_objects(&self, before: &Document, after: &Document) -> Result<Vec<ObjectId>, String> {
        let before = Canvas::from_document(before)?;
        let after = Canvas::from_document(after)?;
        let mut objects = Vec::new();
        let mut pending: Vec<_> = after.roots.iter().rev().copied().collect();
        while let Some(id) = pending.pop() {
            if !before.objects.contains_key(&id) {
                objects.push(id);
            }
            if let Content::Group { children, .. } = &after.objects[&id].content {
                pending.extend(children.iter().rev().copied());
            }
        }
        Ok(objects)
    }

    fn supports_canvas_position(&self) -> bool {
        true
    }
    fn place(
        &self,
        snapshot: &Snapshot,
        request: &browser::Placement,
        _info: Option<fold_media::VideoInfo>,
    ) -> Result<EditBatch, String> {
        crate::placement::place(snapshot, request)
    }
}
impl Creation for Provider {
    fn create(&self, id: DocumentId) -> Result<Document, String> {
        Canvas::default().document(id)
    }
}
impl BrowserKind for Provider {
    fn browser_kind(&self) -> Option<browser::DocumentKind> {
        Some(browser::DocumentKind {
            type_id: TYPE,
            title: "Motion",
        })
    }
}
impl Authoring for Provider {
    fn authoring_catalog(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "edits": schemars::schema_for!(Edit),
            "document": schemars::schema_for!(Canvas),
            "script_entry": "pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String>",
            "time": "Exact rational seconds; controls are runtime values",
            "path_animation": {
                "owner": "Native point ID; preserve point, track and key IDs when editing",
                "coordinates": "Position is path-local; incoming and outgoing are offsets from the sampled position",
                "tracks": ["position_track", "incoming_track", "outgoing_track"],
                "values": "Replacement Vec2 in Canvas units and Screen space; X/Y share native key times",
                "editing": "Use guarded object replacement from inspection. Clearing a track requires an explicit retained static pose."
            },
            "guards": "Use expected values from inspect. Preserve IDs and extension data."
        })
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        serde_json::to_value(Canvas::from_document(document)?).map_err(|e| e.to_string())
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        let edits = edits
            .iter()
            .map(|edit| serde_json::from_value(edit.clone()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<Edit>, _>>()?;
        editing::apply(&Canvas::from_document(document)?, &edits)?.replace_document(document)
    }
    fn author_cancellable(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
        cancel: &Cancel,
    ) -> Result<Document, String> {
        cancel.check()?;
        let result = self.author(document, edits)?;
        cancel.check()?;
        Ok(result)
    }
}

impl Parameters for Provider {
    fn parameters(&self, document: &Document) -> Result<Vec<parameters::Descriptor>, String> {
        let canvas = Canvas::from_document(document)?;
        let mut descriptors = vec![];
        for (&id, object) in &canvas.objects {
            let editable = editing::editable(&canvas, id).is_ok();
            for (label, scalar) in object.named_controls() {
                let control_locked = matches!(&object.content, Content::Script { controls, .. }
                    if controls.values().any(|c| c.protected && matches!(&c.value, ControlValue::Number(n) if n.id == scalar.id)));
                descriptors.push(descriptor(
                    document.id,
                    id,
                    scalar.id,
                    &object.name,
                    label,
                    scalar_literal(object, scalar, Time::ZERO).ty(),
                    editable && !scalar.protected && !control_locked,
                ));
            }
            if let Content::Path { contours, .. } = &object.content {
                for (contour_index, contour) in contours.iter().enumerate() {
                    for (point_index, point) in contour.points.iter().enumerate() {
                        let mut entry = descriptor(
                            document.id,
                            id,
                            point.id,
                            &object.name,
                            "Position",
                            anchor_literal(point, Time::ZERO)?.ty(),
                            editable,
                        );
                        entry.path = vec![
                            object.name.clone(),
                            format!("Contour {}", contour_index + 1),
                            format!("Anchor {}", point_index + 1),
                            "Position".into(),
                        ];
                        descriptors.extend(parameters::with_components(entry));
                        for (role, label, value) in [
                            (
                                "incoming",
                                "Incoming handle",
                                point.incoming_at(Time::ZERO)?,
                            ),
                            (
                                "outgoing",
                                "Outgoing handle",
                                point.outgoing_at(Time::ZERO)?,
                            ),
                        ] {
                            let mut entry = descriptor(
                                document.id,
                                id,
                                point.id,
                                &object.name,
                                label,
                                Literal::Vector {
                                    value: value.to_vec(),
                                    unit: Unit::Canvas,
                                    space: Space::Screen,
                                }
                                .ty(),
                                editable,
                            );
                            entry.target.parameter = parameters::Key::Property {
                                name: format!("{}/{role}", point.id),
                            };
                            entry.path = vec![
                                object.name.clone(),
                                format!("Contour {}", contour_index + 1),
                                format!("Anchor {}", point_index + 1),
                                label.into(),
                            ];
                            descriptors.extend(parameters::with_components(entry));
                        }
                    }
                }
            }
            if let Content::Script { controls, .. } = &object.content {
                for (&control_id, control) in controls {
                    // Numeric controls use the scalar ID, also used by animation.
                    if matches!(control.value, ControlValue::Number(_)) {
                        continue;
                    }
                    let value = control_literal(&control.value, Time::ZERO);
                    descriptors.extend(parameters::with_components(descriptor(
                        document.id,
                        id,
                        control_id,
                        &object.name,
                        &control.name,
                        value.ty(),
                        editable && !control.protected,
                    )));
                }
            }
        }
        Ok(descriptors)
    }
    fn sample_parameter(
        &self,
        document: &Document,
        target: &parameters::Target,
        time: Time,
        cancel: &Cancel,
    ) -> Result<Literal, String> {
        cancel.check()?;
        if target.document != document.id
            || target.placement.is_some()
            || !target.occurrence.is_empty()
        {
            return Err("Parameter target must be local to this Motion document".into());
        }
        let canvas = Canvas::from_document(document)?;
        let object = canvas
            .objects
            .get(&target.object.ok_or("Missing Motion object target")?)
            .ok_or("Missing Motion object")?;
        if let Some(property) = target.parameter.property() {
            let (point_id, role) = property
                .split_once('/')
                .ok_or("Invalid Motion handle target")?;
            let point_id: ObjectId =
                serde_json::from_value(serde_json::Value::String(point_id.into()))
                    .map_err(|_| "Invalid Motion point ID")?;
            let Content::Path { contours, .. } = &object.content else {
                return Err("Handle target requires a path".into());
            };
            let point = contours
                .iter()
                .flat_map(|c| &c.points)
                .find(|p| p.id == point_id)
                .ok_or("Missing Motion point")?;
            let value = match role {
                "incoming" => point.incoming_at(time)?,
                "outgoing" => point.outgoing_at(time)?,
                _ => return Err("Unknown Motion handle property".into()),
            };
            return parameters::select_component(
                Literal::Vector {
                    value: value.to_vec(),
                    unit: Unit::Canvas,
                    space: Space::Screen,
                },
                target,
            );
        }
        let id = target
            .parameter
            .native()
            .ok_or("Motion parameters require stable IDs")?;
        let value = if let Some(scalar) = object
            .named_controls()
            .into_iter()
            .map(|(_, s)| s)
            .find(|s| s.id == id)
        {
            scalar_literal(object, scalar, time)
        } else if let Content::Path { contours, .. } = &object.content {
            let point = contours
                .iter()
                .flat_map(|c| &c.points)
                .find(|p| p.id == id)
                .ok_or("Missing Motion anchor")?;
            anchor_literal(point, time)?
        } else if let Content::Script { controls, .. } = &object.content {
            let control = controls.get(&id).ok_or("Missing Motion control")?;
            if matches!(control.value, ControlValue::Number(_)) {
                return Err("Numeric control targets use the scalar ID".into());
            }
            control_literal(&control.value, time)
        } else {
            return Err("Missing Motion parameter".into());
        };
        value.validate()?;
        parameters::select_component(value, target)
    }
    fn parameter_time(&self, document: &Document, output_time: Time) -> Result<Time, String> {
        Canvas::from_document(document)?;
        Ok(output_time)
    }
}

impl Animation for Provider {
    fn animation_channels(
        &self,
        document: &Document,
        object: Option<ObjectId>,
    ) -> Result<Vec<animation::Channel>, String> {
        let canvas = Canvas::from_document(document)?;
        if let Some(id) = object
            && !canvas.objects.contains_key(&id)
        {
            return Err("Missing Motion object".into());
        }
        let mut result: Vec<_> = canvas
            .objects
            .iter()
            .filter(|(id, _)| object.is_none_or(|selected| selected == **id))
            .flat_map(|(&id, object)| {
                object
                    .named_controls()
                    .into_iter()
                    .filter_map(move |(label, scalar)| {
                        scalar.animation.as_ref().map(|curve| animation::Channel {
                            target: animation::Target {
                                group: None,
                                object: id,
                                path: scalar.id.to_string(),
                            },
                            node_label: object.name.clone(),
                            property: scalar.id.to_string(),
                            property_label: label.into(),
                            component: String::new(),
                            curve: curve.clone(),
                        })
                    })
            })
            .collect();
        for (&id, native) in &canvas.objects {
            if object.is_some_and(|selected| selected != id) {
                continue;
            }
            if let Content::Path { contours, .. } = &native.content {
                for (c, contour) in contours.iter().enumerate() {
                    for (p, point) in contour.points.iter().enumerate() {
                        for (role, label, track) in [
                            ("position", "Position", &point.position_track),
                            ("incoming", "Incoming handle", &point.incoming_track),
                            ("outgoing", "Outgoing handle", &point.outgoing_track),
                        ] {
                            let Some(track) = track else { continue };
                            let property = if role == "position" {
                                point.id.to_string()
                            } else {
                                format!("{}/{role}", point.id)
                            };
                            for (component, curve) in fold_graph::curves::curves(track)? {
                                result.push(animation::Channel {
                                    target: animation::Target {
                                        group: None,
                                        object: id,
                                        path: format!("{property}/{component}"),
                                    },
                                    node_label: native.name.clone(),
                                    property: property.clone(),
                                    property_label: format!(
                                        "Contour {} · Anchor {} · {}",
                                        c + 1,
                                        p + 1,
                                        label
                                    ),
                                    component,
                                    curve,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(result)
    }
    fn edit_animation(
        &self,
        document: &Document,
        channels: &[animation::Channel],
        time: Time,
    ) -> Result<Document, String> {
        let canvas = Canvas::from_document(document)?;
        let available = self
            .animation_channels(document, None)?
            .into_iter()
            .map(|c| (c.target.clone(), c))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut anchors = std::collections::BTreeMap::<
            (ObjectId, ObjectId),
            std::collections::BTreeMap<String, Vec<(String, fold_animation::Curve)>>,
        >::new();
        let mut anchor_targets = std::collections::BTreeSet::new();
        let mut edits = vec![];
        let mut targets = std::collections::BTreeSet::new();
        for channel in channels {
            if !channel.component.is_empty() {
                let original = available
                    .get(&channel.target)
                    .ok_or("Missing Motion anchor channel")?;
                if channel.property != original.property
                    || channel.component != original.component
                    || !anchor_targets.insert(channel.target.clone())
                {
                    return Err(
                        "Anchor animation edit has an inconsistent or repeated target".into(),
                    );
                }
                let (point, role) = channel
                    .property
                    .split_once('/')
                    .unwrap_or((&channel.property, "position"));
                let id = serde_json::from_value(serde_json::Value::String(point.into()))
                    .map_err(|_| "Invalid Motion anchor ID")?;
                anchors
                    .entry((channel.target.object, id))
                    .or_default()
                    .entry(role.into())
                    .or_default()
                    .push((channel.component.clone(), channel.curve.clone()));
                continue;
            }
            if channel.target.group.is_some() || !channel.component.is_empty() {
                return Err("Motion scalar channels do not have groups or components".into());
            }
            let id: ObjectId =
                serde_json::from_value(serde_json::Value::String(channel.target.path.clone()))
                    .map_err(|_| "Invalid Motion scalar ID")?;
            if channel.property != channel.target.path
                || !targets.insert((channel.target.object, id))
            {
                return Err("Animation edit has an inconsistent or repeated target".into());
            }
            let object = canvas
                .objects
                .get(&channel.target.object)
                .ok_or("Missing Motion object")?;
            let expected = object
                .named_controls()
                .into_iter()
                .map(|(_, s)| s)
                .find(|s| s.id == id)
                .ok_or("Missing Motion parameter")?
                .clone();
            let mut value = expected.clone();
            value.animation = if channel.curve.keys.is_empty() {
                None
            } else {
                Some(channel.curve.clone())
            };
            edits.push(Edit::Parameter {
                object: channel.target.object,
                expected,
                value,
            });
        }
        for ((object, id), components) in anchors {
            let Content::Path { contours, .. } = &canvas.objects[&object].content else {
                return Err("Object is not a path".into());
            };
            let expected = contours
                .iter()
                .flat_map(|c| &c.points)
                .find(|p| p.id == id)
                .ok_or("Missing Motion anchor")?;
            let mut value = expected.clone();
            for (role, components) in components {
                let sampled = match role.as_str() {
                    "position" => expected.position_at(time)?,
                    "incoming" => expected.incoming_at(time)?,
                    "outgoing" => expected.outgoing_at(time)?,
                    _ => return Err("Unknown Motion point animation property".into()),
                };
                let (base, slot) = match role.as_str() {
                    "position" => (&mut value.position, &mut value.position_track),
                    "incoming" => (&mut value.incoming, &mut value.incoming_track),
                    "outgoing" => (&mut value.outgoing, &mut value.outgoing_track),
                    _ => unreachable!(),
                };
                let track = slot
                    .as_ref()
                    .ok_or("Point property has no animation track")?;
                if components.iter().all(|(_, c)| c.keys.is_empty()) {
                    *base = sampled;
                    *slot = None;
                } else {
                    *slot = Some(fold_graph::curves::replace(track, &components)?);
                }
            }
            edits.push(Edit::Point {
                object,
                expected: expected.clone(),
                value,
            });
        }
        editing::apply(&canvas, &edits)?.replace_document(document)
    }
}

fn descriptor(
    document: DocumentId,
    object: ObjectId,
    id: ObjectId,
    owner: &str,
    name: &str,
    value_type: Type,
    editable: bool,
) -> parameters::Descriptor {
    parameters::Descriptor {
        target: parameters::Target {
            document,
            placement: None,
            object: Some(object),
            occurrence: vec![],
            parameter: id.into(),
            component: None,
        },
        path: vec![owner.into(), name.into()],
        value_type: Some(value_type),
        editable,
    }
}
fn anchor_literal(point: &fold_motion_canvas::model::Point, time: Time) -> Result<Literal, String> {
    Ok(Literal::Vector {
        value: point.position_at(time)?.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    })
}
// Units come from native field identity, never artist-controlled labels.
fn scalar_literal(object: &Object, scalar: &Scalar, time: Time) -> Literal {
    let id = scalar.id;
    let distance = id == object.transform.x.id
        || id == object.transform.y.id
        || id == object.paint.stroke_width.id
        || match &object.content {
            Content::Rectangle {
                width,
                height,
                radius,
                ..
            } => [width.id, height.id, radius.id].contains(&id),
            Content::Ellipse {
                radius_x, radius_y, ..
            } => [radius_x.id, radius_y.id].contains(&id),
            Content::Text { size, .. } => size.id == id,
            Content::Image { width, height, .. } => [width.id, height.id].contains(&id),
            _ => false,
        };
    if distance {
        Literal::Distance {
            value: scalar.at(time),
            unit: Unit::Canvas,
        }
    } else {
        Literal::Scalar {
            value: scalar.at(time),
        }
    }
}

fn control_literal(value: &ControlValue, time: Time) -> Literal {
    match value {
        ControlValue::Number(value) => Literal::Scalar {
            value: value.at(time),
        },
        ControlValue::Text(value) => Literal::Text {
            value: value.clone(),
        },
        ControlValue::Image(asset) => Literal::AssetRef {
            value: fold_graph::types::AssetReference::Resource(*asset),
        },
        ControlValue::Boolean(value) => Literal::Bool { value: *value },
        ControlValue::Color(value) => Literal::Color {
            value: fold_color::authored::from_picker(*value, false),
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        },
    }
}

pub fn edit(snapshot: &Snapshot, arguments: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let arguments: EditArgs = serde_json::from_slice(arguments).map_err(|e| e.to_string())?;
    let batch = editing::stage(snapshot, arguments.document, &arguments.edits)?;
    cancel.check()?;
    Ok(batch)
}

pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    register_with(registry, Provider::default(), vec![], None)
}

/// Assemble document capabilities and the configured renderer in one package.
pub fn register_with(
    registry: &mut PackageRegistry,
    provider: Provider,
    video: Vec<Box<dyn fold_platform::VideoProvider>>,
    convert: Option<CommandHandler>,
) -> Result<(), String> {
    let mut commands = vec![CommandRegistration {
        id: EDIT,
        title: "Edit Motion",
        execution: Execution::Worker,
        handler: edit,
    }];
    if let Some(handler) = convert {
        commands.push(CommandRegistration {
            id: CONVERT,
            title: "Convert to editable",
            execution: Execution::Worker,
            handler,
        });
    }
    registry.register(Contributions {
        manifest: Manifest {
            id: PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: &[
                PanelDescriptor {
                    id: PANEL,
                    title: "Motion",
                    placement: PanelPlacement::Editor,
                },
                PanelDescriptor {
                    id: INSPECTOR,
                    title: "Motion",
                    placement: PanelPlacement::Inspector,
                },
            ],
            build: "motion-context-1",
        },
        documents: vec![Box::new(provider)],
        commands,
        video,
        audio: vec![],
    })
}
