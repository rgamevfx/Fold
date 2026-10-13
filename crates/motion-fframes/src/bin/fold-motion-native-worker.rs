//! Native-only documents use the same retained worker and renderer as scripts.
use fold_motion_fframes::{Context, ObjectId, Scripts, fframes};
struct Native;
impl Scripts for Native {
    fn render<'a>(&mut self, _: ObjectId, _: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
        Err("Native worker cannot execute script objects".into())
    }
}
fn main() {
    if let Err(error) = fold_motion_fframes::worker::run(Native) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
