//! Static application assembly, shared by desktop, workers, and CLI.
use fold_platform::packages::PackageRegistry;
use std::sync::{Arc, OnceLock};
pub fn builtins() -> Arc<PackageRegistry> {
    static PACKAGES: OnceLock<Arc<PackageRegistry>> = OnceLock::new();
    PACKAGES
        .get_or_init(|| {
            let mut registry = PackageRegistry::default();
            fold_composition::package::register_with_edit(
                &mut registry,
                crate::composition_layers::edit,
            )
            .expect("valid composition package registration");
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
                        build: "composition-v2-layer-graphs",
                    },
                    documents: vec![],
                    video: vec![],
                    audio: vec![],
                    commands: vec![
                        CommandRegistration {
                            id: crate::timeline_transition::ADD,
                            title: "Add Transition",
                            execution: Execution::Immediate,
                            handler: crate::timeline_transition::add,
                        },
                        CommandRegistration {
                            id: fold_platform::settings::PROJECT_COMMAND,
                            title: "Project settings",
                            execution: Execution::Worker,
                            handler: crate::settings::project,
                        },
                        CommandRegistration {
                            id: fold_platform::settings::DOCUMENT_COMMAND,
                            title: "Document settings",
                            execution: Execution::Immediate,
                            handler: crate::settings::document,
                        },
                        CommandRegistration {
                            id: crate::rust_asset::CREATE,
                            title: "Create Rust asset",
                            execution: Execution::Worker,
                            handler: crate::rust_asset::create,
                        },
                        CommandRegistration {
                            id: crate::composition_layers::ADD,
                            title: "Add graph",
                            execution: Execution::Immediate,
                            handler: crate::composition_layers::add,
                        },
                        CommandRegistration {
                            id: crate::composition_layers::CONNECT,
                            title: "Connect graphs",
                            execution: Execution::Immediate,
                            handler: crate::composition_layers::connect,
                        },
                        CommandRegistration {
                            id: fold_platform::image_bake::COMMAND,
                            title: "Bake image textures",
                            execution: Execution::Worker,
                            handler: crate::image_bake::run,
                        },
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
