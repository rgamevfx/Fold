use super::{Layer, Roto};
use crate::animation::Property;
impl Roto {
    pub fn properties(&self) -> Vec<Property> {
        fn visit(l: &Layer, out: &mut Vec<Property>) {
            let mut property = |suffix: &str,
                                label: &str,
                                values: Vec<f64>,
                                channels: &'static [&'static str],
                                range| {
                out.push(Property {
                    id: format!("roto/{}/{suffix}", l.id),
                    label: format!("{} · {label}", l.name),
                    unit: "",
                    values,
                    channels,
                    range,
                })
            };
            property(
                "opacity",
                "Opacity",
                vec![l.opacity],
                &["Value"],
                Some([0., 1.]),
            );
            property(
                "translate",
                "Translate",
                l.translate.to_vec(),
                &["X", "Y"],
                None,
            );
            property("scale", "Scale", l.scale.to_vec(), &["X", "Y"], None);
            property("rotate", "Rotate", vec![l.rotate], &["Value"], None);
            property("center", "Center", l.center.to_vec(), &["X", "Y"], None);
            if let Some(s) = &l.shape {
                property(
                    "feather",
                    "Feather",
                    vec![s.feather],
                    &["Value"],
                    Some([-10000., 10000.]),
                );
                property(
                    "falloff",
                    "Falloff",
                    vec![s.falloff],
                    &["Value"],
                    Some([0.01, 100.]),
                );
                for (i, p) in s.points.iter().enumerate() {
                    for (part, value) in [
                        ("position", p.position),
                        ("incoming", p.incoming),
                        ("outgoing", p.outgoing),
                        ("feather", p.feather),
                    ] {
                        property(
                            &format!("point/{}/{part}", p.id),
                            &format!("Point {} {part}", i + 1),
                            value.to_vec(),
                            &["X", "Y"],
                            None,
                        );
                    }
                }
            }
            for child in &l.children {
                visit(child, out);
            }
        }
        let mut out = vec![];
        visit(&self.root, &mut out);
        out
    }
    pub fn set_property(&mut self, path: &str, value: &[f64]) -> Result<(), String> {
        let parts: Vec<_> = path.split('/').collect();
        if parts.len() < 3 || parts[0] != "roto" || value.iter().any(|v| !v.is_finite()) {
            return Err("Invalid Roto property".into());
        }
        let id = serde_json::from_value(serde_json::Value::String(parts[1].into()))
            .map_err(|_| "Invalid layer ID")?;
        let l = self.root.find_mut(id).ok_or("Missing Roto layer")?;
        match (parts[2], value) {
            ("opacity", [v]) => l.opacity = *v,
            ("translate", [x, y]) => l.translate = [*x, *y],
            ("scale", [x, y]) => l.scale = [*x, *y],
            ("rotate", [v]) => l.rotate = *v,
            ("center", [x, y]) => l.center = [*x, *y],
            ("feather", [v]) => l.shape.as_mut().ok_or("Not a shape")?.feather = *v,
            ("falloff", [v]) => l.shape.as_mut().ok_or("Not a shape")?.falloff = *v,
            ("point", [x, y]) if parts.len() == 5 => {
                let p = l
                    .shape
                    .as_mut()
                    .ok_or("Not a shape")?
                    .points
                    .iter_mut()
                    .find(|p| p.id.to_string() == parts[3])
                    .ok_or("Missing Roto point")?;
                match parts[4] {
                    "position" => p.position = [*x, *y],
                    "incoming" => p.incoming = [*x, *y],
                    "outgoing" => p.outgoing = [*x, *y],
                    "feather" => p.feather = [*x, *y],
                    _ => return Err("Unknown point control".into()),
                }
            }
            _ => return Err("Unknown Roto control".into()),
        }
        Ok(())
    }
}
