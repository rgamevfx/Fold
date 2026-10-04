//! Bounded worker-only browser previews. This is the existing SDR source policy,
//! not an ACES working image or a frame usable for export.
use crate::{Cancel, ingest::SourceProfile, process::Process};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
};
pub const SIZE: [u32; 2] = [128, 72];
pub fn thumbnail(
    path: &Path,
    fingerprint: &str,
    profile: &SourceProfile,
    cancel: &Cancel,
) -> Result<Vec<u8>, String> {
    // Archives are not proof of decode safety. Revalidate the narrow source
    // profile on the worker before letting the codec allocate native planes.
    let inspected = crate::ingest::inspect_linked(path, cancel)?;
    if inspected.fingerprint != fingerprint || &inspected.metadata.profile != profile {
        return Err("Source changed; relink required".into());
    }
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-max_alloc",
            "33554432",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-filter_complex_threads",
            "1",
            "-i",
        ])
        .arg(path);
    match profile {
        SourceProfile::Video(_) => {
            command.args(["-an", "-vf", "scale=128:72:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=bilinear,zscale=matrixin=709:transferin=709:primariesin=709:rangein=limited:matrix=gbr:transfer=iec61966-2-1:primaries=709:range=full,format=gbrpf32le,format=rgb24,pad=128:72:(ow-iw)/2:(oh-ih)/2"]);
        }
        SourceProfile::Exr(_) => {
            return Err("EXR browser thumbnails require a selected color interpretation".into());
        }
        SourceProfile::Ppm { .. } => {
            command.args(["-vf", "scale=128:72:force_original_aspect_ratio=decrease:flags=bilinear,pad=128:72:(ow-iw)/2:(oh-ih)/2"]);
        }
        SourceProfile::Wave(_) => {
            command.args([
                "-filter_complex",
                "atrim=duration=30,showwavespic=s=128x72:colors=68b394",
            ]);
        }
    }
    command
        .args(["-frames:v", "1", "-pix_fmt", "rgba", "-f", "rawvideo"])
        .arg(output.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    let bytes = (SIZE[0] * SIZE[1] * 4) as usize;
    Process::spawn(
        command,
        cancel.clone(),
        Some((output.path().into(), bytes as u64)),
    )?
    .finish()?;
    let mut pixels = Vec::new();
    output
        .reopen()
        .map_err(|e| e.to_string())?
        .take(bytes as u64 + 1)
        .read_to_end(&mut pixels)
        .map_err(|e| e.to_string())?;
    if pixels.len() != bytes {
        return Err("invalid thumbnail dimensions".into());
    }
    if crate::fingerprint(path, cancel)? != fingerprint {
        return Err("Source changed during thumbnail".into());
    }
    cancel.check()?;
    Ok(pixels)
}
