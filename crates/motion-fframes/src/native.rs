use fframes::{Svgr, svgr};
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{
    model::{Canvas, Cap, Content, ImageFit, Join, Object, PaintSource, Point, Spread},
    scripts::Context,
};
use std::fmt::Write;

pub trait Scripts {
    fn render<'a>(&mut self, id: ObjectId, input: Context<'a>) -> Result<Svgr<'a>, String>;
    fn animate(
        &mut self,
        _id: ObjectId,
        _input: fold_motion_canvas::motion::Context<'_>,
    ) -> Result<fold_motion_canvas::motion::Pose, String> {
        Err("No compiled motion behavior".into())
    }
}

/// Evaluate only this script's local output. Its authored transform and opacity
/// stay on the original object after conversion, while inherited paint must be
/// resolved here to match the rendered script.
pub fn script_output<'a>(
    canvas: &'a Canvas,
    id: ObjectId,
    time: Time,
    scripts: &mut dyn Scripts,
) -> Result<Svgr<'a>, String> {
    canvas.validate()?;
    if time < Time::ZERO || time >= canvas.duration {
        return Err("Motion time is outside the document".into());
    }
    let object = canvas.objects.get(&id).ok_or("Missing script object")?;
    if !matches!(object.content, Content::Script { .. }) {
        return Err("Only script objects can be captured".into());
    }
    let output = scripts.render(
        id,
        Context {
            canvas,
            object,
            time,
        },
    )?;
    let mut local = object.clone();
    local.transform = Default::default();
    local.motion = None;
    let children = wrap(id, &local, time, output)?;
    let (width, height) = (canvas.width, canvas.height);
    let view_box = format!("0 0 {width} {height}");
    Ok(
        svgr!(<svg xmlns="http://www.w3.org/2000/svg" width={width} height={height} viewBox={view_box}>{children}</svg>),
    )
}

