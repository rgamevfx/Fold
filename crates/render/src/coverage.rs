//! Feature-neutral scalar coverage program. Native curves use the shared vector
//! renderer; compositing preserves ordered operations and isolated groups.
use crate::vector::Drawing;
use fold_media::Cancel;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Combine {
    Over,
    Add,
    Subtract,
    Intersect,
    Replace,
}
impl Combine {
    pub fn apply(self, a: f32, b: f32) -> f32 {
        (match self {
            Self::Over => a + b * (1. - a),
            Self::Add => a + b,
            Self::Subtract => b * (1. - a),
            Self::Intersect => a * b,
            Self::Replace => a,
        })
        .clamp(0., 1.)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Constant(f32),
    Draw(Arc<Vec<Drawing>>),
    Combine {
        first: usize,
        second: usize,
        mode: Combine,
    },
    Adjust {
        input: usize,
        invert: bool,
        opacity: f32,
    },
    Mix {
        first: usize,
        second: usize,
        amount: f32,
    },
}
impl Op {
    pub fn inputs(&self) -> [Option<usize>; 2] {
        match *self {
            Self::Constant(_) | Self::Draw(_) => [None; 2],
            Self::Adjust { input, .. } => [Some(input), None],
            Self::Combine { first, second, .. } | Self::Mix { first, second, .. } => {
                [Some(first), Some(second)]
            }
        }
    }
    pub fn apply(&self, a: f32, b: f32) -> f32 {
        match *self {
            Self::Constant(v) => v,
            Self::Adjust {
                invert, opacity, ..
            } => (if invert { 1. - a } else { a }) * opacity,
            Self::Combine { mode, .. } => mode.apply(a, b),
            Self::Mix { amount, .. } => a * (1. - amount) + b * amount,
            Self::Draw(_) => unreachable!(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub nodes: Vec<Op>,
    pub output: usize,
}
impl Program {
    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.len() > 4096 || self.output >= self.nodes.len() {
            return Err("Invalid coverage program size".into());
        }
        for (id, op) in self.nodes.iter().enumerate() {
            if op.inputs().into_iter().flatten().any(|i| i >= id) {
                return Err("Invalid coverage dependency".into());
            }
            match op {
                Op::Draw(drawings) => {
                    crate::vector::validate_working(drawings, true)?;
                    if drawings
                        .iter()
                        .any(|d| d.stroke.is_some() || d.fill.is_none_or(|c| c[..3] != [1.; 3]))
                    {
                        return Err("Coverage requires white filled paths".into());
                    }
                }
                Op::Constant(v) if !v.is_finite() || !(0. ..=1.).contains(v) => {
                    return Err("Invalid coverage constant".into());
                }
                Op::Adjust { opacity, .. }
                    if !opacity.is_finite() || !(0. ..=1.).contains(opacity) =>
                {
                    return Err("Invalid coverage opacity".into());
                }
                Op::Mix { amount, .. } if !amount.is_finite() || !(0. ..=1.).contains(amount) => {
                    return Err("Invalid coverage mix".into());
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn needed(&self) -> Vec<bool> {
        let mut needed = vec![false; self.nodes.len()];
        needed[self.output] = true;
        for i in (0..self.nodes.len()).rev() {
            if needed[i] {
                for input in self.nodes[i].inputs().into_iter().flatten() {
                    needed[input] = true;
                }
            }
        }
        needed
    }
    pub fn uses(&self) -> Vec<usize> {
        let mut uses = vec![0; self.nodes.len()];
        let needed = self.needed();
        for (_, op) in self.nodes.iter().enumerate().filter(|(i, _)| needed[*i]) {
            for i in op.inputs().into_iter().flatten() {
                uses[i] += 1;
            }
        }
        uses[self.output] += 1;
        uses
    }
}
pub(crate) fn rasterize(
    program: &Program,
    width: u32,
    height: u32,
    cancel: &Cancel,
) -> Result<Vec<[f32; 4]>, String> {
    let count = width as usize * height as usize;
    let mut values: Vec<Option<Vec<f32>>> = vec![None; program.nodes.len()];
    let mut leases = (0..program.nodes.len()).map(|_| None).collect::<Vec<_>>();
    let mut uses = program.uses();
    let needed = program.needed();
    for (id, op) in program.nodes.iter().enumerate() {
        if !needed[id] {
            continue;
        }
        cancel.check()?;
        leases[id] =
            Some(fold_media::budget::reserve_working(count as u64 * 4).map_err(|e| e.to_string())?);
        let result = if let Op::Draw(drawings) = op {
            crate::vector::rasterize(drawings, width, height, cancel, true)?
                .into_iter()
                .map(|p| p[3])
                .collect()
        } else {
            let inputs = op.inputs();
            let a = inputs[0].map(|i| values[i].as_ref().unwrap());
            let b = inputs[1].map(|i| values[i].as_ref().unwrap());
            let mut result = Vec::with_capacity(count);
            for i in 0..count {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                result.push(op.apply(a.map_or(0., |a| a[i]), b.map_or(0., |b| b[i])));
            }
            result
        };
        values[id] = Some(result);
        for input in op.inputs().into_iter().flatten() {
            uses[input] -= 1;
            if uses[input] == 0 {
                values[input] = None;
                leases[input] = None;
            }
        }
    }
    Ok(values[program.output]
        .take()
        .unwrap()
        .into_iter()
        .map(|v| [v, 0., 0., 1.])
        .collect())
}
