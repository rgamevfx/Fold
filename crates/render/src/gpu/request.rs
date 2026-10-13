//! Request validation, dependency planning and leased source preparation.
#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
use super::*;
pub(super) struct Prepared {
    pub graph: RenderGraph,
    pub aces: bool,
    pub processors: std::collections::BTreeMap<String, fold_color::Processor>,
    pub needed: Vec<bool>,
    pub uses: Vec<usize>,
    pub fused: Vec<Option<(usize, f32)>>,
    pub chains: Vec<Option<pointwise::Chain>>,
    pub stats: Statistics,
    pub sources: Vec<Option<GpuFrame>>,
    pub dimensions: [u32; 2],
    pub keys: Vec<Option<still::Key>>,
    pub resident: Vec<Option<Arc<Image>>>,
    pub exr: crate::preparation::SourcePipeline,
    pub dependencies: Vec<validation::Completion>,
    pub video: Vec<Option<preparation::Video>>,
    pub evaluate_begin: std::time::Instant,
}
impl Renderer {
    fn relieve_pressure(&mut self) -> u32 {
        let memory = self.host.memory();
        if memory.allocated.saturating_sub(memory.reusable) <= memory.budget.saturating_mul(3) / 4 {
            return 0;
        }
        let target = memory.budget.saturating_mul(2) / 3;
        let mut evictions = 0;
        let mut misses = 0;
        // Fair, bounded work. Submitted/displayed leases remain independent.
        // Stop below the low-water mark instead of emptying every cache.
        for _ in 0..128 {
            let memory = self.host.memory();
            if memory.allocated.saturating_sub(memory.reusable) <= target {
                break;
            }
            let lane = self.pressure_cursor % 8;
            self.pressure_cursor = (lane + 1) % 8;
            let removed = match lane {
                0 => self.host.sources().evict_one(),
                1 => self.coverage.evict_one(),
                2 => self.baked.evict_one(),
                3 => self.yuv.evict_one(),
                4 => self.uploads.evict_one(),
                5 => self.vector.evict_one(),
                6 => self.filtered_blur.evict_one(),
                _ => {
                    #[cfg(all(feature = "native-video", target_os = "linux"))]
                    {
                        self.native_video
                            .as_mut()
                            .is_some_and(|native| native.evict_one())
                    }
                    #[cfg(not(all(feature = "native-video", target_os = "linux")))]
                    {
                        false
                    }
                }
            };
            if removed {
                evictions += 1;
                misses = 0;
            } else {
                misses += 1;
            }
            if misses == 8 {
                break;
            }
        }
        evictions
    }
    pub(super) fn prepare_request(
        &mut self,
        mut graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<Prepared, fold_foundation::Failure> {
        cancel.check()?;
        self.host.poll()?;
        // Retained inputs are expendable under host pressure; displayed and
        // submitted resources remain protected by their independent leases.
        let pressure_evictions = self.relieve_pressure();
        let evaluate_begin = std::time::Instant::now();
        let aces = config.is_some();
        crate::sampling::prepare(&mut graph);
        graph::validate(&graph, aces).map_err(|error| {
            fold_foundation::Failure::new(fold_foundation::FailureKind::InvalidInput, error)
        })?;
        let processors = graph::input_processors(&graph, config)?;
        crate::video_sources::reuse(&mut graph);
        crate::reuse::pure(&mut graph);
        let (mut needed, mut uses) = graph::dependencies(&graph);
        let fused = fusion::opacity_over(&graph, &mut needed, &mut uses);
        let chains = pointwise::plan(&graph, &mut needed, &mut uses);
        let prepare_nanoseconds = evaluate_begin.elapsed().as_nanos();
        let mut stats = Statistics {
            graph_prepare_nanoseconds: prepare_nanoseconds,
            pressure_evictions,
            ..Default::default()
        };
        let mut sources: Vec<Option<GpuFrame>> = (0..graph.nodes.len()).map(|_| None).collect();
        for (id, op) in graph.nodes.iter().enumerate() {
            if needed[id]
                && let ImageOp::Source(source) = op
            {
                let frame = source.render(&self.host, cancel)?;
                if frame.host.id() != self.host.id()
                    || frame.dimensions() != source.dimensions()
                    || frame.working_space != WorkingSpace::LegacyLinearSrgb
                    || frame.config_identity.is_some()
                {
                    return Err("External source returned an incompatible frame".into());
                }
                stats.accumulate(&frame.statistics);
                sources[id] = Some(frame);
            }
            if needed[id]
                && let ImageOp::Spatial { scene, binding } = op
            {
                #[cfg(feature = "spatial")]
                {
                    let _permit = class
                        .map(|class| crate::scheduling::Scheduler::shared().enter(class, cancel))
                        .transpose()?;
                    if self.spatial.is_none() {
                        self.spatial = Some(spatial::Adapter::new(self.host.clone())?);
                    }
                    let frame = self
                        .spatial
                        .as_mut()
                        .unwrap()
                        .render(scene, binding.clone(), cancel)?
                        .into_image(binding)?;
                    stats.accumulate(&frame.statistics);
                    sources[id] = Some(frame);
                }
                #[cfg(not(feature = "spatial"))]
                {
                    let _ = (scene, binding);
                    return Err("Spatial rendering is not enabled in this build".into());
                }
            }
            if needed[id]
                && let ImageOp::WindowedSource { source, .. } = op
            {
                let native =
                    crate::sampling::source_dimensions(source).ok_or("invalid region source")?;
                let frame = self.evaluate_admitted(
                    RenderGraph {
                        width: native[0],
                        height: native[1],
                        nodes: vec![*source.clone()],
                        output: 0,
                    },
                    config,
                    decoder,
                    cancel,
                    class,
                )?;
                // Queue ordering protects the input copy. Carry completion and
                // validation into the descendant rather than stalling this worker.
                stats.accumulate(&frame.statistics);
                sources[id] = Some(frame);
            }
        }
        let dimensions = [graph.width, graph.height];
        let mut keys = Vec::with_capacity(graph.nodes.len());
        let mut resident = Vec::with_capacity(graph.nodes.len());
        let mut requests = Vec::new();
        for (id, op) in graph.nodes.iter().enumerate() {
            let key = if needed[id] {
                still::Key::new(op, dimensions, &processors)?
                    .filter(|key| still::Cache::eligible(key, self.host.memory().budget))
            } else {
                None
            };
            let image = key
                .as_ref()
                .map(|key| self.host.sources().get(key, cancel))
                .transpose()?
                .flatten();
            if needed[id]
                && image.is_none()
                && let ImageOp::Exr {
                    source, channels, ..
                } = op
            {
                requests.push((
                    id,
                    fold_media::exr::cache::Request {
                        source: source.clone(),
                        channels: channels.clone(),
                        dimensions,
                    },
                ));
            }
            keys.push(key);
            resident.push(image);
        }
        let exr = crate::preparation::SourcePipeline::new(requests, cancel)?;
        let dependencies = sources
            .iter()
            .flatten()
            .flat_map(|f| std::iter::once(f.ready.clone()).chain(f.dependencies.iter().cloned()))
            .collect();
        let video = if aces {
            self.prepare_video(&graph, &needed, decoder, cancel, &mut stats)?
        } else {
            Vec::new()
        };
        Ok(Prepared {
            graph,
            aces,
            processors,
            needed,
            uses,
            fused,
            chains,
            stats,
            sources,
            dimensions,
            keys,
            resident,
            exr,
            dependencies,
            video,
            evaluate_begin,
        })
    }
}
