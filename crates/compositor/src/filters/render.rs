use crate::OperationConfig as P;
use fold_foundation::Time;
use fold_render::{ImageId, ImageOp, RenderGraph, shader::Program};
use std::sync::{Arc, OnceLock};

pub(super) fn program(
    graph: &mut RenderGraph,
    source: &str,
    inputs: Vec<ImageId>,
    values: Vec<[f32; 4]>,
    logical_size: [f32; 2],
) -> Result<ImageId, String> {
    let program = Program {
        source: source.into(),
        images: if inputs.len() == 2 {
            vec!["source".into(), "glow".into()]
        } else {
            vec!["source".into()]
        },
        values,
        logical_size,
        time: 0.,
        progress: 0.,
    };
    program.validate()?;
    let id = graph.nodes.len();
    graph.nodes.push(ImageOp::Shader {
        inputs,
        program: Arc::new(program),
    });
    Ok(id)
}
fn lens_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let mut taps=String::from("var result=vec4<f32>(0.);\n");
        for y in -1..=2 { for x in -1..=2 {
            taps.push_str(&format!("result+=tap(base+vec2<f32>({x}.,{y}.))*cubic(f.x - ({x}.))*cubic(f.y - ({y}.));\n"));
        } }
        taps.push_str("return result;");
        include_str!("lens.wgsl").replace("// CUBIC_TAPS", &taps).replace("// INVERSE_STEPS", &"q=solve_step(q,p);\n".repeat(16))
    })
}

