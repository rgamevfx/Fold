//! Bounded, leased video preparation before graphics admission. The two app
//! workers each stage one graph; source storage remains under aggregate budgets.
use super::{ImageOp, RenderGraph, Renderer, Statistics};
use fold_media::{Cancel, DecodeBackend, Decoder, YuvFrame};

pub(super) enum Video {
    Planes(YuvFrame),
    #[cfg(all(feature = "native-video", target_os = "linux"))]
    Native(std::sync::Arc<super::native_video::NativeInput>),
}

impl Renderer {
    pub(super) fn prepare_video(
        &mut self,
        graph: &RenderGraph,
        needed: &[bool],
        decoder: &mut Decoder,
        cancel: &Cancel,
        stats: &mut Statistics,
    ) -> Result<Vec<Option<Video>>, fold_foundation::Failure> {
        let sources: Vec<_> = graph
            .nodes
            .iter()
            .zip(needed)
            .enumerate()
            .filter_map(|(index, (op, needed))| match op {
                ImageOp::Video { source, time } | ImageOp::VideoInput { source, time, .. }
                    if *needed =>
                {
                    Some((index, source, *time))
                }
                _ => None,
            })
            .collect();
        let mut inputs: Vec<Option<Video>> = (0..graph.nodes.len()).map(|_| None).collect();
        if sources.is_empty() {
            self.secondary_decoder = None;
            decoder.single_stream(false);
            return Ok(inputs);
        }
        let begin = std::time::Instant::now();
        let mut native_sources = Vec::new();
        let mut software_sources = Vec::new();
        for (index, source, time) in sources {
            if decoder.backend() == DecodeBackend::CudaNative
                && decoder.source_format(source, cancel)?.nvdec_compatible
            {
                native_sources.push((index, source, time));
            } else {
                software_sources.push((index, source, time));
            }
        }
        #[cfg(not(all(feature = "native-video", target_os = "linux")))]
        if !native_sources.is_empty() {
            return Err("native NVDEC support is not enabled in this build/platform".into());
        }
        if native_sources.is_empty() {
            decoder.single_stream(software_sources.len() > 1);
            if software_sources.len() > 1 {
                let secondary = self
                    .secondary_decoder
                    .get_or_insert_with(|| Decoder::with_backend(decoder.backend()));
                if secondary.backend() != decoder.backend() {
                    *secondary = Decoder::with_backend(decoder.backend());
                }
                secondary.single_stream(true);
                let (first, rest) = software_sources.split_at(1);
                let (a, b) = std::thread::scope(|scope| {
                    let worker = scope.spawn(|| planes(secondary, rest, cancel));
                    let a = planes(decoder, first, cancel);
                    (
                        a,
                        worker
                            .join()
                            .map_err(|_| {
                                fold_foundation::Failure::from("video preparation worker panicked")
                            })
                            .and_then(|r| r),
                    )
                });
                for (frames, bytes) in [a?, b?] {
                    stats.native_pipe_bytes += bytes;
                    for (id, frame) in frames {
                        inputs[id] = Some(Video::Planes(frame));
                    }
                }
            } else {
                self.secondary_decoder = None;
                let (frames, bytes) = planes(decoder, &software_sources, cancel)?;
                stats.native_pipe_bytes += bytes;
                for (id, frame) in frames {
                    inputs[id] = Some(Video::Planes(frame));
                }
            }
        } else {
            decoder.single_stream(false);
            self.secondary_decoder = None;
            std::thread::scope(|scope| -> Result<(), fold_foundation::Failure> {
                // Hardware and software decode own distinct codec state. Only
                // spawn when both lanes contain work; a single clip stays local.
                let worker = (!software_sources.is_empty())
                    .then(|| scope.spawn(|| planes(decoder, &software_sources, cancel)));
                #[cfg(all(feature = "native-video", target_os = "linux"))]
                for (index, source, time) in native_sources {
                    if self.native_video.is_none() {
                        self.native_video = Some(super::native_video::NativeVideo::new()?);
                    }
                    let native = self.native_video.as_mut().unwrap();
                    let before = native.statistics();
                    let input = native.decode(&self.host, source, time, cancel)?;
                    let after = native.statistics();
                    stats.native_allocate_nanoseconds += native.allocate_nanoseconds;
                    stats.native_submit_nanoseconds += native.submit_nanoseconds;
                    stats.native_codec_call_nanoseconds += after
                        .codec_call_nanoseconds
                        .saturating_sub(before.codec_call_nanoseconds);
                    stats.native_copy_ready_nanoseconds += after
                        .copy_ready_nanoseconds
                        .saturating_sub(before.copy_ready_nanoseconds);
                    stats.native_gpu_local_copy_bytes += after
                        .gpu_local_copy_bytes
                        .saturating_sub(before.gpu_local_copy_bytes);
                    stats.native_seeks += after.seeks.saturating_sub(before.seeks);
                    stats.native_forward_reuses +=
                        after.forward_reuses.saturating_sub(before.forward_reuses);
                    inputs[index] = Some(Video::Native(input));
                }
                if let Some(worker) = worker {
                    let (frames, bytes) = worker
                        .join()
                        .map_err(|_| "video preparation worker panicked")??;
                    stats.native_pipe_bytes += bytes;
                    for (id, frame) in frames {
                        inputs[id] = Some(Video::Planes(frame));
                    }
                }
                Ok(())
            })?;
        }
        stats.native_decode_nanoseconds += begin.elapsed().as_nanos();
        Ok(inputs)
    }
}

type Source<'a> = (usize, &'a fold_media::VideoSource, fold_foundation::Time);
fn planes(
    decoder: &mut Decoder,
    sources: &[Source<'_>],
    cancel: &Cancel,
) -> Result<(Vec<(usize, YuvFrame)>, u64), fold_foundation::Failure> {
    let before = decoder.statistics().pipe_bytes;
    let mut frames = Vec::with_capacity(sources.len());
    for &(id, source, time) in sources {
        cancel.check()?;
        frames.push((id, decoder.decode_native(source, time, cancel)?));
    }
    Ok((frames, decoder.statistics().pipe_bytes - before))
}
