//! Temporal evaluation re-enters the same authored scope at an exact time.
use super::*;
use crate::nodes::interface::GroupSettings;

impl<'a> Evaluator<'a> {
    pub fn with_runtime(
        motion: &'a Motion,
        time: Time,
        runtime: Arc<crate::simulation::Runtime>,
    ) -> Result<Self, String> {
        let mut result = Self::new(motion, time)?;
        result.runtime = runtime;
        Ok(result)
    }

    /// Rebuild group bindings, including animated upstream inputs, at sample time.
    /// Never reuse lazy fields captured at the original request time.
    pub(crate) fn at_time(&mut self, time: Time) -> Result<Self, String> {
        if self.step_context.is_some() {
            return Err("temporal sampling inside a solver update is not supported; record history in explicit state".into());
        }
        let mut result = Self::for_graph(self.motion, &self.motion.graph, time)?;
        result.runtime = self.runtime.clone();
        result.identity = self.identity.clone();
        result.object_path = self.object_path.clone();
        result.layer_times = self.layer_times.clone();
        result.instance_timing = self.instance_timing;
        result.output_format = self.output_format;
        result.current_object = self.current_object;
        result.budget = std::mem::take(&mut self.budget);
        let rebuild = (|| {
            for id in &self.scope {
                let node = result.graph.node(*id)?.clone();
                let group_id = node
                    .settings::<GroupSettings>()?
                    .group
                    .ok_or("missing group")?;
                let group = self.motion.groups.get(&group_id).ok_or("missing group")?;
                let mut bindings = BTreeMap::new();
                for port in &group.inputs {
                    bindings.insert(port.id.clone(), result.input(&node, &port.id)?);
                }
                result.graph = &group.graph;
                result.bindings = bindings;
                result.cache.clear();
                result.active.clear();
                result.scope.push(*id);
                result.group_depth += 1;
            }
            Ok(())
        })();
        if let Err(error) = rebuild {
            self.budget = result.budget;
            return Err(error);
        }
        Ok(result)
    }

    pub(crate) fn temporal_input(
        &mut self,
        node: &Node,
        socket: &str,
        time: Time,
    ) -> Result<Value, String> {
        let mut sampled = self.at_time(time)?;
        let result = sampled.input(node, socket);
        self.budget = sampled.budget;
        result
    }

    pub(crate) fn simulation_key(
        &mut self,
        node: ObjectId,
    ) -> Result<crate::simulation::Key, String> {
        let identity = match &self.identity {
            Some(value) => value.clone(),
            None => {
                let bytes = Arc::new(self.motion.evaluation_identity()?);
                self.identity = Some(bytes.clone());
                bytes
            }
        };
        Ok(crate::simulation::Key {
            identity,
            timing: self.instance_timing,
            format: self.output_format,
            scope: self.scope.clone(),
            object: self.current_object,
            node,
        })
    }
}
