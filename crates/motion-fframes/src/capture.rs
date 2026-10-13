//! Static editable capture of an evaluated upstream tree. This builds fresh
//! native identities; it does not reconstruct procedural or animation logic.
use fframes::usvgr as svg;
use fold_foundation::{AssetId, ObjectId};
use fold_motion_canvas::model::{
    Cap, Content, Contour, ImageFit, Join, Object, PaintSource, Point, Scalar, Spread, Stop,
    Transform,
};
use std::collections::BTreeMap;
use tiny_skia_path::PathSegment;

pub use fold_motion_canvas::model::Captured;

/// All-or-nothing conversion. Unsupported features return an error rather than
/// silently flattening effects, dropping paint, or modifying a document.
pub fn tree(tree: &svg::Tree, images: &crate::Images) -> Result<Captured, String> {
    let mut capture = Builder {
        objects: BTreeMap::new(),
        points: 0,
        images,
    };
    let root = capture.group(tree.root(), 0)?;
    Ok(Captured {
        roots: vec![root],
        objects: capture.objects,
    })
}

struct Builder<'a> {
    objects: BTreeMap<ObjectId, Object>,
    points: usize,
    images: &'a crate::Images,
}
impl Builder<'_> {
    fn insert(&mut self, object: Object) -> Result<ObjectId, String> {
        if self.objects.len() >= 100_000 {
            return Err("Editable capture exceeds 100,000 objects".into());
        }
        let id = ObjectId::new();
        self.objects.insert(id, object);
        Ok(id)
    }

    fn group(&mut self, group: &svg::Group, depth: usize) -> Result<ObjectId, String> {
        if depth >= 128 {
            return Err("Editable capture exceeds 128 group levels".into());
        }
        if group.blend_mode() != svg::BlendMode::Normal
            || group.isolate()
            || group.clip_path().is_some()
            || group.mask().is_some()
            || !group.filters().is_empty()
        {
            return Err(
                "Editable capture does not yet support blends, isolation, clips, masks or filters"
                    .into(),
            );
        }
        let mut children = Vec::new();
        for node in group.children() {
            children.push(match node {
                svg::Node::Group(value) => self.group(value, depth + 1)?,
                svg::Node::Path(value) => self.path(value)?,
                svg::Node::FastShape(value) => self.shape(value)?,
                svg::Node::Text(value) => self.group(value.flattened(), depth + 1)?,
                svg::Node::Image(value) => self.image(value)?,
            });
        }
        let mut object = Object::new(
            "Captured group",
            Content::Group {
                children,
                extensions: Default::default(),
            },
        );
        let t = group.transform();
        object.transform = Transform::from_affine([
            t.sx as f64,
            t.ky as f64,
            t.kx as f64,
            t.sy as f64,
            t.tx as f64,
            t.ty as f64,
        ])?;
        object.transform.opacity.value = group.opacity().get() as f64;
        object.paint.fill = None;
        self.insert(object)
    }

    fn image(&mut self, image: &svg::Image) -> Result<ObjectId, String> {
        let asset = image_asset(image, self.images)?;
        let view = image.view_box();
        let fit = match view.aspect.align {
            svg::Align::None => ImageFit::Stretch,
            svg::Align::XMidYMid if view.aspect.slice => ImageFit::Cover,
            svg::Align::XMidYMid => ImageFit::Contain,
            _ => {
                return Err("Editable capture requires centered image alignment or stretch".into());
            }
        };
        let mut object = Object::new(
            "Captured image",
            Content::Image {
                asset,
                width: Scalar::new(view.rect.width() as f64),
                height: Scalar::new(view.rect.height() as f64),
                fit,
                extensions: Default::default(),
            },
        );
        object.transform.x.value = view.rect.x() as f64;
        object.transform.y.value = view.rect.y() as f64;
        if image.visibility() != svg::Visibility::Visible {
            object.transform.opacity.value = 0.;
        }
        object.paint.fill = None;
        self.insert(object)
    }

    fn shape(&mut self, shape: &svg::FastShape) -> Result<ObjectId, String> {
        let rect = shape.kind().rect();
        let (content, position) = match shape.kind() {
            svg::FastShapeKind::Ellipse(_) => (
                Content::Ellipse {
                    radius_x: Scalar::new(rect.width() as f64 / 2.),
                    radius_y: Scalar::new(rect.height() as f64 / 2.),
                    extensions: Default::default(),
                },
                [
                    rect.x() as f64 + rect.width() as f64 / 2.,
                    rect.y() as f64 + rect.height() as f64 / 2.,
                ],
            ),
            svg::FastShapeKind::RoundRect { rx, ry, .. } if rx == ry => (
                Content::Rectangle {
                    width: Scalar::new(rect.width() as f64),
                    height: Scalar::new(rect.height() as f64),
                    radius: Scalar::new(rx as f64),
                    extensions: Default::default(),
                },
                [rect.x() as f64, rect.y() as f64],
            ),
            _ => return self.path(&shape.to_path().ok_or("Cannot capture the shape outline")?),
        };
        let mut object = styled(shape.path(), content, self.images)?;
        object.transform.x.value = position[0];
        object.transform.y.value = position[1];
        // Native primitives are centered/anchored at their local origin. SVG
        // gradients use the original shape coordinates, so move their space
        // with the geometry when extracting that origin into the transform.
        for source in [&mut object.paint.fill, &mut object.paint.stroke]
            .into_iter()
            .flatten()
        {
            match source {
                PaintSource::ImagePattern { transform, .. }
                | PaintSource::Linear { transform, .. }
                | PaintSource::Radial { transform, .. } => {
                    transform[4] -= position[0];
                    transform[5] -= position[1];
                }
                PaintSource::Solid { .. } => {}
            }
        }
        self.insert(object)
    }

    fn path(&mut self, path: &svg::Path) -> Result<ObjectId, String> {
        let mut contours: Vec<Contour> = Vec::new();
        for segment in path.data().segments() {
            if let PathSegment::MoveTo(p) = segment {
                contours.push(Contour {
                    id: ObjectId::new(),
                    points: vec![point([p.x as f64, p.y as f64])],
                    closed: false,
                    extensions: Default::default(),
                });
            } else {
                let contour = contours.last_mut().ok_or("Path has no starting point")?;
                if contour.closed {
                    return Err("Path segment follows a closed contour without a new start".into());
                }
                let previous = contour.points.last_mut().ok_or("Empty path contour")?;
                let start = previous.position;
                match segment {
                    PathSegment::LineTo(p) => contour.points.push(point([p.x as f64, p.y as f64])),
                    PathSegment::QuadTo(control, end) => {
                        let q = [control.x as f64, control.y as f64];
                        let end = [end.x as f64, end.y as f64];
                        previous.outgoing =
                            std::array::from_fn(|axis| (q[axis] - start[axis]) * (2. / 3.));
                        let mut next = point(end);
                        next.incoming =
                            std::array::from_fn(|axis| (q[axis] - end[axis]) * (2. / 3.));
                        contour.points.push(next);
                    }
                    PathSegment::CubicTo(a, b, end) => {
                        previous.outgoing = [a.x as f64 - start[0], a.y as f64 - start[1]];
                        let mut next = point([end.x as f64, end.y as f64]);
                        next.incoming =
                            [b.x as f64 - next.position[0], b.y as f64 - next.position[1]];
                        contour.points.push(next);
                    }
                    PathSegment::Close => {
                        contour.closed = true;
                        if contour.points.len() > 1
                            && contour.points.last().unwrap().position == contour.points[0].position
                        {
                            let end = contour.points.pop().unwrap();
                            contour.points[0].incoming = end.incoming;
                        }
                    }
                    PathSegment::MoveTo(_) => unreachable!(),
                }
            }
            self.points += 1;
            if self.points > 1_000_000 {
                return Err("Editable capture exceeds 1,000,000 path segments".into());
            }
        }
        self.insert(styled(
            path,
            Content::Path {
                contours,
                extensions: Default::default(),
            },
            self.images,
        )?)
    }
}

