//! Style changes preserve paths and instance references. Repeated shared sources
//! are rewritten once per operation, not independently for every copy.
use super::{Content, Geometry};
use fold_render::vector::Stroke;
use std::{collections::BTreeMap, sync::Arc};
pub fn paint(
    source: &Content,
    fill: Option<([f64; 4], bool)>,
    stroke: Option<Stroke>,
) -> Result<Content, String> {
    fn walk(
        source: &Content,
        fill: Option<([f64; 4], bool)>,
        stroke: &Option<Stroke>,
        cache: &mut BTreeMap<usize, Content>,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<Content, String> {
        if depth > 64 {
            return Err("style nesting exceeds 64".into());
        }
        let key = Arc::as_ptr(source) as usize;
        if let Some(result) = cache.get(&key) {
            return Ok(result.clone());
        }
        let mut result = Vec::with_capacity(source.len());
        for item in source.iter() {
            *remaining = remaining
                .checked_sub(1)
                .ok_or("style element budget exceeded")?;
            let mut item = item.clone();
            match &item.geometry {
                Geometry::Group(children) => {
                    item.geometry =
                        Geometry::Group(walk(children, fill, stroke, cache, remaining, depth + 1)?)
                }
                Geometry::Instance(children) => {
                    item.geometry = Geometry::Instance(walk(
                        children,
                        fill,
                        stroke,
                        cache,
                        remaining,
                        depth + 1,
                    )?)
                }
                Geometry::Path(_) | Geometry::Glyph { .. } => {
                    if let Some((color, even_odd)) = fill {
                        item.fill = Some(color);
                        item.even_odd = even_odd;
                    }
                    if let Some(stroke) = stroke {
                        item.stroke = Some(stroke.clone());
                    }
                }
                Geometry::Point => return Err("style requires paths/text, not points".into()),
            }
            if matches!(item.geometry, Geometry::Group(_) | Geometry::Instance(_)) {
                if fill.is_some() {
                    item.fill = None;
                }
                if stroke.is_some() {
                    item.stroke = None;
                }
            }
            result.push(item);
        }
        let result = Arc::new(result);
        cache.insert(key, result.clone());
        Ok(result)
    }
    walk(source, fill, &stroke, &mut BTreeMap::new(), &mut 16384, 0)
}
