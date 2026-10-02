//! Relocation smoke test: install this executable under package/bin, then run
//! it from an unrelated working directory with OCIO set to an invalid location.
fn main() -> Result<(), String> {
    let runtime = fold_color::Runtime::installed()?;
    let config = runtime.bundled()?;
    let processor = config.display(fold_color::WORKING_SPACE, &Default::default())?;
    let mut pixels = [[0.18, 0.18, 0.18, 1.]];
    processor.apply(&mut pixels)?;
    if (pixels[0][0] - 0.3559523).abs() > 0.0002 {
        return Err(format!("reference mismatch: {pixels:?}"));
    }
    println!(
        "OCIO {}: {} -> {:?}",
        fold_color::OCIO_VERSION,
        config.identity().content_sha256,
        pixels[0]
    );
    Ok(())
}
