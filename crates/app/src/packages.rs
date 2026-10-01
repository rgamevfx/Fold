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
            Arc::new(registry)
        })
        .clone()
}
