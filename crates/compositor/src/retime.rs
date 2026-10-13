//! Time remapping requests upstream nodes at exact rational frame boundaries.
use crate::{OperationConfig, PreparedComposite, animation::Property};
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Nearest,
    Blend,
    Rife,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outside {
    Hold,
    Black,
    Loop,
    Bounce,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retime {
    pub source_frame: f64,
    pub offset: f64,
    pub first: f64,
    pub last: f64,
    /// Frame coordinates refer to this upstream sampling grid, initialized from the composition.
    pub rate: [u32; 2],
    pub interpolation: Interpolation,
    pub outside: Outside,
    #[serde(default = "default_cut_threshold")]
    pub cut_threshold: f64,
    #[serde(default = "default_motion_scale")]
    pub motion_scale: u32,
    pub motion_blur: bool,
    pub shutter_angle: f64,
    /// Offset in output frames from a centered shutter.
    pub shutter_offset: f64,
    pub samples: u32,
}
fn default_cut_threshold() -> f64 {
    0.35
}
fn default_motion_scale() -> u32 {
    1
}
impl Default for Retime {
    fn default() -> Self {
        Self {
            source_frame: 0.,
            offset: 0.,
            first: 0.,
            last: 239.,
            rate: [24, 1],
            interpolation: Interpolation::Blend,
            outside: Outside::Hold,
            cut_threshold: default_cut_threshold(),
            motion_scale: 1,
            motion_blur: false,
            shutter_angle: 180.,
            shutter_offset: 0.,
            samples: 8,
        }
    }
}
impl Retime {
    pub fn properties(&self) -> Vec<Property> {
        [
            (
                "source_frame",
                "Source frame",
                "frames",
                self.source_frame,
                [-10000000., 10000000.],
            ),
            (
                "offset",
                "Frame offset",
                "frames",
                self.offset,
                [-10000000., 10000000.],
            ),
            (
                "first",
                "First frame",
                "frames",
                self.first,
                [-10000000., 10000000.],
            ),
            (
                "last",
                "Last frame",
                "frames",
                self.last,
                [-10000000., 10000000.],
            ),
            (
                "shutter_angle",
                "Shutter angle",
                "°",
                self.shutter_angle,
                [0., 720.],
            ),
            (
                "shutter_offset",
                "Shutter offset",
                "frames",
                self.shutter_offset,
                [-2., 2.],
            ),
        ]
        .into_iter()
        .map(|(id, label, unit, value, range)| Property {
            id: id.into(),
            label: label.into(),
            unit,
            channels: &["Value"],
            values: vec![value],
            range: Some(range),
        })
        .collect()
    }
    pub fn set_property(&mut self, id: &str, values: &[f64]) -> Result<(), String> {
        let [v] = values else {
            return Err("Expected one retime value".into());
        };
        if !v.is_finite() {
            return Err("Retime requires finite values".into());
        }
        *match id {
            "source_frame" => &mut self.source_frame,
            "offset" => &mut self.offset,
            "first" => &mut self.first,
            "last" => &mut self.last,
            "shutter_angle" => &mut self.shutter_angle,
            "shutter_offset" => &mut self.shutter_offset,
            _ => return Err("Unknown retime property".into()),
        } = *v;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        for p in self.properties() {
            let [a, b] = p.range.unwrap();
            if !p.values[0].is_finite() || !(a..=b).contains(&p.values[0]) {
                return Err(format!("{} outside {a}..{b}", p.label));
            }
        }
        if self.first > self.last
            || self.rate.contains(&0)
            || self.rate[0] > 1000000
            || self.rate[1] > 1000000
            || !(1..=64).contains(&self.samples)
            || ![1, 2, 4].contains(&self.motion_scale)
            || !self.cut_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.cut_threshold)
        {
            return Err(
                "Retime requires an ordered frame range, valid rate and 1..64 samples".into(),
            );
        }
        Ok(())
    }
    fn frame_time(&self, frame: i64) -> Result<Time, String> {
        Time::new(
            frame
                .checked_mul(i64::from(self.rate[1]))
                .ok_or("Frame overflow")?,
            self.rate[0],
        )
        .map_err(|e| e.to_string())
    }
    pub fn identity_curve(&self) -> Result<fold_animation::Curve, String> {
        let mut curve = fold_animation::Curve::default();
        curve.insert(Time::ZERO, 0.);
        let last = self.last.max(1.).ceil() as i64;
        curve.insert(self.frame_time(last)?, last as f64);
        Ok(curve)
    }
    fn boundary(&self, frame: i64) -> Option<i64> {
        let first = self.first.ceil() as i64;
        let last = self.last.floor() as i64;
        if first > last {
            return None;
        }
        Some(match self.outside {
            Outside::Hold => frame.clamp(first, last),
            Outside::Black => {
                if frame < first || frame > last {
                    return None;
                }
                frame
            }
            Outside::Loop => first + (frame - first).rem_euclid(last - first + 1),
            Outside::Bounce => {
                let span = last - first;
                if span == 0 {
                    first
                } else {
                    let p = (frame - first).rem_euclid(2 * span);
                    first + p.min(2 * span - p)
                }
            }
        })
    }
}

