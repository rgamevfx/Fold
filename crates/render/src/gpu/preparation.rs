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
    ) -> Result<Vec<Option<Video>>, String> {
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
            return Ok(inputs);
        }
        let begin = std::time::Instant::now();
        if decoder.backend() == DecodeBackend::CudaNative {
            #[cfg(all(feature = "native-video", target_os = "linux"))]
            {
                if self.native_video.is_none() {
                    self.native_video = Some(super::native_video::NativeVideo::new()?);
                }
                let native = self.native_video.as_mut().unwrap();
                for (index, source, time) in sources {
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
            }
            #[cfg(not(all(feature = "native-video", target_os = "linux")))]
            return Err("native NVDEC support is not enabled in this build/platform".into());
        } else {
            for (index, source, time) in sources {
                cancel.check()?;
                let before = decoder.statistics().pipe_bytes;
                let frame = decoder.decode_native(source, time, cancel)?;
                stats.native_pipe_bytes += decoder.statistics().pipe_bytes - before;
                inputs[index] = Some(Video::Planes(frame));
            }
        }
        stats.native_decode_nanoseconds += begin.elapsed().as_nanos();
        Ok(inputs)
    }
}
