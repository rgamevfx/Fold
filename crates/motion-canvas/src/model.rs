use fold_animation::Curve;
use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_project::{Document, Metadata};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// All coordinates are canvas pixels. Paint colors are straight, encoded sRGB.
/// The renderer must convert to the compositor's declared linear working space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rate: [u32; 2],
    pub duration: Time,
    pub roots: Vec<ObjectId>,
    pub objects: BTreeMap<ObjectId, Object>,
    #[serde(flatten)]
    pub extensions: Metadata,
}

/// Static output captured from the evaluated renderer tree. These identities
/// belong to the new editable artwork, not to procedural source elements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Captured {
    pub roots: Vec<ObjectId>,
    pub objects: BTreeMap<ObjectId, Object>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Object {
    pub name: String,
    pub transform: Transform,
    pub paint: Paint,
    pub content: Content,
    /// Graph presentation only; hierarchy/order remains in roots and groups.
    pub graph_position: Option<[f32; 2]>,
    pub protected: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Image {
        asset: AssetId,
        width: Scalar,
        height: Scalar,
        fit: ImageFit,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Rectangle {
        width: Scalar,
        height: Scalar,
        radius: Scalar,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Ellipse {
        radius_x: Scalar,
        radius_y: Scalar,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Path {
        contours: Vec<Contour>,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Text {
        text: String,
        font_family: String,
        size: Scalar,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Group {
        children: Vec<ObjectId>,
        #[serde(flatten)]
        extensions: Metadata,
    },
    /// One authored object, regardless of the number of generated elements.
    Script {
        source: String,
        controls: BTreeMap<ObjectId, Control>,
        #[serde(flatten)]
        extensions: Metadata,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    Contain,
    Cover,
    Stretch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Control {
    pub name: String,
    pub value: ControlValue,
    pub protected: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ControlValue {
    Number(Scalar),
    Text(String),
    Boolean(bool),
    Color([f64; 4]),
    /// Project-managed still image, resolved through the immutable resource snapshot.
    Image(AssetId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Contour {
    pub id: ObjectId,
    pub points: Vec<Point>,
    pub closed: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Contour {
    /// Reverse traversal while preserving each anchor and its Bézier geometry.
    pub fn reverse(&mut self) {
        self.points.reverse();
        for point in &mut self.points {
            std::mem::swap(&mut point.incoming, &mut point.outgoing);
            std::mem::swap(&mut point.incoming_track, &mut point.outgoing_track);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Point {
    pub id: ObjectId,
    pub position: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_track: Option<fold_graph::binding::Track>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming_track: Option<fold_graph::binding::Track>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outgoing_track: Option<fold_graph::binding::Track>,
    /// Handles are offsets from the point; translating an anchor keeps its handles.
    pub incoming: [f64; 2],
    pub outgoing: [f64; 2],
    #[serde(flatten)]
    pub extensions: Metadata,
}

impl Point {
    pub fn position_at(&self, time: Time) -> Result<[f64; 2], String> {
        vector_at(self.position, self.position_track.as_ref(), time)
    }
    pub fn incoming_at(&self, time: Time) -> Result<[f64; 2], String> {
        vector_at(self.incoming, self.incoming_track.as_ref(), time)
    }
    pub fn outgoing_at(&self, time: Time) -> Result<[f64; 2], String> {
        vector_at(self.outgoing, self.outgoing_track.as_ref(), time)
    }
    pub fn tracks(&self) -> impl Iterator<Item = &fold_graph::binding::Track> {
        [
            &self.position_track,
            &self.incoming_track,
            &self.outgoing_track,
        ]
        .into_iter()
        .filter_map(|t| t.as_ref())
    }
}
fn vector_at(
    base: [f64; 2],
    track: Option<&fold_graph::binding::Track>,
    time: Time,
) -> Result<[f64; 2], String> {
    use fold_graph::{
        binding::Combination,
        types::{Literal, Space, Unit},
    };
    let Some(track) = track else {
        return Ok(base);
    };
    if track.combination != Combination::Replace || track.base.is_some() {
        return Err("Path coordinate tracks require replacement canvas coordinates".into());
    }
    let Literal::Vector {
        value,
        unit: Unit::Canvas,
        space: Space::Screen,
    } = track.sample(time)?
    else {
        return Err("Path coordinate tracks require a canvas-space vector".into());
    };
    if value.len() != 2 {
        return Err("Path coordinate tracks require two components".into());
    }
    Ok([value[0], value[1]])
}
impl Contour {
    pub fn at(&self, time: Time) -> Result<Self, String> {
        Ok(Self {
            id: self.id,
            closed: self.closed,
            extensions: self.extensions.clone(),
            points: self
                .points
                .iter()
                .map(|point| {
                    Ok(Point {
                        id: point.id,
                        position: point.position_at(time)?,
                        position_track: None,
                        incoming_track: None,
                        outgoing_track: None,
                        incoming: point.incoming_at(time)?,
                        outgoing: point.outgoing_at(time)?,
                        extensions: point.extensions.clone(),
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Scalar {
    pub id: ObjectId,
    pub value: f64,
    pub animation: Option<Curve>,
    pub protected: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Scalar {
    pub fn new(value: f64) -> Self {
        Self {
            id: ObjectId::new(),
            value,
            animation: None,
            protected: false,
            extensions: Metadata::new(),
        }
    }
    pub fn at(&self, time: Time) -> f64 {
        self.animation
            .as_ref()
            .and_then(|curve| curve.sample(time))
            .unwrap_or(self.value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Transform {
    pub x: Scalar,
    pub y: Scalar,
    pub rotation: Scalar,
    pub scale_x: Scalar,
    pub scale_y: Scalar,
    /// Off-diagonal coefficient before rotation; independent of Scale Y so
    /// singular and reflected affine transforms can also be captured exactly.
    pub shear: Scalar,
    pub opacity: Scalar,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: Scalar::new(0.),
            y: Scalar::new(0.),
            rotation: Scalar::new(0.),
            scale_x: Scalar::new(1.),
            scale_y: Scalar::new(1.),
            shear: Scalar::new(0.),
            opacity: Scalar::new(1.),
            extensions: Metadata::new(),
        }
    }
}
impl Transform {
    pub fn affine(&self, time: Time) -> Result<[f64; 6], String> {
        let angle = self.rotation.at(time).to_radians();
        let (sin, cos) = angle.sin_cos();
        let (sx, sy, shear) = (
            self.scale_x.at(time),
            self.scale_y.at(time),
            self.shear.at(time),
        );
        let matrix = [
            cos * sx,
            sin * sx,
            cos * shear - sin * sy,
            sin * shear + cos * sy,
            self.x.at(time),
            self.y.at(time),
        ];
        if matrix.iter().all(|value| value.is_finite()) {
            Ok(matrix)
        } else {
            Err("Motion animation produced a nonfinite transform".into())
        }
    }

    /// Capture a complete SVG affine matrix as editable native parameters.
    /// This preserves reflection and rank-deficient transforms without baking
    /// coordinates or changing stroke widths.
    pub fn from_affine([a, b, c, d, x, y]: [f64; 6]) -> Result<Self, String> {
        if ![a, b, c, d, x, y].iter().all(|value| value.is_finite()) {
            return Err("Cannot capture a nonfinite transform".into());
        }
        let sx = a.hypot(b);
        let rotation = if sx > 0. { b.atan2(a) } else { (-c).atan2(d) };
        let (sin, cos) = rotation.sin_cos();
        let result = Self {
            x: Scalar::new(x),
            y: Scalar::new(y),
            rotation: Scalar::new(rotation.to_degrees()),
            scale_x: Scalar::new(sx),
            scale_y: Scalar::new(cos * d - sin * c),
            shear: Scalar::new(cos * c + sin * d),
            ..Default::default()
        };
        result.affine(Time::ZERO)?;
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Paint {
    pub fill: Option<PaintSource>,
    pub stroke: Option<PaintSource>,
    pub stroke_width: Scalar,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f64,
    pub even_odd: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaintSource {
    ImagePattern {
        asset: AssetId,
        origin: [f64; 2],
        size: [f64; 2],
        /// Tile coordinates to object-local canvas coordinates.
        transform: [f64; 6],
        #[serde(flatten)]
        extensions: Metadata,
    },
    Solid {
        color: [f64; 4],
        #[serde(flatten)]
        extensions: Metadata,
    },
    Linear {
        start: [f64; 2],
        end: [f64; 2],
        /// Gradient coordinates to object-local canvas coordinates.
        transform: [f64; 6],
        spread: Spread,
        stops: Vec<Stop>,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Radial {
        center: [f64; 2],
        focus: [f64; 2],
        radius: f64,
        transform: [f64; 6],
        spread: Spread,
        stops: Vec<Stop>,
        #[serde(flatten)]
        extensions: Metadata,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Stop {
    pub id: ObjectId,
    pub offset: f64,
    pub color: [f64; 4],
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl From<[f64; 4]> for PaintSource {
    fn from(color: [f64; 4]) -> Self {
        Self::Solid {
            color,
            extensions: Metadata::new(),
        }
    }
}
impl PaintSource {
    pub fn visible(&self) -> bool {
        match self {
            Self::ImagePattern { .. } => true,
            Self::Solid { color, .. } => color[3] > 0.,
            Self::Linear { stops, .. } | Self::Radial { stops, .. } => {
                stops.iter().any(|stop| stop.color[3] > 0.)
            }
        }
    }
    pub fn extensions(&self) -> &Metadata {
        match self {
            Self::ImagePattern { extensions, .. }
            | Self::Solid { extensions, .. }
            | Self::Linear { extensions, .. }
            | Self::Radial { extensions, .. } => extensions,
        }
    }
    pub fn stops(&self) -> Option<&[Stop]> {
        match self {
            Self::ImagePattern { .. } | Self::Solid { .. } => None,
            Self::Linear { stops, .. } | Self::Radial { stops, .. } => Some(stops),
        }
    }
    fn validate(
        &self,
        take: &mut impl FnMut(ObjectId) -> Result<(), String>,
    ) -> Result<(), String> {
        match self {
            Self::ImagePattern {
                origin,
                size,
                transform,
                extensions: extra,
                ..
            } => {
                extensions(extra, &["kind", "asset", "origin", "size", "transform"])?;
                let determinant = transform[0] * transform[3] - transform[1] * transform[2];
                if origin
                    .iter()
                    .chain(size)
                    .chain(transform)
                    .any(|v| !v.is_finite())
                    || size.iter().any(|v| *v <= 0. || *v > 1e6)
                    || !determinant.is_finite()
                    || determinant == 0.
                {
                    return Err("Invalid Motion image pattern".into());
                }
                return Ok(());
            }
            Self::Solid {
                color,
                extensions: extra,
            } => {
                extensions(extra, &["kind", "color"])?;
                return validate_color(*color);
            }
            Self::Linear {
                start,
                end,
                extensions: extra,
                ..
            } => {
                extensions(
                    extra,
                    &["kind", "start", "end", "transform", "spread", "stops"],
                )?;
                if !start.iter().chain(end).all(|v| v.is_finite()) || start == end {
                    return Err("Invalid Motion linear gradient".into());
                }
            }
            Self::Radial {
                center,
                focus,
                radius,
                extensions: extra,
                ..
            } => {
                extensions(
                    extra,
                    &[
                        "kind",
                        "center",
                        "focus",
                        "radius",
                        "transform",
                        "spread",
                        "stops",
                    ],
                )?;
                if !center.iter().chain(focus).all(|v| v.is_finite())
                    || !radius.is_finite()
                    || *radius <= 0.
                {
                    return Err("Invalid Motion radial gradient".into());
                }
            }
        }
        let (transform, stops) = match self {
            Self::Linear {
                transform, stops, ..
            }
            | Self::Radial {
                transform, stops, ..
            } => (transform, stops),
            Self::ImagePattern { .. } | Self::Solid { .. } => unreachable!(),
        };
        let determinant = transform[0] * transform[3] - transform[1] * transform[2];
        if !transform.iter().all(|v| v.is_finite())
            || !determinant.is_finite()
            || determinant == 0.
            || !(2..=256).contains(&stops.len())
        {
            return Err("Invalid Motion gradient transform or stop count".into());
        }
        let mut previous = 0.;
        for stop in stops {
            take(stop.id)?;
            extensions(&stop.extensions, &["id", "offset", "color"])?;
            validate_color(stop.color)?;
            if !stop.offset.is_finite() || !(previous..=1.).contains(&stop.offset) {
                return Err("Gradient stops must be ordered in 0..=1".into());
            }
            previous = stop.offset;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    Miter,
    Round,
    Bevel,
}
impl Default for Paint {
    fn default() -> Self {
        Self {
            fill: Some([0.35, 0.2, 0.95, 1.].into()),
            stroke: None,
            stroke_width: Scalar::new(1.),
            cap: Cap::Round,
            join: Join::Round,
            miter_limit: 4.,
            even_odd: false,
            extensions: Metadata::new(),
        }
    }
}
impl Object {
    pub fn new(name: impl Into<String>, content: Content) -> Self {
        Self {
            name: name.into(),
            transform: Transform::default(),
            paint: Paint::default(),
            content,
            graph_position: None,
            protected: false,
            extensions: Metadata::new(),
        }
    }
    pub fn scalars(&self) -> Vec<&Scalar> {
        self.named_scalars()
            .into_iter()
            .map(|(_, value)| value)
            .collect()
    }
    /// Controls that affect rendered content. Storage validation uses all scalars.
    pub fn named_controls(&self) -> Vec<(&str, &Scalar)> {
        self.named_scalars()
            .into_iter()
            .filter(|(_, scalar)| {
                !matches!(self.content, Content::Image { .. })
                    || scalar.id != self.paint.stroke_width.id
            })
            .collect()
    }
    /// Labels are presentation only. Scalar IDs remain the edit and animation targets.
    pub fn named_scalars(&self) -> Vec<(&str, &Scalar)> {
        let t = &self.transform;
        let mut result = vec![
            ("Position X", &t.x),
            ("Position Y", &t.y),
            ("Rotation", &t.rotation),
            ("Scale X", &t.scale_x),
            ("Scale Y", &t.scale_y),
            ("Shear", &t.shear),
            ("Opacity", &t.opacity),
            ("Stroke Width", &self.paint.stroke_width),
        ];
        match &self.content {
            Content::Image { width, height, .. } => {
                result.extend([("Width", width), ("Height", height)])
            }
            Content::Rectangle {
                width,
                height,
                radius,
                ..
            } => result.extend([("Width", width), ("Height", height), ("Radius", radius)]),
            Content::Ellipse {
                radius_x, radius_y, ..
            } => result.extend([("Radius X", radius_x), ("Radius Y", radius_y)]),
            Content::Text { size, .. } => result.push(("Text Size", size)),
            Content::Script { controls, .. } => result.extend(controls.values().filter_map(
                |control| match &control.value {
                    ControlValue::Number(value) => Some((control.name.as_str(), value)),
                    _ => None,
                },
            )),
            _ => {}
        }
        result
    }
    pub fn scalar_mut(&mut self, id: ObjectId) -> Option<&mut Scalar> {
        let t = &mut self.transform;
        let mut values = vec![
            &mut t.x,
            &mut t.y,
            &mut t.rotation,
            &mut t.scale_x,
            &mut t.scale_y,
            &mut t.shear,
            &mut t.opacity,
            &mut self.paint.stroke_width,
        ];
        match &mut self.content {
            Content::Image { width, height, .. } => values.extend([width, height]),
            Content::Rectangle {
                width,
                height,
                radius,
                ..
            } => values.extend([width, height, radius]),
            Content::Ellipse {
                radius_x, radius_y, ..
            } => values.extend([radius_x, radius_y]),
            Content::Text { size, .. } => values.push(size),
            Content::Script { controls, .. } => values.extend(controls.values_mut().filter_map(
                |control| match &mut control.value {
                    ControlValue::Number(value) => Some(value),
                    _ => None,
                },
            )),
            _ => {}
        }
        values.into_iter().find(|value| value.id == id)
    }
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            rate: [30, 1],
            duration: Time::new(5, 1).expect("valid duration"),
            roots: vec![],
            objects: BTreeMap::new(),
            extensions: Metadata::new(),
        }
    }
}
impl Canvas {
    pub fn validate(&self) -> Result<(), String> {
        extensions(
            &self.extensions,
            &["width", "height", "rate", "duration", "roots", "objects"],
        )?;
        if self.width == 0
            || self.height == 0
            || self.width > 16_384
            || self.height > 16_384
            || self.rate.contains(&0)
            || self.duration <= Time::ZERO
        {
            return Err("Invalid Motion canvas dimensions, rate or duration".into());
        }
        let mut ids = BTreeSet::new();
        let mut take = |id| {
            if ids.insert(id) {
                Ok(())
            } else {
                Err(format!("Duplicate Motion identity {id:?}"))
            }
        };
        for (&id, object) in &self.objects {
            take(id)?;
            extensions(
                &object.extensions,
                &[
                    "name",
                    "transform",
                    "paint",
                    "content",
                    "graph_position",
                    "protected",
                ],
            )?;
            extensions(
                &object.transform.extensions,
                &[
                    "x", "y", "rotation", "scale_x", "scale_y", "shear", "opacity",
                ],
            )?;
            extensions(
                &object.paint.extensions,
                &[
                    "fill",
                    "stroke",
                    "stroke_width",
                    "cap",
                    "join",
                    "miter_limit",
                    "even_odd",
                ],
            )?;
            extensions(object.content.extensions(), object.content.fields())?;
            if object.name.trim().is_empty()
                || object.name.chars().any(char::is_control)
                || object
                    .graph_position
                    .is_some_and(|p| !p.iter().all(|v| v.is_finite()))
            {
                return Err("Invalid Motion object name or graph position".into());
            }
            for scalar in object.scalars() {
                take(scalar.id)?;
                extensions(
                    &scalar.extensions,
                    &["id", "value", "animation", "protected"],
                )?;
                if !scalar.value.is_finite() {
                    return Err("Nonfinite Motion parameter".into());
                }
                if let Some(curve) = &scalar.animation {
                    curve.validate()?;
                    if curve.keys.iter().any(|key| key.rotation.is_some()) {
                        return Err(
                            "Motion scalar animation cannot contain quaternion context".into()
                        );
                    }
                    for key in &curve.keys {
                        take(key.id)?;
                    }
                }
            }
            if !object.paint.miter_limit.is_finite() || object.paint.miter_limit < 1. {
                return Err("Motion miter limit must be finite and at least one".into());
            }
            for source in [&object.paint.fill, &object.paint.stroke]
                .into_iter()
                .flatten()
            {
                source.validate(&mut take)?;
            }
            match &object.content {
                Content::Path { contours, .. } => {
                    for contour in contours {
                        take(contour.id)?;
                        extensions(&contour.extensions, &["id", "points", "closed"])?;
                        for point in &contour.points {
                            take(point.id)?;
                            extensions(
                                &point.extensions,
                                &[
                                    "id",
                                    "position",
                                    "position_track",
                                    "incoming",
                                    "outgoing",
                                    "incoming_track",
                                    "outgoing_track",
                                ],
                            )?;
                            for track in point.tracks() {
                                track.validate()?;
                                take(track.id)?;
                                for key in &track.keys {
                                    take(key.id)?;
                                }
                            }
                            point.position_at(Time::ZERO)?;
                            point.incoming_at(Time::ZERO)?;
                            point.outgoing_at(Time::ZERO)?;
                            if !point
                                .position
                                .iter()
                                .chain(&point.incoming)
                                .chain(&point.outgoing)
                                .all(|v| v.is_finite())
                            {
                                return Err("Nonfinite path point or handle".into());
                            }
                        }
                    }
                }
                Content::Script { controls, .. } => {
                    let mut names = BTreeSet::new();
                    for (&id, control) in controls {
                        take(id)?;
                        extensions(&control.extensions, &["name", "value", "protected"])?;
                        if control.name.is_empty() || !names.insert(&control.name) {
                            return Err("Script controls need unique names".into());
                        }
                        if let ControlValue::Color(color) = control.value {
                            validate_color(color)?;
                        }
                    }
                }
                _ => {}
            }
        }
        fn visit(
            canvas: &Canvas,
            id: ObjectId,
            reached: &mut BTreeSet<ObjectId>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 128 || !reached.insert(id) {
                return Err("Motion hierarchy repeats an object or contains a cycle".into());
            }
            let object = canvas
                .objects
                .get(&id)
                .ok_or("Missing Motion object in hierarchy")?;
            if let Content::Group { children, .. } = &object.content {
                for child in children {
                    visit(canvas, *child, reached, depth + 1)?;
                }
            }
            Ok(())
        }
        let mut reached = BTreeSet::new();
        for root in &self.roots {
            visit(self, *root, &mut reached, 0)?;
        }
        if reached.len() != self.objects.len() {
            return Err("Every Motion object must have one place in the hierarchy".into());
        }
        Ok(())
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: crate::TYPE.into(),
            schema_version: crate::SCHEMA,
            revision: Default::default(),
            dependencies: vec![],
            assets: self.assets(),
            payload: serde_json::to_vec(self).map_err(|e| e.to_string())?,
            extensions: Metadata::new(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != crate::TYPE
            || document.schema_version != crate::SCHEMA
        {
            return Err("Unsupported Motion document; preserve its opaque payload".into());
        }
        let canvas: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        canvas.validate()?;
        if !document.dependencies.is_empty() || document.assets != canvas.assets() {
            return Err("Motion resource declarations do not match image references".into());
        }
        Ok(canvas)
    }
    pub fn replace_document(&self, original: &Document) -> Result<Document, String> {
        Self::from_document(original)?;
        self.validate()?;
        let mut result = original.clone();
        result.payload = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        result.assets = self.assets();
        Ok(result)
    }
    /// One sorted declaration per project asset, regardless of instance count.
    pub fn assets(&self) -> Vec<AssetId> {
        let mut assets = BTreeSet::new();
        for object in self.objects.values() {
            for source in object.paint.fill.iter().chain(&object.paint.stroke) {
                if let PaintSource::ImagePattern { asset, .. } = source {
                    assets.insert(*asset);
                }
            }
            match &object.content {
                Content::Image { asset, .. } => {
                    assets.insert(*asset);
                }
                Content::Script { controls, .. } => {
                    for control in controls.values() {
                        if let ControlValue::Image(asset) = control.value {
                            assets.insert(asset);
                        }
                    }
                }
                _ => {}
            }
        }
        assets.into_iter().collect()
    }
}
impl Content {
    pub fn extensions(&self) -> &Metadata {
        match self {
            Self::Image { extensions, .. }
            | Self::Rectangle { extensions, .. }
            | Self::Ellipse { extensions, .. }
            | Self::Path { extensions, .. }
            | Self::Text { extensions, .. }
            | Self::Group { extensions, .. }
            | Self::Script { extensions, .. } => extensions,
        }
    }
    fn fields(&self) -> &'static [&'static str] {
        match self {
            Self::Image { .. } => &["kind", "asset", "width", "height", "fit"],
            Self::Rectangle { .. } => &["kind", "width", "height", "radius"],
            Self::Ellipse { .. } => &["kind", "radius_x", "radius_y"],
            Self::Path { .. } => &["kind", "contours"],
            Self::Text { .. } => &["kind", "text", "font_family", "size"],
            Self::Group { .. } => &["kind", "children"],
            Self::Script { .. } => &["kind", "source", "controls"],
        }
    }
}
fn extensions(metadata: &Metadata, fields: &[&str]) -> Result<(), String> {
    if fields.iter().any(|field| metadata.contains_key(*field)) {
        Err("Motion extension collides with a native field".into())
    } else {
        Ok(())
    }
}
pub(crate) fn validate_color(color: [f64; 4]) -> Result<(), String> {
    if color
        .iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    {
        Ok(())
    } else {
        Err("Motion paint needs encoded sRGB components in 0..=1".into())
    }
}
