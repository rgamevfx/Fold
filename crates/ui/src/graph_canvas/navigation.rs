//! Screen-space hit geometry for event-time background-pan routing.
//! Hit-test the incoming pointer, not last frame's hovered ID: a move and press
//! may arrive together, and must never turn a node/pin/wire drag into panning.
type Point = [f32; 2];
type Rect = [Point; 2];
pub(super) type Curve = [Point; 4];
#[derive(Default)]
pub(super) struct HitMap {
    pub bounds: Option<Rect>,
    pub nodes: Vec<Rect>,
    pub links: Vec<Curve>,
}
impl HitMap {
    pub fn clear(&mut self) {
        self.bounds = None;
        self.nodes.clear();
        self.links.clear();
    }
    pub fn background(&self, p: Point) -> bool {
        self.bounds.is_some_and(|r| inside(r, p, 0.0))
            && !self.nodes.iter().any(|&r| inside(r, p, 6.0))
            && !self.links.iter().any(|&c| near_curve(c, p))
    }
}
fn inside(r: Rect, p: Point, margin: f32) -> bool {
    (0..2).all(|i| p[i] >= r[0][i] - margin && p[i] < r[1][i] + margin)
}
/// Match imgui-node-editor 0.18's eased horizontal pin tangents. Pins use
/// point pivots; project this curve to screen space with the native camera.
pub(super) fn link_curve(a: Point, b: Point, strength: f32) -> Curve {
    let half = (b[0] - a[0]).hypot(b[1] - a[1]) * 0.5;
    let strength = if half < strength {
        strength * (std::f32::consts::FRAC_PI_2 * half / strength).sin()
    } else {
        strength
    };
    [a, [a[0] + strength, a[1]], [b[0] - strength, b[1]], b]
}
fn near_curve(c: Curve, p: Point) -> bool {
    let min = [0, 1].map(|i| c.iter().map(|v| v[i]).fold(f32::INFINITY, f32::min));
    let max = [0, 1].map(|i| c.iter().map(|v| v[i]).fold(f32::NEG_INFINITY, f32::max));
    if !inside([min, max], p, 8.0) {
        return false;
    }
    let mut a = c[0];
    for step in 1..=64 {
        let t = step as f32 / 64.0;
        let u = 1.0 - t;
        let b = [0, 1].map(|i| {
            u * u * u * c[0][i]
                + 3.0 * u * u * t * c[1][i]
                + 3.0 * u * t * t * c[2][i]
                + t * t * t * c[3][i]
        });
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0] * d[0] + d[1] * d[1];
        let k = if len > 0.0 {
            (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if (p[0] - a[0] - k * d[0]).hypot(p[1] - a[1] - k * d[1]) <= 8.0 {
            return true;
        }
        a = b;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_pointer_excludes_objects_and_curved_wires() {
        let map = HitMap {
            bounds: Some([[0.0; 2], [600.0; 2]]),
            nodes: vec![[[10.0, 10.0], [100.0, 100.0]]],
            links: vec![link_curve([100.0, 50.0], [400.0, 250.0], 100.0)],
        };
        assert!(!map.background([50.0, 50.0]));
        assert!(!map.background([103.0, 50.0])); // pin edge
        assert!(!map.background([250.0, 150.0])); // curve midpoint
        assert!(map.background([250.0, 50.0])); // empty space in curve bounds
        assert!(map.background([450.0, 450.0]));
        assert!(!map.background([650.0, 450.0]));
    }
}