pub fn document<'a>(
    canvas: &'a Canvas,
    time: Time,
    scripts: &mut dyn Scripts,
) -> Result<Svgr<'a>, String> {
    canvas.validate()?;
    if time < Time::ZERO || time >= canvas.duration {
        return Err("Motion time is outside the document".into());
    }
    let children = canvas
        .roots
        .iter()
        .map(|id| object(canvas, *id, time, scripts))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .collect::<Svgr<'a>>();
    let width = canvas.width;
    let height = canvas.height;
    let view_box = format!("0 0 {width} {height}");
    Ok(
        svgr!(<svg xmlns="http://www.w3.org/2000/svg" width={width} height={height} viewBox={view_box}>{children}</svg>),
    )
}
fn object<'a>(
    canvas: &'a Canvas,
    id: ObjectId,
    time: Time,
    scripts: &mut dyn Scripts,
) -> Result<Svgr<'a>, String> {
    let value = &canvas.objects[&id];
    if !value.visible {
        return Ok(Svgr::default());
    }
    let children = match &value.content {
        Content::Image {
            asset,
            width,
            height,
            fit,
            ..
        } => {
            let width = dimension(width.at(time))?;
            let height = dimension(height.at(time))?;
            let href = asset.to_string();
            let aspect = match fit {
                ImageFit::Contain => "xMidYMid meet",
                ImageFit::Cover => "xMidYMid slice",
                ImageFit::Stretch => "none",
            };
            svgr!(<image href={href} width={width} height={height} preserveAspectRatio={aspect}/>)
        }
        Content::Rectangle {
            width,
            height,
            radius,
            ..
        } => {
            let (width, height, radius) = (
                dimension(width.at(time))?,
                dimension(height.at(time))?,
                dimension(radius.at(time))?,
            );
            svgr!(<rect width={width} height={height} rx={radius}/>)
        }
        Content::Ellipse {
            radius_x, radius_y, ..
        } => {
            let (rx, ry) = (dimension(radius_x.at(time))?, dimension(radius_y.at(time))?);
            svgr!(<ellipse rx={rx} ry={ry}/>)
        }
        Content::Text {
            text,
            font_family,
            size,
            layout,
            ..
        } => {
            let size = dimension(size.at(time))?;
            let tracking = layout.tracking;
            fold_motion_canvas::typography::layout(text, size, layout).into_iter().map(|line| {
                let (x,y)=(line.x,line.baseline);
                svgr!(<text x={x} y={y} font-family={font_family.clone()} font-size={size} letter-spacing={tracking}>{line.text}</text>)
            }).collect()
        }
        Content::Path { contours, .. } => {
            let mut path = String::new();
            for contour in contours {
                let Some(first) = contour.points.first() else {
                    continue;
                };
                let first = GeometryPoint::sample(first, time)?;
                write!(path, "M {} {} ", first.position[0], first.position[1])
                    .expect("String write");
                let mut previous = first;
                for point in contour.points.iter().skip(1) {
                    let next = GeometryPoint::sample(point, time)?;
                    segment(&mut path, &previous, &next);
                    previous = next;
                }
                if contour.closed {
                    if contour.points.len() > 1 {
                        segment(&mut path, &previous, &first);
                    }
                    path.push_str("Z ");
                }
            }
            svgr!(<path d={path}/>)
        }
        Content::Group { children, .. } => children
            .iter()
            .map(|id| object(canvas, *id, time, scripts))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect(),
        Content::Script { .. } => scripts.render(
            id,
            Context {
                canvas,
                object: value,
                time,
            },
        )?,
    };
    wrap(id, value, time, children)
}
fn dimension(value: f64) -> Result<f64, String> {
    if value.is_finite() && value >= 0. {
        Ok(value)
    } else {
        Err("Shape dimensions must be finite and nonnegative".into())
    }
}
fn wrap<'a>(
    id: ObjectId,
    object: &Object,
    time: Time,
    children: Svgr<'a>,
) -> Result<Svgr<'a>, String> {
    let values = [object.opacity(time), object.paint.stroke_width.at(time)];
    if !values.iter().all(|v| v.is_finite()) {
        return Err("Motion animation produced a nonfinite value".into());
    }
    let [opacity, width] = values;
    let [a, b, c, d, x, y] = object.affine(time)?;
    let transform = format!("matrix({a} {b} {c} {d} {x} {y})");
    let (fill, fill_opacity, fill_definition) =
        paint(object.paint.fill.as_ref(), format!("fold-fill-{id}"));
    let (stroke, stroke_opacity, stroke_definition) =
        paint(object.paint.stroke.as_ref(), format!("fold-stroke-{id}"));
    let opacity = opacity.clamp(0., 1.);
    let width = width.max(0.);
    let miter_limit = object.paint.miter_limit;
    let cap = match object.paint.cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    };
    let join = match object.paint.join {
        Join::Miter => "miter",
        Join::Round => "round",
        Join::Bevel => "bevel",
    };
    let rule = if object.paint.even_odd {
        "evenodd"
    } else {
        "nonzero"
    };
    Ok(
        svgr!(<g transform={transform} opacity={opacity} fill={fill} fill-opacity={fill_opacity} stroke={stroke} stroke-opacity={stroke_opacity} stroke-width={width} stroke-linecap={cap} stroke-linejoin={join} stroke-miterlimit={miter_limit} fill-rule={rule}>{fill_definition}{stroke_definition}{children}</g>),
    )
}
fn color([r, g, b, _]: [f64; 4]) -> String {
    format!("rgb({}% {}% {}%)", r * 100., g * 100., b * 100.)
}
fn paint<'a>(source: Option<&PaintSource>, id: String) -> (String, f64, Svgr<'a>) {
    match source {
        None => ("none".into(), 1., std::iter::empty::<Svgr<'a>>().collect()),
        Some(PaintSource::Solid { color: value, .. }) => (
            color(*value),
            value[3],
            std::iter::empty::<Svgr<'a>>().collect(),
        ),
        Some(PaintSource::ImagePattern {
            asset,
            origin: [x, y],
            size: [width, height],
            transform: [a, b, c, d, tx, ty],
            ..
        }) => {
            let reference = format!("url(#{id})");
            let href = asset.to_string();
            let transform = format!("matrix({a} {b} {c} {d} {tx} {ty})");
            let definition = svgr!(<defs><pattern id={id} patternUnits="userSpaceOnUse" patternContentUnits="userSpaceOnUse" x={*x} y={*y} width={*width} height={*height} patternTransform={transform}><image href={href} width={*width} height={*height} preserveAspectRatio="none"/></pattern></defs>);
            (reference, 1., definition)
        }
        Some(source) => {
            let (transform, spread, stops) = match source {
                PaintSource::Linear {
                    transform,
                    spread,
                    stops,
                    ..
                }
                | PaintSource::Radial {
                    transform,
                    spread,
                    stops,
                    ..
                } => (transform, spread, stops),
                PaintSource::ImagePattern { .. } | PaintSource::Solid { .. } => unreachable!(),
            };
            let [a, b, c, d, x, y] = transform;
            let transform = format!("matrix({a} {b} {c} {d} {x} {y})");
            let spread = match spread {
                Spread::Pad => "pad",
                Spread::Reflect => "reflect",
                Spread::Repeat => "repeat",
            };
            let stops = stops
                .iter()
                .map(|stop| {
                    let offset = stop.offset;
                    let opacity = stop.color[3];
                    let color = color(stop.color);
                    svgr!(<stop offset={offset} stop-color={color} stop-opacity={opacity}/>)
                })
                .collect::<Svgr<'a>>();
            let reference = format!("url(#{id})");
            let definition = match source {
                PaintSource::Linear {
                    start: [x1, y1],
                    end: [x2, y2],
                    ..
                } => {
                    svgr!(<defs><linearGradient id={id} gradientUnits="userSpaceOnUse" x1={*x1} y1={*y1} x2={*x2} y2={*y2} gradientTransform={transform} spreadMethod={spread}>{stops}</linearGradient></defs>)
                }
                PaintSource::Radial {
                    center: [cx, cy],
                    focus: [fx, fy],
                    radius,
                    ..
                } => {
                    svgr!(<defs><radialGradient id={id} gradientUnits="userSpaceOnUse" cx={*cx} cy={*cy} fx={*fx} fy={*fy} r={*radius} gradientTransform={transform} spreadMethod={spread}>{stops}</radialGradient></defs>)
                }
                PaintSource::ImagePattern { .. } | PaintSource::Solid { .. } => unreachable!(),
            };
            (reference, 1., definition)
        }
    }
}
// Rendering needs sampled coordinates, not document identities or extension data.
#[derive(Clone, Copy)]
struct GeometryPoint {
    position: [f64; 2],
    incoming: [f64; 2],
    outgoing: [f64; 2],
}
impl GeometryPoint {
    fn sample(point: &Point, time: Time) -> Result<Self, String> {
        Ok(Self {
            position: point.position_at(time)?,
            incoming: point.incoming_at(time)?,
            outgoing: point.outgoing_at(time)?,
        })
    }
}
fn segment(path: &mut String, a: &GeometryPoint, b: &GeometryPoint) {
    if a.outgoing == [0.; 2] && b.incoming == [0.; 2] {
        write!(path, "L {} {} ", b.position[0], b.position[1]).expect("String write");
    } else {
        write!(
            path,
            "C {} {} {} {} {} {} ",
            a.position[0] + a.outgoing[0],
            a.position[1] + a.outgoing[1],
            b.position[0] + b.incoming[0],
            b.position[1] + b.incoming[1],
            b.position[0],
            b.position[1]
        )
        .expect("String write");
    }
}
