//! Native document/desktop registration. Rendering is qualified separately in M4.
use crate::{binding::Binding, editing, model::*, types::*};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::packages::*;
use fold_project::{Document, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const EDIT: &str = "fold.motion.native.edit";
pub const PANEL: &str = "fold.motion.native.editor";
pub const INSPECTOR: &str = "fold.motion.native.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: "fold.motion.native.scene.network",
        title: "Scene Network",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: "fold.motion.native.scene.inspector",
        title: "Scene Inspector",
        placement: PanelPlacement::Inspector,
    },
    PanelDescriptor {
        id: PANEL,
        title: "Network",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: INSPECTOR,
        title: "Inspector",
        placement: PanelPlacement::Inspector,
    },
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub construction: Construction,
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
        .ok_or("Missing Motion document")?;
    let after = editing::validate_document(original, editing::Scope::Document, args.construction)
        .map_err(|e| format!("{e:?}"))?;
    cancel.check()?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(after)],
    })
}

/// A useful editable starting point, built from the qualified native registry.
pub fn create(id: DocumentId) -> Result<Document, String> {
    create_for_canvas(
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
pub fn create_for_canvas(id: DocumentId, canvas: Canvas) -> Result<Document, String> {
    let size = Input {
        id: ObjectId::new(),
        name: "Width".into(),
        ty: Type::Distance { unit: Unit::Canvas },
        default: Literal::Distance {
            value: 200.,
            unit: Unit::Canvas,
        },
        protected: false,
        extensions: Default::default(),
    };
    let shape = crate::registry::resolve("fold.motion.shape", 1, "rectangle")?.node(
        "Rectangle",
        [
            (
                "width".into(),
                Binding::Link {
                    reference: Reference::Input { input: size.id }.into(),
                },
            ),
            (
                "height".into(),
                Binding::Literal {
                    value: Literal::Distance {
                        value: 100.,
                        unit: Unit::Canvas,
                    },
                },
            ),
        ]
        .into(),
    )?;
    let fill = crate::registry::resolve("fold.motion.fill", 1, "solid")?.node(
        "Rectangle fill",
        [(
            "geometry".into(),
            Binding::Link {
                reference: Reference::Node { node: shape.id }.into(),
            },
        )]
        .into(),
    )?;
    let place = crate::registry::resolve("fold.motion.place", 1, "screen")?.node(
        "Placement",
        [
            (
                "geometry".into(),
                Binding::Link {
                    reference: Reference::Node { node: fill.id }.into(),
                },
            ),
            (
                "position".into(),
                Binding::Literal {
                    value: Literal::Vector {
                        value: vec![f64::from(canvas.width) / 2., f64::from(canvas.height) / 2.],
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    },
                },
            ),
        ]
        .into(),
    )?;
    let scene = crate::registry::resolve("fold.motion.scene", 1, "ordered")?.node(
        "Scene",
        [(
            "children".into(),
            Binding::List {
                values: vec![Binding::Link {
                    reference: Reference::Node { node: place.id }.into(),
                }],
            },
        )]
        .into(),
    )?;
    let output = Output {
        id: ObjectId::new(),
        name: "main".into(),
        ty: Type::Scene,
        value: Reference::Node { node: scene.id }.into(),
        extensions: Default::default(),
    };
    let definition = Definition {
        id: ObjectId::new(),
        name: "Motion".into(),
        inputs: vec![size],
        nodes: vec![shape, place, scene, fill],
        instances: vec![],
        outputs: vec![output],
        extensions: Default::default(),
    };
    let value = Construction {
        seed: 0,
        canvas,
        root: Instance {
            id: ObjectId::new(),
            name: "Main".into(),
            definition: definition.id,
            clock: Clock::default(),
            overrides: Default::default(),
            extensions: Default::default(),
        },
        definitions: vec![definition],
        presentation: Presentation::default(),
        extensions: Default::default(),
    };
    value.validate_semantics()?;
    value.document(id)
}

struct Documents;
impl DocumentProvider for Documents {
    fn unlock_for_artist(
        &self,
        document: &Document,
        object: Option<ObjectId>,
        targets: &[ObjectId],
    ) -> Result<Document, String> {
        if object.is_some() {
            return Err("Standalone graph unlock has no object scope".into());
        }
        editing::unlock_document_for_artist(document, targets).map_err(|error| format!("{error:?}"))
    }

    fn animation_channels(
        &self,
        document: &Document,
        object: Option<ObjectId>,
    ) -> Result<Vec<fold_platform::animation::Channel>, String> {
        crate::keyed::channels(&Construction::from_document(document)?, object)
    }
    fn edit_animation(
        &self,
        document: &Document,
        channels: &[fold_platform::animation::Channel],
        _: Time,
    ) -> Result<Document, String> {
        let construction =
            crate::keyed::replace(&Construction::from_document(document)?, channels)?;
        construction.replace_document(document)
    }

    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::CONSTRUCTION
    }
    fn schema(&self) -> u32 {
        crate::SCHEMA_VERSION
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Construction::from_document(document).map(|_| ())
    }
    fn create(&self, id: DocumentId) -> Result<Document, String> {
        create(id)
    }
    fn authoring_catalog(&self) -> serde_json::Value {
        crate::registry::catalog()
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        let construction = Construction::from_document(document)?;
        Ok(
            serde_json::json!({"construction": construction, "source": crate::source::canonical(&construction)?, "rendering": {
                "screen": true,
                "spatial": fold_render::spatial::GPU_ENABLED,
                "mixed": fold_render::spatial::GPU_ENABLED,
                "camera_plane": false,
                "spatial_aces": fold_render::spatial::GPU_ENABLED,
                "spatial_depth": "contiguous_world_pass"
            }}),
        )
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        let patches: Vec<crate::patch::Patch> = edits
            .iter()
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let before = Construction::from_document(document)?;
        let after = crate::patch::construction(&before, &patches).map_err(|e| format!("{e:?}"))?;
        editing::validate_document(document, editing::Scope::Document, after)
            .map_err(|e| format!("{e:?}"))
    }
    fn authoring_receipt(
        &self,
        before: &Document,
        after: &Document,
    ) -> Result<serde_json::Value, String> {
        serde_json::to_value(crate::diff::construction(
            &Construction::from_document(before)?,
            &Construction::from_document(after)?,
        )?)
        .map_err(|e| e.to_string())
    }
}
pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    registry.register(Contributions {
        manifest: Manifest {
            id: crate::PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: PANELS,
            build: "native-motion-schema1-cpu1",
        },
        documents: vec![
            Box::new(Documents),
            Box::new(crate::scene::package::Documents),
        ],
        commands: vec![CommandRegistration {
            id: EDIT,
            title: "Edit Motion construction",
            execution: Execution::Immediate,
            handler: edit,
        }]
        .into_iter()
        .chain(crate::scene::package::commands())
        .collect(),
        video: vec![
            Box::new(crate::video::Provider),
            Box::new(crate::scene::video::Provider),
        ],
        audio: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_document_creates_evaluates_edits_and_rejects_legacy_or_stale_requests() {
        let mut registry = PackageRegistry::default();
        register(&mut registry).unwrap();
        let id = DocumentId::new();
        let mut document = registry.create(crate::CONSTRUCTION, id).unwrap();
        document
            .extensions
            .insert("vendor".into(), serde_json::json!([1, null, 3]));
        let construction = Construction::from_document(&document).unwrap();
        let prepared = crate::evaluation::Prepared::new(construction.clone()).unwrap();
        assert!(matches!(
            prepared
                .evaluate(
                    construction.definitions[0].outputs[0].id,
                    Time::ZERO,
                    Default::default(),
                    &|| Ok(())
                )
                .unwrap(),
            crate::evaluation::Value::Scene(_)
        ));
        assert!(registry.supports(&document));
        let mut legacy = document.clone();
        legacy.package_id = "fold.motion".into();
        assert!(!registry.supports(&legacy));
        let mut project = fold_project::Project::new(8);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document.clone())],
            })
            .unwrap();
        assert_eq!(
            registry.output(&project.snapshot(), id).unwrap().frames,
            120
        );
        let mut changed = construction.clone();
        changed.definitions[0].inputs[0].default = Literal::Distance {
            value: 350.,
            unit: Unit::Canvas,
        };
        let request = CommandRequest {
            id: EDIT.into(),
            base: project.snapshot().revision(),
            arguments: serde_json::to_vec(&EditArgs {
                document: id,
                construction: changed.clone(),
            })
            .unwrap(),
        };
        let batch = registry
            .stage(&project.snapshot(), &request, &Default::default())
            .unwrap();
        project.commit(batch).unwrap();
        assert_eq!(
            Construction::from_document(&project.snapshot().state().documents[&id]).unwrap(),
            changed
        );
        assert_eq!(
            project.snapshot().state().documents[&id].extensions,
            document.extensions
        );
        assert!(
            registry
                .stage(&project.snapshot(), &request, &Default::default())
                .is_err()
        );
        project.undo().unwrap();
        assert_eq!(
            Construction::from_document(&project.snapshot().state().documents[&id]).unwrap(),
            construction
        );
        let cancelled = fold_media::Cancel::default();
        cancelled.cancel();
        assert!(
            registry
                .stage(
                    &project.snapshot(),
                    &CommandRequest {
                        base: project.snapshot().revision(),
                        ..request
                    },
                    &cancelled
                )
                .is_err()
        );
    }
}