pub(crate) fn initial_channels(config: &OperationConfig) -> crate::animation::Channels {
    let mut channels = crate::animation::Channels::default();
    if let OperationConfig::Retime { settings } = config {
        if let Ok(curve) = settings.identity_curve() {
            channels
                .insert("source_frame.0".into(), curve)
                .expect("valid initial retime curve");
        }
    }
    channels
}

/// Coalesce samples by requested frame, including transparent samples. Identical
/// shutter samples on a freeze therefore request exactly one upstream frame.
pub(crate) fn samples(
    graph: &PreparedComposite<'_>,
    id: ObjectId,
    time: Time,
) -> Result<Vec<(Option<Time>, f64)>, String> {
    let OperationConfig::Retime { settings: center } = graph.evaluated_parameters(id, time)? else {
        return Err("Expected Retime".into());
    };
    let count = if center.motion_blur && center.shutter_angle > 0. {
        center.samples
    } else {
        1
    };
    let mut weights = BTreeMap::<Option<Time>, f64>::new();
    for i in 0..count {
        let delta = if count == 1 {
            0.
        } else {
            center.shutter_offset
                + ((f64::from(i) + 0.5) / f64::from(count) - 0.5) * center.shutter_angle / 360.
        };
        // Quantize only the UI-authored shutter fraction, then use rational arithmetic.
        let tick = Time::new(i64::from(graph.info.rate[1]), graph.info.rate[0])
            .map_err(|e| e.to_string())?;
        let at = time
            .checked_add(
                tick.checked_scale((delta * 1000000.).round() as i64, 1000000)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let OperationConfig::Retime { settings } = graph.evaluated_parameters(id, at)? else {
            unreachable!()
        };
        let frame = settings.source_frame + settings.offset;
        let nearest = frame.round() as i64;
        let base = frame.floor() as i64;
        let fraction = frame - base as f64;
        let parts = match center.interpolation {
            Interpolation::Nearest => vec![(nearest, 1.)],
            Interpolation::Rife => return Err("RIFE samples require temporal synthesis".into()),
            Interpolation::Blend => vec![(base, 1. - fraction), (base + 1, fraction)],
        };
        for (frame, weight) in parts {
            if weight <= 1e-12 {
                continue;
            }
            let at = center
                .boundary(frame)
                .map(|f| center.frame_time(f))
                .transpose()?;
            *weights.entry(at).or_default() += weight / f64::from(count);
        }
    }
    Ok(weights.into_iter().collect())
}

/// Keep fractional synthesis requests distinct until after interpolation. A shutter
/// exposure cannot be collapsed into source-frame blend weights for nonlinear RIFE.
#[derive(Clone, Debug)]
pub(crate) struct Sample {
    pub first: Option<Time>,
    pub second: Option<Time>,
    pub fraction: f64,
    pub weight: f64,
}
fn requests(
    graph: &PreparedComposite<'_>,
    id: ObjectId,
    time: Time,
) -> Result<Vec<Sample>, String> {
    let OperationConfig::Retime { settings: center } = graph.evaluated_parameters(id, time)? else {
        unreachable!()
    };
    if center.interpolation != Interpolation::Rife {
        return Ok(samples(graph, id, time)?
            .into_iter()
            .map(|(at, weight)| Sample {
                first: at,
                second: at,
                fraction: 0.,
                weight,
            })
            .collect());
    }
    let count = if center.motion_blur && center.shutter_angle > 0. {
        center.samples
    } else {
        1
    };
    let tick =
        Time::new(i64::from(graph.info.rate[1]), graph.info.rate[0]).map_err(|e| e.to_string())?;
    let mut values = BTreeMap::new();
    for i in 0..count {
        let delta = if count == 1 {
            0.
        } else {
            center.shutter_offset
                + ((f64::from(i) + 0.5) / f64::from(count) - 0.5) * center.shutter_angle / 360.
        };
        let at = time
            .checked_add(
                tick.checked_scale((delta * 1000000.).round() as i64, 1000000)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let OperationConfig::Retime { settings } = graph.evaluated_parameters(id, at)? else {
            unreachable!()
        };
        let frame = settings.source_frame + settings.offset;
        let base = frame.floor() as i64;
        let mut fraction = frame - base as f64;
        let first = center
            .boundary(base)
            .map(|f| center.frame_time(f))
            .transpose()?;
        let second = if fraction <= 1e-12 {
            fraction = 0.;
            first
        } else {
            center
                .boundary(base + 1)
                .map(|f| center.frame_time(f))
                .transpose()?
        };
        if first == second {
            fraction = 0.;
        }
        *values
            .entry((first, second, fraction.to_bits()))
            .or_insert(0.) += 1. / f64::from(count);
    }
    Ok(values
        .into_iter()
        .map(|((first, second, fraction), weight)| Sample {
            first,
            second,
            fraction: f64::from_bits(fraction),
            weight,
        })
        .collect())
}

pub(crate) type Key = (ObjectId, Time);
pub(crate) struct Plan {
    pub order: Vec<Key>,
    pub samples: BTreeMap<Key, Vec<Sample>>,
}
/// Iterative postorder avoids recursive evaluation and shares branch/frame requests.
pub(crate) fn plan(
    graph: &PreparedComposite<'_>,
    output: ObjectId,
    time: Time,
    cancel: &fold_media::Cancel,
) -> Result<Plan, String> {
    graph.render_order_from(output)?;
    let mut plan = Plan {
        order: vec![],
        samples: BTreeMap::new(),
    };
    let mut done = BTreeSet::new();
    let mut stack = vec![((output, time), false)];
    let mut visits = 0;
    while let Some((key, ready)) = stack.pop() {
        cancel.check()?;
        if done.contains(&key) {
            continue;
        }
        visits += 1;
        if visits > 32768 || plan.order.len() > 4096 {
            return Err("Temporal graph exceeds 4096 evaluations; reduce retime samples".into());
        }
        if ready {
            done.insert(key);
            plan.order.push(key);
            continue;
        }
        stack.push((key, true));
        let node = graph.node(key.0)?;
        if node.disabled {
            if let Some(input) = node
                .config()
                .primary_slot()
                .and_then(|slot| node.input_ids().get(slot).copied().flatten())
            {
                stack.push(((input, key.1), false));
            }
        } else if matches!(node.config(), OperationConfig::Retime { .. }) {
            let requests = requests(graph, key.0, key.1)?;
            let input = node.input_ids()[0].ok_or("Retime: connect image input")?;
            for sample in &requests {
                for at in [sample.first, sample.second].into_iter().flatten() {
                    stack.push(((input, at), false));
                }
            }
            plan.samples.insert(key, requests);
        } else {
            for input in node.input_ids().iter().flatten() {
                stack.push(((*input, key.1), false));
            }
            if let Some(mask) = node
                .effect
                .mask_node()
                .filter(|_| !node.effect.mask_disabled && !node.background_only_merge())
            {
                stack.push(((mask, key.1), false));
            }
        }
    }
    Ok(plan)
}

pub(crate) fn synthesize(
    graph: &mut fold_render::RenderGraph,
    requests: &[Sample],
    input: ObjectId,
    cache: &BTreeMap<Key, fold_render::channels::Channels>,
    names: &[fold_render::channels::ChannelName],
    settings: &Retime,
) -> Result<fold_render::channels::Channels, String> {
    use fold_render::{
        ImageOp,
        channels::{Channels, RGBA},
    };
    if settings.interpolation != Interpolation::Rife {
        return blend(
            graph,
            &requests
                .iter()
                .map(|s| (s.first, s.weight))
                .collect::<Vec<_>>(),
            input,
            cache,
            names,
        );
    }
    let mut accumulated = None;
    let mut total = 0.;
    for s in requests {
        let mut frame = |at: Option<Time>| -> Result<usize, String> {
            match at {
                Some(at) => cache
                    .get(&(input, at))
                    .ok_or("Missing RIFE source frame")?
                    .pack(graph, RGBA),
                None => {
                    let id = graph.nodes.len();
                    graph.nodes.push(ImageOp::Solid { rgba: [0.; 4] });
                    Ok(id)
                }
            }
        };
        let first = frame(s.first)?;
        let image = if s.fraction == 0. || s.first == s.second {
            first
        } else {
            let second = frame(s.second)?;
            let id = graph.nodes.len();
            // Transparent range boundaries use the defined ordinary fade, not invented motion.
            if s.first.is_none() || s.second.is_none() {
                graph.nodes.push(ImageOp::Mix {
                    original: first,
                    processed: second,
                    mask: None,
                    mask_channel: 0,
                    invert: false,
                    amount: s.fraction as f32,
                    channels: [true; 4],
                });
            } else {
                graph.nodes.push(ImageOp::Rife {
                    first,
                    second,
                    fraction: s.fraction as f32,
                    motion_scale: settings.motion_scale,
                    cut_threshold: settings.cut_threshold as f32,
                });
            }
            id
        };
        let image = if let Some(previous) = accumulated {
            let id = graph.nodes.len();
            graph.nodes.push(ImageOp::Mix {
                original: previous,
                processed: image,
                mask: None,
                mask_channel: 0,
                invert: false,
                amount: (s.weight / (total + s.weight)) as f32,
                channels: [true; 4],
            });
            id
        } else {
            image
        };
        accumulated = Some(image);
        total += s.weight;
    }
    let mut output = Channels::rgba(accumulated.ok_or("Retime has no samples")?);
    // Auxiliary data keeps its established temporal sampling; RGB/alpha share inferred motion.
    let extras: Vec<_> = names
        .iter()
        .filter(|n| !RGBA.contains(&n.as_str()))
        .cloned()
        .collect();
    if !extras.is_empty() {
        let mut weights = BTreeMap::new();
        for s in requests {
            *weights.entry(s.first).or_insert(0.) += s.weight * (1. - s.fraction);
            *weights.entry(s.second).or_insert(0.) += s.weight * s.fraction;
        }
        let data = blend(
            graph,
            &weights
                .into_iter()
                .filter(|(_, w)| *w > 0.)
                .collect::<Vec<_>>(),
            input,
            cache,
            &extras,
        )?;
        for name in extras {
            output.insert(name.clone(), data.get(name.as_str())?);
        }
    }
    Ok(output)
}

pub(crate) fn blend(
    graph: &mut fold_render::RenderGraph,
    requests: &[(Option<Time>, f64)],
    input: ObjectId,
    cache: &BTreeMap<Key, fold_render::channels::Channels>,
    names: &[fold_render::channels::ChannelName],
) -> Result<fold_render::channels::Channels, String> {
    use fold_render::{
        ImageOp,
        channels::{Channel, Channels, Interpretation, Value, pack},
    };
    let mut result = Channels::default();
    // Pack data in RGB with opaque alpha; arbitrary channels can exceed 0..1.
    for group in names.chunks(3) {
        let mut accumulated = None;
        let mut total = 0.;
        for (time, weight) in requests {
            let source = time.and_then(|t| cache.get(&(input, t)));
            let mut values = [Value::Zero, Value::Zero, Value::Zero, Value::One];
            for (i, name) in group.iter().enumerate() {
                values[i] = source
                    .and_then(|c| c.get(name.as_str()).ok())
                    .map(|c| c.value)
                    .unwrap_or(Value::Zero);
            }
            let image = pack(graph, values)?;
            let image = if let Some(previous) = accumulated {
                let id = graph.nodes.len();
                graph.nodes.push(ImageOp::Mix {
                    original: previous,
                    processed: image,
                    mask: None,
                    mask_channel: 0,
                    invert: false,
                    amount: (weight / (total + weight)) as f32,
                    channels: [true; 4],
                });
                id
            } else {
                image
            };
            accumulated = Some(image);
            total += weight;
        }
        let image = accumulated.ok_or("Retime has no samples")?;
        for (i, name) in group.iter().enumerate() {
            let interpretation = requests
                .iter()
                .filter_map(|(time, _)| time.and_then(|t| cache.get(&(input, t))))
                .find_map(|c| c.get(name.as_str()).ok())
                .map(|c| c.interpretation)
                .unwrap_or(Interpretation::Data);
            result.insert(
                name.clone(),
                Channel {
                    value: Value::Component {
                        image,
                        component: i as u8,
                    },
                    interpretation,
                },
            );
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rife_requests(time: Time, blur: bool, speed: f64) -> Vec<Sample> {
        let source = crate::Node::new(OperationConfig::Solid { rgba: [1.; 4] }, vec![]);
        let mut retime = crate::Node::new(
            OperationConfig::Retime {
                settings: Retime {
                    interpolation: Interpolation::Rife,
                    motion_blur: blur,
                    samples: 4,
                    ..Default::default()
                },
            },
            vec![source.id],
        );
        let mut curve = fold_animation::Curve::default();
        curve.insert(Time::ZERO, 0.);
        curve.insert(Time::new(1, 1).unwrap(), 24. * speed);
        retime
            .animation
            .insert("source_frame.0".into(), curve)
            .unwrap();
        let id = retime.id;
        let output = crate::Node::new(OperationConfig::Output, vec![id]);
        let graph = crate::Composite {
            info: fold_media::VideoInfo {
                width: 1,
                height: 1,
                rate: [24, 1],
                frames: 240,
            },
            output: output.id,
            nodes: vec![source, retime, output],
            extensions: Default::default(),
        };
        requests(&graph.prepare().unwrap(), id, time).unwrap()
    }

    #[test]
    fn rife_preserves_fractional_shutter_samples_and_bypasses_exact_frames() {
        let exact = rife_requests(Time::new(1, 2).unwrap(), false, 1.);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].first, Some(Time::new(1, 2).unwrap()));
        assert_eq!(exact[0].first, exact[0].second);
        assert_eq!(exact[0].fraction, 0.);
        let fractional = rife_requests(Time::new(49, 96).unwrap(), false, 1.);
        assert_eq!(fractional.len(), 1);
        assert!((fractional[0].fraction - 0.25).abs() < 1e-10);
        let shutter = rife_requests(Time::new(49, 96).unwrap(), true, 1.);
        assert_eq!(shutter.len(), 4);
        assert!(
            shutter
                .iter()
                .all(|s| s.first == fractional[0].first && s.second == fractional[0].second)
        );
        assert!((shutter.iter().map(|s| s.fraction * s.weight).sum::<f64>() - 0.25).abs() < 1e-10);
        let frozen = rife_requests(Time::new(1, 2).unwrap(), true, 0.);
        assert_eq!(frozen.len(), 1);
        assert_eq!(frozen[0].fraction, 0.);
        assert_eq!(frozen[0].weight, 1.);
    }

    fn shutter_weights(count: u32, speed: f64) -> Vec<(Option<Time>, f64)> {
        let source = crate::Node::new(OperationConfig::Solid { rgba: [1.; 4] }, vec![]);
        let mut retime = crate::Node::new(
            OperationConfig::Retime {
                settings: Retime {
                    motion_blur: true,
                    samples: count,
                    ..Default::default()
                },
            },
            vec![source.id],
        );
        let mut curve = fold_animation::Curve::default();
        curve.insert(Time::ZERO, 0.);
        curve.insert(Time::new(1, 1).unwrap(), 24. * speed);
        retime
            .animation
            .insert("source_frame.0".into(), curve)
            .unwrap();
        let id = retime.id;
        let output = crate::Node::new(OperationConfig::Output, vec![id]);
        let graph = crate::Composite {
            info: fold_media::VideoInfo {
                width: 1,
                height: 1,
                rate: [24, 1],
                frames: 240,
            },
            output: output.id,
            nodes: vec![source, retime, output],
            extensions: Default::default(),
        };
        samples(&graph.prepare().unwrap(), id, Time::new(1, 2).unwrap()).unwrap()
    }

    #[test]
    fn shutter_sample_count_refines_fast_retime_but_can_converge_at_identity() {
        let preview = shutter_weights(4, 1.);
        let fine = shutter_weights(64, 1.);
        assert_eq!(preview.len(), 3);
        assert_eq!(preview.len(), fine.len());
        for ((a, wa), (b, wb)) in preview.iter().zip(&fine) {
            assert_eq!(a, b);
            assert!((wa - wb).abs() < 1e-10);
        }
        let fast_preview = shutter_weights(4, 16.);
        let fast_fine = shutter_weights(64, 16.);
        assert!(fast_fine.len() > fast_preview.len());
        for weights in [&preview, &fine, &fast_preview, &fast_fine] {
            assert!((weights.iter().map(|(_, w)| w).sum::<f64>() - 1.).abs() < 1e-10);
        }
        eprintln!(
            "identity 4/64: {preview:?} / {fine:?}; 16x source frames: {} / {}",
            fast_preview.len(),
            fast_fine.len()
        );
    }
}