pub(crate) fn compile(
    graph: &mut RenderGraph,
    config: &P,
    input: ImageId,
    scale: [f64; 2],
    time: Time,
    aces: bool,
) -> Result<ImageId, String> {
    let size = [
        (graph.width as f64 / scale[0]) as f32,
        (graph.height as f64 / scale[1]) as f32,
    ];
    let luma = if aces {
        [0.2722287, 0.6740818, 0.0536895, 0.]
    } else {
        [0.2126, 0.7152, 0.0722, 0.]
    };
    match config {
        P::ColorShift { settings } => color_shift(graph, settings, input, size, aces, false),
        P::ChromaticAberration { settings: s } => {
            if s.amount == 0. || s.protect >= 1. {
                return Ok(input);
            }
            let delta = program(
                graph,
                include_str!("aberration.wgsl"),
                vec![input],
                vec![
                    [s.amount as f32, s.red as f32, s.green as f32, s.blue as f32],
                    [s.center_x as f32, s.center_y as f32, s.falloff as f32, 0.],
                    [
                        0.,
                        0.,
                        0.,
                        f32::from(s.edges == fold_render::operations::Edges::Clamp),
                    ],
                    [
                        s.red_x as f32,
                        s.red_y as f32,
                        s.green_x as f32,
                        s.green_y as f32,
                    ],
                    [s.blue_x as f32, s.blue_y as f32, 0., 0.],
                ],
                size,
            )?;
            let delta = if s.blur > 0. {
                let id = graph.nodes.len();
                graph.nodes.push(ImageOp::FilteredBlur {
                    input: delta,
                    size: [(s.blur * scale[0]) as f32, (s.blur * scale[1]) as f32],
                    filter: fold_render::operations::BlurFilter::Gaussian,
                    edges: s.edges,
                });
                id
            } else {
                delta
            };
            program(
                graph,
                include_str!("aberration_combine.wgsl"),
                vec![input, delta],
                vec![
                    [s.saturation as f32, s.protect as f32, 0., 0.],
                    [s.center_x as f32, s.center_y as f32, 0., 0.],
                    luma,
                ],
                size,
            )
        }
        P::LensDistortion { settings: s } => {
            if s.distortion == 0.
                && s.quartic == 0.
                && s.sextic == 0.
                && s.tangent_x == 0.
                && s.tangent_y == 0.
                && s.scale == 1.
            {
                return Ok(input);
            }
            use fold_render::operations::{Edges, Filter};
            program(
                graph,
                lens_source(),
                vec![input],
                vec![
                    [
                        s.distortion as f32,
                        s.quartic as f32,
                        s.sextic as f32,
                        f32::from(s.undistort),
                    ],
                    [
                        s.tangent_x as f32,
                        s.tangent_y as f32,
                        s.center_x as f32,
                        s.center_y as f32,
                    ],
                    [
                        match s.filter {
                            Filter::Nearest => 0.,
                            Filter::Linear => 1.,
                            Filter::Cubic => 2.,
                        },
                        f32::from(s.edges == Edges::Clamp),
                        s.squeeze as f32,
                        s.scale as f32,
                    ],
                ],
                size,
            )
        }
        P::FilmEmulation { settings: s } => {
            if s.strength == 0. {
                return Ok(input);
            }
            let glow = if s.halation > 0. && s.halation_radius > 0. {
                let extract = program(
                    graph,
                    "fn effect(uv:vec2<f32>)->vec4<f32>{let p=source(uv);let threshold=fold_params.values[0].x;let rgb=max(p.rgb-vec3<f32>(threshold*p.a),vec3<f32>(0.));return vec4<f32>(rgb,p.a);}",
                    vec![input],
                    vec![[s.halation_threshold as f32, 0., 0., 0.]],
                    size,
                )?;
                let id = graph.nodes.len();
                graph.nodes.push(ImageOp::FilteredBlur {
                    input: extract,
                    size: [
                        (s.halation_radius * scale[0]) as f32,
                        (s.halation_radius * scale[1]) as f32,
                    ],
                    filter: fold_render::operations::BlurFilter::Gaussian,
                    edges: fold_render::operations::Edges::Transparent,
                });
                id
            } else {
                let id = graph.nodes.len();
                graph.nodes.push(ImageOp::Solid { rgba: [0.; 4] });
                id
            };
            // Original scene-linear looks: contrast, chroma, black lift, shoulder, RGB exposure bias.
            let (stock, tint) = match s.preset {
                super::FilmPreset::NeutralNegative => {
                    ([0.96, 0.93, 0.002, 0.12], [0.025, 0., -0.025, 0.])
                }
                super::FilmPreset::WarmPortrait => {
                    ([0.91, 0.86, 0.004, 0.2], [0.09, 0.02, -0.08, 0.])
                }
                super::FilmPreset::CoolDaylight => {
                    ([1.02, 0.92, 0.001, 0.13], [-0.05, 0.01, 0.08, 0.])
                }
                super::FilmPreset::TungstenCinema => {
                    ([1.06, 0.88, 0.002, 0.22], [-0.06, 0.015, 0.12, 0.])
                }
                super::FilmPreset::VividSlide => ([1.16, 1.2, 0., 0.08], [0.04, -0.015, 0.015, 0.]),
                super::FilmPreset::FadedPrint => {
                    ([0.8, 0.72, 0.026, 0.28], [0.07, 0.025, -0.075, 0.])
                }
                super::FilmPreset::SoftMonochrome => ([0.92, 0., 0.009, 0.24], [0.; 4]),
                super::FilmPreset::HardMonochrome => ([1.3, 0., 0., 0.12], [0.; 4]),
            };
            program(
                graph,
                include_str!("film.wgsl"),
                vec![input, glow],
                vec![
                    [
                        s.strength as f32,
                        s.exposure as f32,
                        s.contrast as f32,
                        s.saturation as f32,
                    ],
                    [
                        s.rolloff as f32,
                        s.shadow_tint as f32,
                        s.highlight_tint as f32,
                        s.halation as f32,
                    ],
                    stock,
                    tint,
                    luma,
                ],
                size,
            )
        }
        P::Grain { settings: s } => {
            if s.amount == 0. {
                return Ok(input);
            }
            let frame = if s.freeze {
                0
            } else {
                (fold_animation::seconds(time) * s.rate).floor() as i64
            };
            let seed = s.seed.round() as u32;
            let temporal = seed.wrapping_add((frame as u32).wrapping_mul(747796405)) % 16777216;
            program(
                graph,
                include_str!("grain.wgsl"),
                vec![input],
                vec![
                    [
                        s.amount as f32,
                        s.size as f32,
                        s.color as f32,
                        f32::from(s.mode == super::NoiseMode::Sensor),
                    ],
                    [s.shadows as f32, s.midtones as f32, s.highlights as f32, 0.],
                    [s.red as f32, s.green as f32, s.blue as f32, 0.],
                    [
                        s.softness as f32,
                        s.clumping as f32,
                        temporal as f32,
                        seed as f32,
                    ],
                    [s.shot as f32, s.read as f32, s.fixed as f32, 0.],
                    luma,
                ],
                size,
            )
        }
        _ => Err("Not a photographic filter".into()),
    }
}

pub(crate) fn color_shift(
    graph: &mut RenderGraph,
    s: &super::ColorShift,
    input: ImageId,
    size: [f32; 2],
    aces: bool,
    matte: bool,
) -> Result<ImageId, String> {
    if !matte && !s.use_target && s.hue == 0. && s.saturation == 1. && s.brightness == 0. {
        return Ok(input);
    }
    program(
        graph,
        include_str!("color_shift.wgsl"),
        vec![input],
        vec![
            s.selected.map(|v| v as f32),
            s.target.map(|v| v as f32),
            [
                s.range as f32,
                s.softness as f32,
                f32::from(s.use_target),
                0.,
            ],
            [
                s.hue_range as f32,
                s.saturation_range as f32,
                s.brightness_range as f32,
                0.,
            ],
            [s.hue as f32, s.saturation as f32, s.brightness as f32, 0.],
            [f32::from(matte), 0., 0., f32::from(aces)],
            if aces {
                [0.2722287, 0.6740818, 0.0536895, 0.]
            } else {
                [0.2126, 0.7152, 0.0722, 0.]
            },
        ],
        size,
    )
}