fn styled(path: &svg::Path, content: Content, images: &crate::Images) -> Result<Object, String> {
    if path.paint_order() != svg::PaintOrder::FillAndStroke
        || path.rendering_mode() != svg::ShapeRendering::GeometricPrecision
    {
        return Err(
            "Editable capture does not yet support this paint order or shape rendering mode".into(),
        );
    }
    let name = match &content {
        Content::Ellipse { .. } => "Captured ellipse",
        Content::Rectangle { .. } => "Captured rectangle",
        _ => "Captured path",
    };
    let mut object = Object::new(name, content);
    object.paint.fill = path
        .fill()
        .map(|fill| color(fill.paint(), fill.opacity().get(), images))
        .transpose()?;
    object.paint.even_odd = path
        .fill()
        .is_some_and(|fill| fill.rule() == svg::FillRule::EvenOdd);
    object.paint.stroke = None;
    if let Some(stroke) = path.stroke() {
        if stroke.dasharray().is_some() {
            return Err("Editable capture does not yet support dashed strokes".into());
        }
        object.paint.miter_limit = stroke.miterlimit().get() as f64;
        object.paint.stroke = Some(color(stroke.paint(), stroke.opacity().get(), images)?);
        object.paint.stroke_width.value = stroke.width().get() as f64;
        object.paint.cap = match stroke.linecap() {
            svg::LineCap::Butt => Cap::Butt,
            svg::LineCap::Round => Cap::Round,
            svg::LineCap::Square => Cap::Square,
        };
        object.paint.join = match stroke.linejoin() {
            svg::LineJoin::Miter => Join::Miter,
            svg::LineJoin::Round => Join::Round,
            svg::LineJoin::Bevel => Join::Bevel,
            svg::LineJoin::MiterClip => {
                return Err("Editable capture does not yet support clipped miter joins".into());
            }
        };
    }
    if path.visibility() != svg::Visibility::Visible {
        object.transform.opacity.value = 0.;
    }
    Ok(object)
}

