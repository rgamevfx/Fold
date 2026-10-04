//! Static application assembly, shared by desktop, workers, and CLI.
use fold_platform::packages::PackageRegistry;
use std::sync::{Arc, OnceLock};
pub fn builtins() -> Arc<PackageRegistry> {
    static PACKAGES: OnceLock<Arc<PackageRegistry>> = OnceLock::new();
    PACKAGES
        .get_or_init(|| {
            let mut registry = PackageRegistry::default();
            fold_timeline::package::register(&mut registry)
                .expect("valid built-in package registration");
            fold_compositor::package::register(&mut registry)
                .expect("valid compositor package registration");
            fold_motion::package::register(&mut registry)
                .expect("valid motion package registration");
            use fold_platform::packages::*;
            registry
                .register(Contributions {
                    manifest: Manifest {
                        id: "fold.app",
                        version: env!("CARGO_PKG_VERSION"),
                        host_api: HOST_API,
                        dependencies: &["fold.timeline", "fold.compositor"],
                        panels: &[PanelDescriptor {
                            id: "fold.app.project",
                            title: "Project",
                            placement: PanelPlacement::Browser,
                        }],
                        build: "composition-v1",
                    },
                    documents: vec![],
                    video: vec![],
                    audio: vec![],
                    commands: vec![
                        CommandRegistration {
                            id: crate::read_source::REPLACE,
                            title: "Choose Read source",
                            execution: Execution::Worker,
                            handler: crate::read_source::replace,
                        },
                        CommandRegistration {
                            id: crate::composition::CREATE,
                            title: "Create Composition (keep audio)",
                            execution: Execution::Immediate,
                            handler: crate::composition::create,
                        },
                        CommandRegistration {
                            id: crate::document_placement::INSERT,
                            title: "Insert document into sequence",
                            execution: Execution::Immediate,
                            handler: crate::document_placement::insert,
                        },
                    ],
                })
                .expect("valid application commands");
            Arc::new(registry)
        })
        .clone()
}
