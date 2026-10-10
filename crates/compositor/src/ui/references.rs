//! Parameter menus edit native bindings; invalid expression drafts never publish.
use crate::{Node, PreparedComposite};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::{binding::Binding, expression::Expr, model::Reference, types::Literal};
use fold_ui::sdk::{self, imgui::Ui};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Drafts {
    drafts: BTreeMap<(DocumentId, ObjectId, String), Draft>,
    pub selected: Option<ObjectId>,
    errors: BTreeMap<(DocumentId, ObjectId, String), String>,
}
struct Draft {
    text: String,
    base: Option<Binding>,
    base_value: f64,
    error: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Clipboard {
    document: DocumentId,
    reference: Reference,
}
pub(super) struct Context<'a, 'b> {
    pub graph: &'a PreparedComposite<'b>,
    pub document: DocumentId,
    pub drafts: &'a mut Drafts,
}
impl Context<'_, '_> {
    pub fn menu(
        &mut self,
        ui: &Ui,
        node: &Node,
        property: &crate::animation::Property,
        component: Option<usize>,
        action: &mut Option<(usize, sdk::parameter_menu::Action)>,
    ) {
        for i in 0..property.values.len() {
            if component.is_some_and(|c| c != i) {
                continue;
            }
            let path = crate::animation::path(&property.id, i);
            let prepared = self.graph.node(node.id).ok();
            let editable = prepared.as_ref().is_some_and(|n| {
                if property.id == "mix" {
                    !n.effect.protected
                } else {
                    n.operation
                        .parameters
                        .get(n.config().parameter_for_property(&property.id))
                        .is_some_and(|p| !p.protected)
                }
            }) && !node
                .animation
                .bindings()
                .get(&path)
                .is_some_and(|p| p.protected);
            let _component = if property.values.len() > 1 && component.is_none() {
                let Some(menu) = ui.begin_menu(property.channels[i]) else {
                    continue;
                };
                Some(menu)
            } else {
                None
            };
            if let Some(a) = sdk::parameter_menu::items(ui, node.animation.driven(&path), editable)
            {
                *action = Some((i, a));
            }
        }
    }
    pub fn finish(
        &mut self,
        ui: &Ui,
        node: &mut Node,
        property: &crate::animation::Property,
        time: Time,
        action: Option<(usize, sdk::parameter_menu::Action)>,
    ) -> bool {
        let mut changed = false;
        let error_key = (self.document, node.id, property.id.clone());
        if let Some((i, action)) = action {
            self.drafts.errors.remove(&error_key);
            if let Err(error) = self.action(node, property, i, time, action, &mut changed) {
                self.drafts.errors.insert(error_key.clone(), error);
            }
        }
        if let Some(error) = self.drafts.errors.get(&error_key) {
            ui.text_wrapped(error);
        }
        for i in 0..property.values.len() {
            let path = crate::animation::path(&property.id, i);
            let key = (self.document, node.id, path.clone());
            let Some(draft) = self.drafts.drafts.get_mut(&key) else {
                continue;
            };
            let _scope = ui.push_id(&format!("expression-{path}"));
            let stale = node.animation.binding(&path).ok().flatten() != draft.base
                || property.values[i] != draft.base_value;
            if property.values.len() > 1 {
                ui.text(format!("{} {}", property.label, property.channels[i]));
            }
            ui.set_next_item_width(ui.content_region_avail()[0]);
            ui.input_text("##expression", &mut draft.text).build();
            let apply = {
                let _disabled = ui.begin_disabled_with_cond(stale);
                ui.button("Apply")
            };
            ui.same_line();
            let cancel = ui.button("Cancel");
            let source = draft.text.clone();
            if stale {
                ui.text_wrapped("Parameter changed; cancel this draft to edit its current value.");
            }
            if !draft.error.is_empty() {
                ui.text_wrapped(&draft.error);
            }
            if apply {
                let result = self
                    .graph
                    .parse_expression(node.id, &source)
                    .and_then(|expression| {
                        set(self.graph, node, &path, Binding::Expression { expression })
                    });
                match result {
                    Ok(()) => {
                        changed = true;
                        self.drafts.drafts.remove(&key);
                    }
                    Err(error) => self.drafts.drafts.get_mut(&key).unwrap().error = error,
                }
            } else if cancel {
                self.drafts.drafts.remove(&key);
            }
        }
        changed
    }
    pub(super) fn action(
        &mut self,
        node: &mut Node,
        property: &crate::animation::Property,
        i: usize,
        time: Time,
        action: sdk::parameter_menu::Action,
        changed: &mut bool,
    ) -> Result<(), String> {
        use sdk::parameter_menu::Action;
        let path = crate::animation::path(&property.id, i);
        let reference = self
            .graph
            .node(node.id)?
            .property_reference(&property.id, i)?;
        let binding = node.animation.binding(&path)?;
        let key = (self.document, node.id, path.clone());
        match action {
            Action::Copy => sdk::parameter_menu::copy_parameter(
                serde_json::to_string(&Clipboard {
                    document: self.document,
                    reference,
                })
                .map_err(|e| e.to_string())?,
            ),
            Action::PasteRelative => {
                let copied: Clipboard = serde_json::from_str(
                    &sdk::parameter_menu::copied_parameter().ok_or("Copy a parameter first")?,
                )
                .map_err(|_| "Copy a compositor component first")?;
                if copied.document != self.document {
                    return Err("Choose a parameter in this compositor graph".into());
                }
                self.graph.reference_owner(copied.reference)?;
                set(
                    self.graph,
                    node,
                    &path,
                    Binding::Link {
                        reference: copied.reference.into(),
                    },
                )?;
                *changed = true;
            }
            Action::EditExpression => {
                let expression = match binding.clone() {
                    Some(Binding::Expression { expression }) => expression,
                    Some(Binding::Link { reference }) => Expr::Reference { reference },
                    Some(Binding::Track { .. }) => {
                        return Err(
                            "Remove animation before replacing it with an expression".into()
                        );
                    }
                    _ => Expr::Literal {
                        value: Literal::Scalar {
                            value: property.values[i],
                        },
                    },
                };
                let text = fold_graph::expression_source::display(&expression, &mut |r| {
                    self.graph.channel_name(node.id, r)
                })?;
                self.drafts.drafts.insert(
                    key,
                    Draft {
                        text,
                        base: binding,
                        base_value: property.values[i],
                        error: String::new(),
                    },
                );
            }
            Action::GoToSource => {
                let refs = match binding {
                    Some(Binding::Link { reference }) => vec![reference.target],
                    Some(Binding::Expression { expression }) => expression.references()?,
                    _ => vec![],
                };
                let owners: std::collections::BTreeSet<_> = refs
                    .into_iter()
                    .map(|r| self.graph.reference_owner(r).map(|v| v.0))
                    .collect::<Result<_, _>>()?;
                if owners.len() != 1 {
                    return Err(
                        "Expression has multiple sources; edit it to inspect its references".into(),
                    );
                }
                self.drafts.selected = owners.first().copied();
            }
            Action::RemoveReference => {
                let value = self.graph.sample_reference(reference, time)?;
                let mut values = property.values.clone();
                values[i] = value;
                let mut next = node.clone();
                next.animation.remove(&path)?;
                next.set_property(&property.id, &values)?;
                validate(self.graph, &next)?;
                *node = next;
                self.drafts.drafts.remove(&key);
                *changed = true;
            }
        }
        Ok(())
    }
}
fn validate(graph: &PreparedComposite<'_>, node: &Node) -> Result<(), String> {
    let mut next = (**graph).clone();
    *next.node_mut(node.id)? = node.clone();
    graph.check_protection(&next)?;
    next.prepare()?;
    Ok(())
}
fn set(
    graph: &PreparedComposite<'_>,
    node: &mut Node,
    path: &str,
    binding: Binding,
) -> Result<(), String> {
    let mut next = node.clone();
    next.animation.set_binding(path.into(), binding)?;
    validate(graph, &next)?;
    *node = next;
    Ok(())
}
