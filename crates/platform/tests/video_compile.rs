use fold_foundation::{DocumentId, Time};
use fold_media::Cancel;
use fold_platform::{VideoCompile, VideoProvider, VideoRegistry};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project, Revision};
use fold_render::{RenderGraph, SolidPlan};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Provider(Arc<AtomicUsize>);
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        "test.video"
    }
    fn type_id(&self) -> &'static str {
        "test.video.document"
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        self.0.fetch_add(1, Ordering::Relaxed);
        assert_eq!(request.dimensions, [2, 2]);
        if let Some(nested) = request.document.dependencies.first() {
            request.cancel.cancel();
            return (request.resolve)(nested, request.time);
        }
        Ok(SolidPlan {
            width: 2,
            height: 2,
            rgba: [1.; 4],
        }
        .into())
    }
}
fn reference(id: DocumentId) -> DocumentRef {
    DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn fixture() -> (Project, DocumentRef, VideoRegistry, Arc<AtomicUsize>) {
    let mut project = Project::new(0);
    let child = DocumentId::new();
    let root = DocumentId::new();
    let documents = [(child, vec![]), (root, vec![reference(child)])].map(|(id, dependencies)| {
        Mutation::PutDocument(Document {
            id,
            package_id: "test.video".into(),
            type_id: "test.video.document".into(),
            schema_version: 1,
            revision: Revision(0),
            dependencies,
            assets: vec![],
            payload: vec![],
            extensions: Default::default(),
        })
    });
    project
        .commit(EditBatch {
            base: Revision(0),
            mutations: documents.into(),
        })
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = VideoRegistry::default();
    registry.register(Provider(calls.clone())).unwrap();
    (project, reference(root), registry, calls)
}
#[test]
fn cancellation_is_checked_before_dispatch_and_inherited_by_nested_resolution() {
    let (project, source, registry, calls) = fixture();
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(
        registry
            .compile_with(&project.snapshot(), &source, Time::ZERO, [2, 2], &cancelled)
            .unwrap_err()
            .contains("cancelled")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    let cancel = Cancel::default();
    assert!(
        registry
            .compile_with(&project.snapshot(), &source, Time::ZERO, [2, 2], &cancel)
            .unwrap_err()
            .contains("cancelled")
    );
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "cancelled parent must not dispatch its child"
    );
}
#[test]
fn providers_cannot_silently_ignore_reference_controls() {
    let (project, mut source, registry, calls) = fixture();
    source
        .extensions
        .insert("fold.controls".into(), true.into());
    assert!(
        registry
            .compile(&project.snapshot(), &source, Time::ZERO, 2, 2)
            .unwrap_err()
            .contains("control overrides")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}
