//! Request validation, dependency planning and leased source preparation.
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
        let memory = self.host.memory();
        if memory.allocated.saturating_sub(memory.reusable) > memory.budget.saturating_mul(3) / 4 {
            self.host.sources().clear();
            self.yuv.clear();
            self.vector.clear();
            self.coverage.clear();
            self.baked.clear();
            self.uploads.clear();
            #[cfg(all(feature = "native-video", target_os = "linux"))]
            if let Some(native) = self.native_video.as_mut() {
                native.clear();
            }
        }
        let evaluate_begin = std::time::Instant::now();
        let aces = config.is_some();
        crate::sampling::prepare(&mut graph);
        graph::validate(&graph, aces).map_err(|error| {
            fold_foundation::Failure::new(fold_foundation::FailureKind::InvalidInput, error)
        })?;
        let processors = graph::input_processors(&graph, config)?;
        crate::video_sources::reuse(&mut graph);
        let (mut needed, mut uses) = graph::dependencies(&graph);
        let fused = fusion::opacity_over(&graph, &mut needed, &mut uses);
        let chains = pointwise::plan(&graph, &mut needed, &mut uses);
        let prepare_nanoseconds = evaluate_begin.elapsed().as_nanos();
        let mut stats = Statistics {
            graph_prepare_nanoseconds: prepare_nanoseconds,
            ..Default::default()
        };
        let mut sources: Vec<Option<GpuFrame>> = (0..graph.nodes.len()).map(|_| None).collect();
        for (id, op) in graph.nodes.iter().enumerate() {
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
