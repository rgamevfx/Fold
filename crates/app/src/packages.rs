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
            use fold_platform::packages::*;
            registry
                .register(Contributions {
                    manifest: Manifest {
                        id: "fold.app",
                        version: env!("CARGO_PKG_VERSION"),
                        host_api: HOST_API,
                        dependencies: &["fold.timeline", "fold.compositor"],
                        panels: &[],
                        build: "composition-v1",
                    },
                    documents: vec![],
                    video: vec![],
                    audio: vec![],
                    commands: vec![CommandRegistration {
                        id: crate::composition::CREATE,
                        title: "Create Composition (keep audio)",
                        execution: Execution::Immediate,
                        handler: crate::composition::create,
                    }],
                })
                .expect("valid application commands");
            Arc::new(registry)
        })
        .clone()
}
