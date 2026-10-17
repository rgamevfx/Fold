#[path = "../../scripts/native_runtime_build.rs"]
mod native_runtime;
fn main() {
    native_runtime::prepare("rife");
}
