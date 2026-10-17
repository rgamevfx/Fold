//! Application-owned worker configuration. Assembly performs no compilation or I/O;
//! evaluation workers initialize the compiler and retained GPU renderer on demand.
use fold_motion_context::{
    runtime::{Config, Profile},
    video::Provider,
};
use std::{path::PathBuf, time::Duration};

pub(crate) fn configuration() -> Config {
    let cache = std::env::var_os("FOLD_MOTION_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .unwrap_or_else(std::env::temp_dir)
                .join("fold/motion")
        });
    let bridge = std::env::var_os("FOLD_MOTION_BRIDGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let executable = std::env::current_exe().expect("Motion executable location");
            let parent = executable.parent().expect("Motion executable directory");
            let profile = std::path::Path::new(env!("FOLD_MOTION_PROFILE"));
            let root = if parent == profile
                || parent == profile.join("deps")
                || parent == profile.join("examples")
            {
                PathBuf::from(env!("FOLD_BUILT_MOTION_ROOT"))
            } else {
                parent.join("../share/fold/motion-sdk")
            };
            root.join("crates/motion-fframes")
        });
    let tools = bridge
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Motion SDK has a root directory")
        .join("toolchain/bin");
    Config {
        bridge,
        cache,
        rustc: tools.join("rustc"),
        profile: Profile::Release,
        compile_timeout: Duration::from_secs(600),
        frame_timeout: Duration::from_secs(30),
    }
}

pub(crate) fn providers() -> (fold_motion_context::package::Provider, Provider) {
    let config = configuration();
    let fonts = vec![
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/DMSans-Medium.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Inter-Bold.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Inter-Medium.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Inter-Regular.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Inter-SemiBold.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Montserrat-Black.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Montserrat-Bold.ttf").to_vec(),
        include_bytes!("../../../resources/fframes-components/Montserrat-ExtraBold.ttf").to_vec(),
    ];
    (
        fold_motion_context::package::Provider::new(fold_motion_context::conversion::Service::new(
            config.clone(),
            fonts.clone(),
        )),
        Provider::new(config, fonts),
    )
}
