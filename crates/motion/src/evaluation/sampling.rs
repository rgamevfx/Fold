use super::{Scope, State, Value};
use crate::{binding::Binding, execution::ElementContext};
use fold_foundation::{ObjectId, Time};

impl State<'_> {
    /// Isolate values and child scopes belonging to a different time/element.
    /// Budgets, cancellation and identity collision checks remain request-wide.
    /// Inputs captured at the owning asset boundary remain uniform.
    pub(super) fn sample_binding(
        &mut self,
        scope: &Scope,
        owner: ObjectId,
        binding: &Binding,
        time: Time,
        element: Option<ElementContext>,
    ) -> Result<Value, String> {
        let values = std::mem::take(&mut self.values);
        let scopes = std::mem::take(&mut self.scopes);
        let previous = std::mem::replace(&mut self.element, element);
        let result = self.binding(
            &Scope {
                time,
                ..scope.clone()
            },
            owner,
            binding,
        );
        self.values = values;
        self.scopes = scopes;
        self.element = previous;
        result
    }
}
