//! Deterministic centered grids. Cell correspondence is independent of flattened order.
use super::{Scope, State, Value};
use crate::types::{Literal, Space, Unit};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub id: u64,
    pub cell: [u32; 2],
    pub position: [f64; 3],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Points {
    pub id: u64,
    pub space: Space,
    pub unit: Unit,
    /// Row-major iteration is presentation order, never an identity seed.
    pub points: Vec<Point>,
}
impl State<'_> {
    pub(super) fn grid(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let primitive = |name| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let count = |name| match primitive(name)? {
            Literal::Integer { value } if (0..=100_000).contains(value) => Ok(*value as usize),
            _ => Err(format!("Grid {name} must be an integer in [0,100000]")),
        };
        let columns = count("columns")?;
        let rows = count("rows")?;
        let total = columns.checked_mul(rows).ok_or("Grid count overflow")?;
        if total > 100_000 {
            return Err("Grid exceeds 100000 points".into());
        }
        let vector = |name, dimensions| match primitive(name)? {
            Literal::Vector {
                value,
                unit: Unit::Meters,
                space: Space::World,
            } if value.len() == dimensions
                && value.iter().all(|v| v.is_finite() && v.abs() <= 1e6) =>
            {
                Ok(value)
            }
            _ => Err(format!("Grid {name} requires a finite World metric vector")),
        };
        let spacing = vector("spacing", 2)?;
        if spacing.iter().any(|v| *v <= 0.) {
            return Err("Grid spacing must be positive".into());
        }
        let center = vector("center", 3)?;
        self.reserve(1 + total)?;
        let stream = self.identity(scope, id, 0, b"grid")?;
        let mut points = Vec::with_capacity(total);
        for row in 0..rows {
            // Empty columns must not turn into a long unchecked outer traversal.
            self.budget.step()?;
            for column in 0..columns {
                self.budget.step()?;
                let position = [
                    center[0] + (column as f64 - (columns as f64 - 1.) * 0.5) * spacing[0],
                    center[1] + (row as f64 - (rows as f64 - 1.) * 0.5) * spacing[1],
                    center[2],
                ];
                if position.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
                    return Err("Grid positions exceed supported World extent".into());
                }
                let cell = [column as u32, row as u32];
                let mut slot = [0u8; 8];
                slot[..4].copy_from_slice(&cell[0].to_le_bytes());
                slot[4..].copy_from_slice(&cell[1].to_le_bytes());
                points.push(Point {
                    id: self.identity(scope, id, stream, &slot)?,
                    cell,
                    position,
                });
            }
        }
        Ok(Value::Points(Arc::new(Points {
            id: stream,
            space: Space::World,
            unit: Unit::Meters,
            points,
        })))
    }
}
