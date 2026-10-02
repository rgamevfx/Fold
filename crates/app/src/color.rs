//! Application color routing. Native handles and all resource I/O stay worker-local.
use fold_color::{Config, Runtime, settings::*};
use fold_project::{Project, Snapshot};
use std::cell::RefCell;
thread_local! { static CONFIG: RefCell<Option<(ProjectColor, Config)>> = const { RefCell::new(None) }; }

pub fn new_project(history: usize) -> Project {
    Project::with_settings(
        history,
        [(
            PROJECT_KEY.into(),
            serde_json::to_value(ProjectColor::default()).expect("serializable color defaults"),
        )]
        .into(),
    )
}
pub fn runtime() -> Result<Runtime, String> {
    // Explicit development/test installation override, never an implicit OCIO config.
    match std::env::var_os("FOLD_COLOR_ROOT") {
        Some(root) => Runtime::at(std::path::Path::new(&root)),
        None => Runtime::installed(),
    }
}
pub fn with_config<T>(
    snapshot: &Snapshot,
    f: impl FnOnce(Option<&Config>) -> Result<T, String>,
) -> Result<T, String> {
    let Some(settings) = fold_platform::color::project(snapshot)? else {
        return f(None);
    };
    CONFIG.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_none_or(|(key, _)| key != &settings) {
            let config = settings.load(&runtime()?)?;
            *slot = Some((settings, config));
        }
        f(slot.as_ref().map(|(_, config)| config))
    })
}
/// Explicit external configuration replacement for an existing ACES project.
/// Never reinterpret legacy provider payloads as ACES on load or Save As.
pub fn external(
    snapshot: &Snapshot,
    path: &std::path::Path,
) -> Result<fold_project::EditBatch, String> {
    let mut color = fold_platform::color::project(snapshot)?
        .ok_or("Legacy projects cannot be reinterpreted as ACEScg")?;
    let config = runtime()?.external(path)?;
    config.display(fold_color::WORKING_SPACE, &Default::default())?;
    config.display(
        fold_color::WORKING_SPACE,
        &OutputTransform::default().display_transform(),
    )?;
    config.conversion(SRGB_INPUT, fold_color::WORKING_SPACE)?;
    config.conversion(VIDEO_INPUT, fold_color::WORKING_SPACE)?;
    // Validate all declared input spaces and display/view resources, not just
    // the current viewer. Unsupported external configurations fail explicitly.
    let catalog = config.catalog()?;
    for space in &catalog.spaces {
        config.conversion(space, fold_color::WORKING_SPACE)?;
    }
    for (display, views) in catalog.displays {
        for view in views {
            config.display(
                fold_color::WORKING_SPACE,
                &fold_color::DisplayTransform {
                    display: display.clone(),
                    view,
                    look: None,
                },
            )?;
        }
    }
    color.config = ConfigSource::External {
        path: path.to_owned(),
        resources: config.identity().resources.clone(),
    };
    let mut settings = snapshot.state().settings.clone();
    settings.insert(
        PROJECT_KEY.into(),
        serde_json::to_value(color).map_err(|e| e.to_string())?,
    );
    Ok(fold_project::EditBatch {
        base: snapshot.revision(),
        mutations: vec![fold_project::Mutation::SetSettings(settings)],
    })
}
pub fn choices(snapshot: &Snapshot) -> Choices {
    match with_config(snapshot, |config| {
        config.map(Choices::from_config).transpose()
    }) {
        Ok(Some(choices)) => choices,
        Ok(None) => Choices::default(),
        Err(error) => Choices {
            error: Some(error),
            ..Default::default()
        },
    }
}
pub fn preview(
    snapshot: &Snapshot,
    frame: &fold_render::Frame,
) -> Result<fold_render::DisplayFrame, String> {
    with_config(snapshot, |config| match config {
        Some(config) => {
            frame.to_output(&config.display(fold_color::WORKING_SPACE, &Default::default())?)
        }
        None => frame.to_display().map_err(str::to_owned),
    })
}
pub fn delivery(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
    frame: &fold_render::Frame,
) -> Result<fold_render::DisplayFrame, String> {
    with_config(snapshot, |config| match config {
        Some(config) => frame.to_output(&config.display(
            fold_color::WORKING_SPACE,
            &fold_platform::color::output(snapshot, document)?.display_transform(),
        )?),
        None => frame.to_display().map_err(str::to_owned),
    })
}
pub fn set_output(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
    transform: OutputTransform,
) -> Result<fold_project::EditBatch, String> {
    if fold_platform::color::project(snapshot)?.is_none() {
        return Err("Legacy output uses its original sRGB transform".into());
    }
    let mut doc = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("Missing output document")?
        .as_ref()
        .clone();
    doc.extensions.insert(
        OUTPUT_KEY.into(),
        serde_json::to_value(transform).map_err(|e| e.to_string())?,
    );
    Ok(fold_project::EditBatch {
        base: snapshot.revision(),
        mutations: vec![fold_project::Mutation::PutDocument(doc)],
    })
}
