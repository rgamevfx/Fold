//! One coalescing worker for application preferences; no I/O during UI redraw.
use crate::sdk::appearance::Appearance;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, thread::JoinHandle};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub ui: Appearance,
    pub application: fold_platform::application::Application,
    pub recent: Vec<PathBuf>,
    pub workspaces: BTreeMap<String, Value>,
}
impl Preferences {
    fn validate(&self) -> Result<(), String> {
        self.ui.validate()?;
        self.application.validate()?;
        if self.recent.len() > 20 || self.workspaces.len() > 32 {
            return Err("Too many saved preferences".into());
        }
        for (name, value) in &self.workspaces {
            if name.trim().is_empty() || name.len() > 128 {
                return Err("Invalid workspace name".into());
            }
            fold_platform::workspace::Workspace::decode(
                &serde_json::to_vec(value).map_err(|e| e.to_string())?,
                "",
            )?;
        }
        Ok(())
    }
}

pub(crate) enum Event {
    Loaded(Result<Preferences, String>),
    Saved(Result<(), String>),
}
pub(crate) struct Store {
    sender: Option<mpsc::SyncSender<Preferences>>,
    result: mpsc::Receiver<Event>,
    pending: Option<Preferences>,
    thread: Option<JoinHandle<()>>,
}
pub(crate) fn config_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".config")))
        .map(|p| p.join("fold/ui.json"))
}
pub(crate) fn load(path: &std::path::Path) -> Result<(Preferences, Value), String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Preferences::default(), json!({"version": 1})));
        }
        Err(e) => return Err(e.to_string()),
        Ok(m) if m.len() > 16 * 1024 * 1024 => {
            return Err("UI settings exceed the size limit".into());
        }
        Ok(_) => {}
    }
    let doc: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if doc.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported UI settings version".into());
    }
    let value: Preferences = serde_json::from_value(doc.clone()).map_err(|e| e.to_string())?;
    value.validate()?;
    Ok((value, doc))
}
fn merge(target: &mut Value, source: Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            merge(target.entry(key).or_insert(Value::Null), value.clone());
        }
    } else {
        *target = source;
    }
}
fn save(path: &std::path::Path, doc: &mut Value, value: Preferences) -> Result<(), String> {
    value.validate()?;
    // Whole maps must replace so rename/delete do not resurrect old presets.
    doc["workspaces"] = serde_json::to_value(&value.workspaces).map_err(|e| e.to_string())?;
    doc["recent"] = serde_json::to_value(&value.recent).map_err(|e| e.to_string())?;
    merge(
        doc,
        json!({"version": 1, "ui": value.ui, "application": value.application}),
    );
    let bytes = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("UI settings exceed the size limit".into());
    }
    std::fs::create_dir_all(
        path.parent()
            .ok_or("UI settings directory is unavailable")?,
    )
    .map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(temporary, path).map_err(|e| e.to_string())
}
impl Store {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let (sender, requests) = mpsc::sync_channel(1);
        let (publish, result) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let initial = path
                .as_ref()
                .ok_or_else(|| "No UI settings directory is configured".to_string())
                .and_then(|p| load(p));
            // Preserve malformed/newer preferences. The error is shown in Settings;
            // do not silently overwrite a file this version cannot understand.
            let mut document = initial.as_ref().ok().map(|(_, doc)| doc.clone());
            let _ = publish.send(Event::Loaded(initial.map(|(value, _)| value)));
            while let Ok(mut value) = requests.recv() {
                while let Ok(newer) = requests.try_recv() {
                    value = newer;
                }
                let result = match (&path, &mut document) {
                    (Some(path), Some(doc)) => save(path, doc, value),
                    _ => Err(
                        "Preferences could not be loaded; the original file has been preserved"
                            .into(),
                    ),
                };
                let _ = publish.send(Event::Saved(result));
            }
        });
        Self {
            sender: Some(sender),
            result,
            pending: None,
            thread: Some(thread),
        }
    }
    pub(crate) fn save(&mut self, value: Preferences) {
        self.pending = Some(value);
        self.flush();
    }
    fn flush(&mut self) {
        if let Some(value) = self.pending.take() {
            match self.sender.as_ref().unwrap().try_send(value) {
                Err(mpsc::TrySendError::Full(value)) => self.pending = Some(value),
                Err(mpsc::TrySendError::Disconnected(_)) | Ok(()) => {}
            }
        }
    }
    pub(crate) fn poll(&mut self) -> Option<Event> {
        self.flush();
        self.result.try_recv().ok()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Some(value) = self.pending.take() {
            let _ = self.sender.as_ref().unwrap().send(value);
        }
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "settings_store_tests.rs"]
mod tests;