fn point(position: [f64; 2]) -> Point {
    Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position,
        incoming: [0.; 2],
        outgoing: [0.; 2],
        extensions: Default::default(),
    }
}
fn color(paint: &svg::Paint, opacity: f32, images: &crate::Images) -> Result<PaintSource, String> {
    match paint {
        svg::Paint::Color(value) => Ok([
            value.red as f64 / 255.,
            value.green as f64 / 255.,
            value.blue as f64 / 255.,
            opacity as f64,
        ]
        .into()),
        svg::Paint::LinearGradient(gradient) => {
            let (transform, spread, stops) = gradient_data(gradient, opacity);
            Ok(PaintSource::Linear {
                start: [gradient.x1() as f64, gradient.y1() as f64],
                end: [gradient.x2() as f64, gradient.y2() as f64],
                transform,
                spread,
                stops,
                extensions: Default::default(),
            })
        }
        svg::Paint::RadialGradient(gradient) => {
            let (transform, spread, stops) = gradient_data(gradient, opacity);
            Ok(PaintSource::Radial {
                center: [gradient.cx() as f64, gradient.cy() as f64],
                focus: [gradient.fx() as f64, gradient.fy() as f64],
                radius: gradient.r().get() as f64,
                transform,
                spread,
                stops,
                extensions: Default::default(),
            })
        }
        svg::Paint::Pattern(pattern) => {
            let root = pattern.root();
            let [svg::Node::Image(image)] = root.children() else {
                return Err("Editable image patterns require one declared image".into());
            };
            let rect = pattern.rect();
            let view = image.view_box();
            if opacity != 1.
                || root.opacity().get() != 1.
                || root.transform() != svg::Transform::identity()
                || root.blend_mode() != svg::BlendMode::Normal
                || root.isolate()
                || root.clip_path().is_some()
                || root.mask().is_some()
                || !root.filters().is_empty()
                || pattern.view_box().is_some()
                || image.visibility() != svg::Visibility::Visible
                || view.aspect.align != svg::Align::None
                || view.rect.x() != 0.
                || view.rect.y() != 0.
                || view.rect.width() != rect.width()
                || view.rect.height() != rect.height()
            {
                return Err(
                    "Editable capture requires an unfiltered full-tile image pattern".into(),
                );
            }
            let t = pattern.transform();
            Ok(PaintSource::ImagePattern {
                asset: image_asset(image, images)?,
                origin: [rect.x() as f64, rect.y() as f64],
                size: [rect.width() as f64, rect.height() as f64],
                transform: [
                    t.sx as f64,
                    t.ky as f64,
                    t.kx as f64,
                    t.sy as f64,
                    t.tx as f64,
                    t.ty as f64,
                ],
                extensions: Default::default(),
            })
        }
    }
}

fn gradient_data(gradient: &svg::BaseGradient, opacity: f32) -> ([f64; 6], Spread, Vec<Stop>) {
    let t = gradient.transform();
    let spread = match gradient.spread_method() {
        svg::SpreadMethod::Pad => Spread::Pad,
        svg::SpreadMethod::Reflect => Spread::Reflect,
        svg::SpreadMethod::Repeat => Spread::Repeat,
    };
    let stops = gradient
        .stops()
        .iter()
        .map(|stop| {
            let c = stop.color();
            Stop {
                id: ObjectId::new(),
                offset: stop.offset().get() as f64,
                color: [
                    c.red as f64 / 255.,
                    c.green as f64 / 255.,
                    c.blue as f64 / 255.,
                    stop.opacity().get() as f64 * opacity as f64,
                ],
                extensions: Default::default(),
            }
        })
        .collect();
    (
        [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from),
        spread,
        stops,
    )
}

fn image_asset(image: &svg::Image, images: &crate::Images) -> Result<AssetId, String> {
    if image.rendering_mode() != svg::ImageRendering::OptimizeQuality {
        return Err("Editable capture does not support this image interpolation mode".into());
    }
    let svg::ImageKind::DATA(data) = image.kind() else {
        return Err("Editable capture requires a declared raster image".into());
    };
    let name = images
        .iter()
        .find_map(|(name, retained)| std::sync::Arc::ptr_eq(data, retained).then_some(name))
        .ok_or("Captured image is not a retained project resource")?;
    serde_json::from_value(serde_json::Value::String(name.clone()))
        .map_err(|_| "Captured image is not named by a project asset ID".into())
}
