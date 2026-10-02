use fold_foundation::{DocumentId, Time};
use fold_media::{Cancel, VideoInfo};
use fold_platform::{VideoCompile, VideoProvider, packages::*, workspace::OutputDescriptor};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project};
use fold_render::{RenderGraph, SolidPlan};

struct Provider;
impl DocumentProvider for Provider {
    fn package_id(&self) -> &'static str {
        "test.ports"
    }
    fn type_id(&self) -> &'static str {
        "test.ports.document"
    }
    fn schema(&self) -> u32 {
        1
    }
    fn validate(&self, _: &Document) -> Result<(), String> {
        Ok(())
    }
}
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        "test.ports"
    }
    fn type_id(&self) -> &'static str {
        "test.ports.document"
    }
    fn outputs(&self, document: &Document) -> Vec<OutputDescriptor> {
        ["beauty", "matte"]
            .map(|port| OutputDescriptor {
                reference: DocumentRef {
                    document: document.id,
                    output: port.into(),
                    extensions: Default::default(),
                },
                label: port.into(),
                info: Ok(VideoInfo {
                    width: 2,
                    height: 2,
                    rate: [24, 1],
                    frames: 24,
                }),
            })
            .into()
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        assert_eq!(request.reference.output, "matte");
        Ok(SolidPlan {
            width: 2,
            height: 2,
            rgba: [1.; 4],
        }
        .into())
    }
}
#[test]
fn provider_ports_are_enumerated_and_resolved_by_stable_output_identity() {
    let mut registry = PackageRegistry::default();
    registry
        .register(Contributions {
            manifest: Manifest {
                id: "test.ports",
                version: "1",
                host_api: HOST_API,
                dependencies: &[],
                panels: &[],
                build: "test",
            },
            documents: vec![Box::new(Provider)],
            video: vec![Box::new(Provider)],
            commands: vec![],
            audio: vec![],
        })
        .unwrap();
    let mut project = Project::new(4);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(Document {
                id,
                type_id: "test.ports.document".into(),
                package_id: "test.ports".into(),
                schema_version: 1,
                revision: Default::default(),
                dependencies: vec![],
                assets: vec![],
                payload: vec![],
                extensions: Default::default(),
            })],
        })
        .unwrap();
    let outputs = registry.outputs(&project.snapshot(), id);
    assert_eq!(outputs.len(), 2);
    let source = outputs[1].reference.clone();
    assert_eq!(
        registry
            .output_ref(&project.snapshot(), &source)
            .unwrap()
            .rate,
        [24, 1]
    );
    registry
        .video_with(
            &project.snapshot(),
            &source,
            Time::ZERO,
            [2, 2],
            &Cancel::default(),
        )
        .unwrap();
    let invalid = DocumentRef {
        output: "missing".into(),
        ..source
    };
    assert!(
        registry
            .video_with(
                &project.snapshot(),
                &invalid,
                Time::ZERO,
                [2, 2],
                &Cancel::default()
            )
            .is_err()
    );
}
