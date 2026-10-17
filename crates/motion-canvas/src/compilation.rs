//! Generate an ordinary Rust fframes program, with no Fold scripting language.
//! Build/start it off the UI thread. Runtime controls do not affect source identity.
use crate::model::{Canvas, Content};
use sha2::{Digest, Sha256};
use std::{fmt::Write, path::Path};

pub const FFRAMES_REVISION: &str = "b644f22cf5be2333e7098caf7ec064c3d36aeb02";
pub struct Program {
    pub identity: String,
    pub manifest: String,
    pub source: String,
    pub executable: String,
    pub requires_compilation: bool,
}

/// `runtime_identity` is the build identity of the native Fold/fFrames bridge.
/// The caller must change it when that bridge or its dependency lock changes.
pub fn program(
    canvas: &Canvas,
    runtime_path: &Path,
    runtime_identity: &str,
) -> Result<Program, String> {
    build(canvas, runtime_path, runtime_identity, false)
}
/// Motion code runs only during explicit preparation. Playback uses native curves.
pub fn motion_program(
    canvas: &Canvas,
    runtime_path: &Path,
    runtime_identity: &str,
) -> Result<Program, String> {
    build(canvas, runtime_path, runtime_identity, true)
}
fn build(
    canvas: &Canvas,
    runtime_path: &Path,
    runtime_identity: &str,
    prepare_motion: bool,
) -> Result<Program, String> {
    canvas.validate()?;
    let runtime_path = runtime_path.to_str().ok_or("Runtime path is not UTF-8")?;
    let mut source =
        "use fold_motion_fframes::{Context, ObjectId, Scripts};\nstruct Program;\n".to_string();
    let mut arms = String::new();
    for (index, (id, object)) in canvas
        .objects
        .iter()
        .filter(|(_, object)| matches!(object.content, Content::Script { .. }))
        .enumerate()
    {
        let Content::Script { source: script, .. } = &object.content else {
            continue;
        };
        writeln!(
            source,
            "mod script_{index} {{\nuse fold_motion_fframes::Context;\n{script}\n}}"
        )
        .map_err(|e| e.to_string())?;
        writeln!(arms, "\"{id}\" => script_{index}::render(input),").map_err(|e| e.to_string())?;
    }
    let mut motion_arms = String::new();
    for (index, (id, object)) in canvas.objects.iter().enumerate() {
        if prepare_motion && let Some(motion) = &object.motion {
            writeln!(source,"mod motion_{index} {{ use fold_motion_fframes::{{MotionContext, MotionPose}};\n{}\n}}",motion.spec.source).map_err(|e|e.to_string())?;
            writeln!(motion_arms, "\"{id}\" => motion_{index}::animate(input),")
                .map_err(|e| e.to_string())?;
        }
    }
    write!(source, "impl Scripts for Program {{\nfn render<'a>(&mut self, id: ObjectId, input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {{\nmatch id.to_string().as_str() {{\n{arms}_ => Err(\"Script source changed; rebuild the Motion program\".into()),\n}}\n}}\nfn animate(&mut self, id:ObjectId, input:fold_motion_fframes::MotionContext<'_>)->Result<fold_motion_fframes::MotionPose,String>{{ match id.to_string().as_str() {{ {motion_arms} _=>Err(\"Missing motion behavior\".into()) }} }}\n}}\nfn main() {{ if let Err(error) = fold_motion_fframes::worker::run(Program) {{ eprintln!(\"{{error}}\"); std::process::exit(1); }} }}\n").map_err(|e| e.to_string())?;
    let mut binary_hash = Sha256::new();
    for part in [runtime_identity, source.as_str()] {
        binary_hash.update((part.len() as u64).to_le_bytes());
        binary_hash.update(part.as_bytes());
    }
    let executable = format!("fold-motion-{:x}", binary_hash.finalize());
    let manifest = format!(
        "[workspace]\n[package]\nname = \"fold-motion-program\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[[bin]]\nname = \"{executable}\"\npath = \"src/main.rs\"\n[dependencies]\nfold-motion-fframes = {{ path = {} }}\nfframes = {{ git = \"https://github.com/dmtrKovalenko/fframes\", rev = \"{FFRAMES_REVISION}\", default-features = false, features = [\"compile-time-svgtree\", \"build-portable\"] }}\n",
        serde_json::to_string(runtime_path).map_err(|e| e.to_string())?
    );
    let mut hash = Sha256::new();
    for part in [runtime_identity, &manifest, &source] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    Ok(Program {
        identity: format!("{:x}", hash.finalize()),
        manifest,
        source,
        executable,
        requires_compilation: !arms.is_empty() || !motion_arms.is_empty(),
    })
}
