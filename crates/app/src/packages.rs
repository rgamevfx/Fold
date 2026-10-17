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
            let (motion, motion_video) = crate::motion_runtime::providers();
            fold_motion_context::package::register_with(
                &mut registry,
                motion,
                vec![Box::new(motion_video)],
                Some(crate::motion_conversion::convert),
                Some(crate::motion_conversion::animate),
            )
            .expect("valid Motion package registration");
            fold_three_d::package::register(&mut registry).expect("valid 3D package registration");
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
                            id: fold_platform::hierarchy::EDIT,
                            title: "Edit hierarchy",
                            execution: Execution::Immediate,
                            handler: crate::composition_layers::edit_hierarchy,
                        },
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
                            id: "fold.app.write-destination",
                            title: "Choose Write destination",
                            execution: Execution::Worker,
                            handler: crate::write_destination::choose,
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
            registry
                .register(Contributions {
                    manifest: Manifest {
                        id: "fold.gallery",
                        version: env!("CARGO_PKG_VERSION"),
                        host_api: HOST_API,
                        dependencies: &["fold.motion", "fold.timeline", "fold.compositor"],
                        panels: &[PanelDescriptor {
                            id: "fold.gallery.browser",
                            title: "Gallery",
                            placement: PanelPlacement::Browser,
                        }],
                        build: "components-v1",
                    },
                    documents: vec![],
                    video: vec![],
                    audio: vec![],
                    commands: vec![CommandRegistration {
                        id: fold_platform::gallery::PLACE,
                        title: "Place Gallery component",
                        execution: Execution::Worker,
                        handler: crate::gallery::stage,
                    }],
                })
                .expect("valid Gallery registration");
            Arc::new(registry)
        })
        .clone()
}
