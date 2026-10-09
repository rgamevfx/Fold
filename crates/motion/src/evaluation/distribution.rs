//! Deterministic centered grids. Cell correspondence is independent of flattened order.
use super::{Scope, State, Value};
use crate::types::{Literal, Space, Unit};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};

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
        let faces = columns.saturating_sub(1) * rows.saturating_sub(1) * 2;
        self.reserve(1 + total + faces)?;
        let stream = self.identity(scope, id, 0, b"grid")?;
        let mesh = super::mesh::grid(
            columns,
            rows,
            [spacing[0], spacing[1]],
            [center[0], center[1], center[2]],
            stream,
            &mut || self.budget.step(),
        )?;
        Ok(Value::Geometry(Arc::new(super::Geometry {
            attributes: Default::default(),
            id: stream,
            space: Space::World,
            unit: Unit::Meters,
            appearance: Default::default(),
            data: super::GeometryData::Mesh(Arc::new(mesh)),
        })))
    }
}
