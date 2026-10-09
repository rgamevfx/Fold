//! Speculative source-only preparation. Uses the same immutable source requests
//! as physical execution, without color processing, GPU allocation or publication.
use crate::{ImageOp, RenderGraph, graph};
use fold_media::{
    Cancel,
    exr::cache::{Request, request},
};

pub struct PreparedSources {
    _tickets: Vec<fold_media::exr::cache::Ticket>,
    _allowance: std::sync::Arc<fold_media::budget::Lease>,
}
pub fn prepare_sources(
    graph: &RenderGraph,
    cancel: &Cancel,
) -> Result<PreparedSources, fold_foundation::Failure> {
    if graph.nodes.is_empty() || graph.output >= graph.nodes.len() {
        return Err("invalid source preparation graph".into());
    }
    // Validate topology before walking dependencies; malformed/disconnected
    // operators must not panic or bypass the renderer's normal validation.
    // Color assignments are validated against the actual config at evaluation.
    // Preparation only reads untransformed samples.
    graph::validate(graph, true)?;
    let (needed, _) = graph::dependencies(graph);
    let mut requests = Vec::new();
    for (op, needed) in graph.nodes.iter().zip(needed) {
        cancel.check()?;
        if !needed {
            continue;
        }
        let op = match op {
            ImageOp::WindowedSource { source, .. } => source.as_ref(),
            _ => op,
        };
        if let ImageOp::Exr {
            source, channels, ..
        } = op
        {
            let request = Request {
                source: source.clone(),
                channels: channels.clone(),
                dimensions: source.info.dimensions,
            };
            if !requests.contains(&request) {
                requests.push(request);
            }
        }
    }
    let bytes = requests
        .iter()
        .map(|r| u64::from(r.dimensions[0]) * u64::from(r.dimensions[1]) * 16)
        .sum();
    let allowance = fold_media::budget::reserve_preparation(bytes)?;
    let mut tickets = Vec::new();
    for input in requests {
        let ticket = request(input, false, cancel)?;
        ticket.wait(cancel)?;
        tickets.push(ticket);
    }
    Ok(PreparedSources {
        _tickets: tickets,
        _allowance: allowance,
    })
}

/// A small demand window overlaps block decoding with encoding/uploading the
/// preceding input. Tickets protect ready samples until the consumer uses them.
#[cfg(feature = "gpu")]
pub(crate) struct SourcePipeline {
    pending: std::collections::VecDeque<(usize, Request)>,
    tickets: std::collections::VecDeque<(usize, u64, fold_media::exr::cache::Ticket)>,
    bytes: u64,
}
#[cfg(feature = "gpu")]
impl SourcePipeline {
    pub fn new(
        requests: impl IntoIterator<Item = (usize, Request)>,
        cancel: &Cancel,
    ) -> Result<Self, fold_foundation::Failure> {
        let mut pipeline = Self {
            pending: requests.into_iter().collect(),
            tickets: Default::default(),
            bytes: 0,
        };
        pipeline.fill(cancel)?;
        Ok(pipeline)
    }
    fn fill(&mut self, cancel: &Cancel) -> Result<(), fold_foundation::Failure> {
        while let Some((_, request)) = self.pending.front() {
            let bytes = u64::from(request.dimensions[0]) * u64::from(request.dimensions[1]) * 16;
            if !self.tickets.is_empty()
                && (self.bytes + bytes > fold_media::budget::decoded_usage().budget / 3
                    || self.tickets.len() >= 4)
            {
                break;
            }
            let (id, request) = self.pending.pop_front().unwrap();
            let ticket = self::request(request, true, cancel)?;
            self.tickets.push_back((id, bytes, ticket));
            self.bytes += bytes;
        }
        Ok(())
    }
    pub fn take(
        &mut self,
        id: usize,
        cancel: &Cancel,
    ) -> Result<std::sync::Arc<fold_media::exr::Pixels>, fold_foundation::Failure> {
        let (expected, bytes, ticket) = self
            .tickets
            .pop_front()
            .ok_or("missing prepared EXR input")?;
        if id != expected {
            return Err("EXR preparation order differs from evaluation".into());
        }
        let pixels = ticket.wait(cancel)?;
        drop(ticket);
        self.bytes -= bytes;
        self.fill(cancel)?;
        Ok(pixels)
    }
}
