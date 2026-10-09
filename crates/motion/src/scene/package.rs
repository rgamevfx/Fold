use super::*;
use crate::{
    model::{Definition, Instance, Node, Output, Reference},
    registry,
};
use fold_platform::packages::*;
use fold_project::{EditBatch, Mutation, Snapshot};

fn graph(
    canvas: &Canvas,
    name: &str,
    nodes: Vec<Node>,
    output_node: ObjectId,
    ty: Type,
) -> Construction {
    let definition = Definition {
        id: ObjectId::new(),
        name: name.into(),
        inputs: vec![],
        nodes,
        instances: vec![],
        outputs: vec![Output {
            id: ObjectId::new(),
            name: "main".into(),
            ty,
            value: Reference::Node { node: output_node }.into(),
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    };
    Construction {
        seed: 0,
        canvas: canvas.clone(),
        root: Instance {
            id: ObjectId::new(),
            name: name.into(),
            definition: definition.id,
            clock: Clock::default(),
            overrides: Default::default(),
            extensions: Default::default(),
        },
        definitions: vec![definition],
        presentation: Default::default(),
        extensions: Default::default(),
    }
}
fn node(
    key: &str,
    mode: &str,
    name: &str,
    parameters: BTreeMap<String, Binding>,
) -> Result<Node, String> {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)?.node(name, parameters)
}
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
pub fn object(canvas: &Canvas, kind: &str) -> Result<Object, String> {
    let (name, nodes, last, ty) = match kind {
        "camera" => {
            let n = node("camera", "perspective", "Camera", Default::default())?;
            ("Camera", vec![n.clone()], n.id, Type::Camera)
        }
        "light" => {
            let n = node("light", "directional", "Light", Default::default())?;
            ("Light", vec![n.clone()], n.id, Type::Light)
        }
        "box" => {
            let mesh = node(
                "mesh",
                "box",
                "Box",
                [(
                    "size".into(),
                    Binding::Literal {
                        value: Literal::Vector {
                            value: vec![1.; 3],
                            unit: Unit::Meters,
                            space: Space::World,
                        },
                    },
                )]
                .into(),
            )?;
            let material = node(
                "material",
                "lit",
                "Material",
                [("geometry".into(), link(mesh.id))].into(),
            )?;
            let id = material.id;
            ("Box", vec![mesh, material], id, Type::Geometry)
        }
        "sphere" => {
            let mesh = node("mesh", "sphere", "Sphere", Default::default())?;
            let material = node(
                "material",
                "lit",
                "Material",
                [("geometry".into(), link(mesh.id))].into(),
            )?;
            let id = material.id;
            ("Sphere", vec![mesh, material], id, Type::Geometry)
        }
        "text" => {
            let text = node("text", "packaged", "Text", Default::default())?;
            let outline = node(
                "outline_text",
                "curves",
                "Outline",
                [("text".into(), link(text.id))].into(),
            )?;
            let extrude = node(
                "extrude",
                "straight_text",
                "Extrude",
                [("profile".into(), link(outline.id))].into(),
            )?;
            let material = node(
                "material",
                "lit",
                "Material",
                [("geometry".into(), link(extrude.id))].into(),
            )?;
            let id = material.id;
            (
                "Text",
                vec![text, outline, extrude, material],
                id,
                Type::Geometry,
            )
        }
        _ => return Err("Unknown scene object kind".into()),
    };
    let graph = graph(canvas, name, nodes, last, ty);
    graph.validate_semantics()?;
    let output = graph.definitions[0].outputs[0].id;
    Ok(Object::new(name, graph, output))
}
pub fn create(id: DocumentId, canvas: Canvas) -> Result<Document, String> {
    let camera = object(&canvas, "camera")?;
    let active_camera = camera.id;
    let objects = vec![camera, object(&canvas, "light")?, object(&canvas, "box")?];
    Scene {
        canvas,
        output: ObjectId::new(),
        active_camera,
        objects,
        extensions: Default::default(),
    }
    .document(id)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub scene: Scene,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphEditArgs {
    pub document: DocumentId,
    pub object: ObjectId,
    pub construction: Construction,
}
/// Trusted local artist command; never part of provider author operations.
pub fn unlock_graph(
    original: &Document,
    object: ObjectId,
    targets: &[ObjectId],
) -> Result<Document, String> {
    let mut scene = Scene::from_document(original)?;
    if scene.locked(object) {
        return Err("Unlock the scene object and its parents first".into());
    }
    let target = scene
        .objects
        .iter_mut()
        .find(|o| o.id == object)
        .ok_or("Missing scene object")?;
    target.graph = crate::editing::unlock_construction_for_artist(&target.graph, targets)
        .map_err(|e| format!("{e:?}"))?;
    let mut after = scene.document(original.id)?;
    after.revision = original.revision;
    after.extensions = original.extensions.clone();
    Ok(after)
}

fn edit(
    snapshot: &Snapshot,
    args: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: EditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing scene")?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(args.scene.replace(original)?)],
    })
}
fn graph_edit(
    snapshot: &Snapshot,
    args: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: GraphEditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing scene")?;
    let mut scene = Scene::from_document(original)?;
    args.construction.validate_semantics()?;
    scene
        .objects
        .iter_mut()
        .find(|o| o.id == args.object)
        .ok_or("Missing scene object")?
        .graph = args.construction;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(scene.replace(original)?)],
    })
}
pub struct Documents;
impl DocumentProvider for Documents {
    fn unlock_for_artist(
        &self,
        document: &Document,
        object: Option<ObjectId>,
        targets: &[ObjectId],
    ) -> Result<Document, String> {
        unlock_graph(document, object.ok_or("Select an object graph")?, targets)
    }
    fn animation_channels(
        &self,
        d: &Document,
        object: Option<ObjectId>,
    ) -> Result<Vec<fold_platform::animation::Channel>, String> {
        animation::channels(&Scene::from_document(d)?, object)
    }
    fn edit_animation(
        &self,
        d: &Document,
        channels: &[fold_platform::animation::Channel],
        _: Time,
    ) -> Result<Document, String> {
        animation::replace(&Scene::from_document(d)?, channels)?.replace(d)
    }
    fn hierarchy(&self, d: &Document) -> Result<fold_platform::hierarchy::Hierarchy, String> {
        let scene = Scene::from_document(d)?;
        let rows = scene
            .objects
            .iter()
            .map(|o| {
                let ty = &o
                    .graph
                    .definition(o.graph.root.definition)?
                    .outputs
                    .iter()
                    .find(|p| p.id == o.output)
                    .ok_or("Missing object output")?
                    .ty;
                Ok(fold_platform::hierarchy::Row {
                    id: o.id,
                    parent: o.parent,
                    name: o.name.clone(),
                    kind: match ty {
                        Type::Camera => "Camera",
                        Type::Light => "Light",
                        _ => "Object",
                    }
                    .into(),
                    visible: o.visible,
                    locked: scene.locked(o.id),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(fold_platform::hierarchy::Hierarchy {
            rows,
            create: vec![
                "box".into(),
                "sphere".into(),
                "text".into(),
                "camera".into(),
                "light".into(),
            ],
        })
    }
    fn edit_hierarchy(
        &self,
        d: &Document,
        edits: &[fold_platform::hierarchy::Edit],
    ) -> Result<Document, String> {
        use fold_platform::hierarchy::Edit;
        let mut scene = Scene::from_document(d)?;
        for edit in edits {
            match edit {
                Edit::Create { kind } => scene.objects.push(object(&scene.canvas, kind)?),
                Edit::ActiveCamera { object } => scene.active_camera = *object,
                Edit::Remove { object } => {
                    let ids = scene
                        .objects
                        .iter()
                        .filter(|o| {
                            scene
                                .ancestors(o.id)
                                .is_ok_and(|ancestors| ancestors.iter().any(|p| p.id == *object))
                        })
                        .map(|o| o.id)
                        .collect::<BTreeSet<_>>();
                    if ids.is_empty() {
                        return Err("Missing object".into());
                    }
                    scene.objects.retain(|o| !ids.contains(&o.id));
                }
                Edit::Lock { object, value } => {
                    // Unlocking is an explicit local artist action; the public author API cannot issue it.
                    let target = scene
                        .objects
                        .iter_mut()
                        .find(|o| o.id == *object)
                        .ok_or("Missing object")?;
                    target.locked = *value;
                }
                Edit::Rename { object, name } => {
                    scene
                        .objects
                        .iter_mut()
                        .find(|o| o.id == *object)
                        .ok_or("Missing object")?
                        .name = name.clone()
                }
                Edit::Reparent { object, parent } => {
                    scene
                        .objects
                        .iter_mut()
                        .find(|o| o.id == *object)
                        .ok_or("Missing object")?
                        .parent = *parent
                }
                Edit::Visibility { object, value } => {
                    scene
                        .objects
                        .iter_mut()
                        .find(|o| o.id == *object)
                        .ok_or("Missing object")?
                        .visible = *value
                }
            }
        }
        // Clearing a direct lock must not authorize any other edit in the same transaction.
        if let [
            Edit::Lock {
                object,
                value: false,
            },
        ] = edits
        {
            let before = Scene::from_document(d)?;
            let target = before.object(*object)?;
            if target.parent.is_some_and(|p| before.locked(p)) {
                return Err("Unlock the parent first".into());
            }
            let mut document = scene.document(d.id)?;
            document.revision = d.revision;
            document.extensions = d.extensions.clone();
            Ok(document)
        } else {
            scene.replace(d)
        }
    }

    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn schema(&self) -> u32 {
        SCHEMA
    }
    fn validate(&self, d: &Document) -> Result<(), String> {
        Scene::from_document(d).map(|_| ())
    }
    fn browser_kind(&self) -> Option<fold_platform::browser::DocumentKind> {
        Some(fold_platform::browser::DocumentKind {
            type_id: TYPE,
            title: "Scene",
        })
    }
    fn create(&self, id: DocumentId) -> Result<Document, String> {
        create(
            id,
            Canvas {
                width: 1920,
                height: 1080,
                rate: [24, 1],
                start: Time::ZERO,
                end: Time::new(5, 1).map_err(|e| e.to_string())?,
                extensions: Default::default(),
            },
        )
    }
    fn inspect(&self, d: &Document) -> Result<serde_json::Value, String> {
        let scene = Scene::from_document(d)?;
        serde_json::to_value(scene).map_err(|e| e.to_string())
    }
    fn authoring_catalog(&self) -> serde_json::Value {
        serde_json::json!({"operations":["replace_scene","object_source"],"object_kinds":["camera","light","box","sphere","text"],"scene_type":TYPE,"schema":SCHEMA})
    }
    fn author(&self, d: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        let mut current = d.clone();
        for edit in edits {
            let mut scene = Scene::from_document(&current)?;
            match edit.get("op").and_then(|v| v.as_str()) {
                Some("replace_scene") => {
                    let expected: Scene = serde_json::from_value(
                        edit.get("expected")
                            .ok_or("Expected scene is required")?
                            .clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    if expected != scene {
                        return Err("Scene changed; inspect and rebase".into());
                    }
                    scene = serde_json::from_value(
                        edit.get("value").ok_or("Missing scene value")?.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Some("object_source") => {
                    let id: ObjectId =
                        serde_json::from_value(edit.get("object").ok_or("Missing object")?.clone())
                            .map_err(|e| e.to_string())?;
                    let target = scene
                        .objects
                        .iter_mut()
                        .find(|o| o.id == id)
                        .ok_or("Missing object")?;
                    let expected: Construction = serde_json::from_value(
                        edit.get("expected")
                            .ok_or("Expected graph is required")?
                            .clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    if target.graph != expected {
                        return Err("Object graph changed; inspect and rebase".into());
                    }
                    let source = edit
                        .get("source")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing graph source")?;
                    target.graph = crate::source::parse(source)
                        .map_err(|e| e.message)?
                        .construction;
                    target.graph.validate_semantics()?;
                }
                _ => return Err("Unsupported scene operation".into()),
            }
            current = scene.replace(&current)?;
        }
        Ok(current)
    }
}
pub fn commands() -> Vec<CommandRegistration> {
    vec![
        CommandRegistration {
            id: EDIT,
            title: "Edit scene",
            execution: Execution::Immediate,
            handler: edit,
        },
        CommandRegistration {
            id: GRAPH_EDIT,
            title: "Edit object graph",
            execution: Execution::Immediate,
            handler: graph_edit,
        },
    ]
}
