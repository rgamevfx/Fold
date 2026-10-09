//! Inspector reference actions edit the same bindings as public Source.
mod samples;
use crate::{
    binding::Binding,
    model::{Construction, Definition, Parameter, Reference},
};
use fold_foundation::{DocumentId, ObjectId};
use fold_ui::sdk::{self, imgui::Ui};
pub(super) use samples::{SampleKey, Samples};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Drafts(pub BTreeMap<(DocumentId, ObjectId, ObjectId), Draft>);
pub(super) struct Draft {
    text: String,
    base: serde_json::Value,
}
fn replace(
    graph: &Construction,
    parameter: &mut Parameter,
    binding: Binding,
) -> Result<(), String> {
    let value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
    let mut candidate = graph.clone();
    let target = candidate
        .definitions
        .iter_mut()
        .flat_map(|d| &mut d.nodes)
        .flat_map(|n| n.parameters.values_mut())
        .find(|p| p.id == parameter.id)
        .ok_or("Missing parameter")?;
    target.value = value.clone();
    crate::protection::validate(graph, &candidate).map_err(|e| e.reason)?;
    candidate.validate_semantics()?;
    parameter.value = value;
    Ok(())
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Clipboard {
    document: DocumentId,
    definition: ObjectId,
    parameter: ObjectId,
}
pub(super) struct Context<'a> {
    pub document: DocumentId,
    pub graph: &'a Construction,
    pub definition: &'a Definition,
    pub owner: &'a str,
    pub sampled: Option<&'a crate::types::Literal>,
    pub editable: bool,
}
pub(super) fn draw(
    ui: &Ui,
    parameter: &mut Parameter,
    drafts: &mut Drafts,
    context: Context<'_>,
) -> Result<(bool, Option<ObjectId>), String> {
    let Context {
        document,
        graph,
        definition,
        owner,
        sampled,
        editable,
    } = context;
    use sdk::parameter_menu::Action;
    let key = (document, definition.id, parameter.id);
    let binding = parameter.binding()?;
    let driven = matches!(binding, Binding::Link { .. } | Binding::Expression { .. });
    let mut changed = false;
    let mut go_to = None;
    let print = |reference| crate::source::reference_name(graph, definition, reference);
    match sdk::parameter_menu::context(ui, driven, editable) {
        Some(Action::Copy) => {
            sdk::parameter_menu::copy_parameter(
                serde_json::to_string(&Clipboard {
                    document,
                    definition: definition.id,
                    parameter: parameter.id,
                })
                .map_err(|e| e.to_string())?,
            );
        }
        Some(Action::PasteRelative) => {
            let text = sdk::parameter_menu::copied_parameter().ok_or("Copy a parameter first")?;
            let copied: Clipboard = serde_json::from_str(&text)
                .map_err(|_| "Clipboard does not contain a Fold parameter")?;
            if copied.document != document || copied.definition != definition.id {
                return Err(
                    "This graph cannot yet resolve a parameter outside its definition".into(),
                );
            }
            let reference = Reference::Parameter {
                parameter: copied.parameter,
                component: None,
            };
            // Resolve the live identity, so rename between Copy and Paste is safe.
            let path = fold_graph::channel::definition_path(definition, reference)?;
            let target = fold_graph::channel::resolve_path(&[], &path)?;
            let relative = fold_graph::channel::relative_path(
                &[definition.name.clone(), owner.into()],
                &target,
            );
            let source = format!(
                "ch({})",
                serde_json::to_string(&relative).map_err(|e| e.to_string())?
            );
            let expression = crate::expression_source::parse(&source, &mut |path| {
                fold_graph::channel::resolve_definition(definition, Some(owner), path)
            })
            .map_err(|e| e.message)?
            .expression;
            replace(graph, parameter, Binding::Expression { expression })?;
            changed = true;
        }
        Some(Action::EditExpression) => {
            let expression =
                match binding.clone() {
                    Binding::Expression { expression } => expression,
                    Binding::Link { reference } => crate::expression::Expr::Reference { reference },
                    Binding::Literal { value } => crate::expression::Expr::Literal { value },
                    _ => return Err(
                        "An animated track must be removed before replacing it with an expression"
                            .into(),
                    ),
                };
            drafts.0.insert(
                key,
                Draft {
                    base: parameter.value.clone(),
                    text: crate::expression_source::canonical(&expression, &mut |r| {
                        if matches!(r, Reference::Parameter { .. }) {
                            let target = fold_graph::channel::resolve_path(&[], &print(r)?)?;
                            Ok(fold_graph::channel::relative_path(
                                &[definition.name.clone(), owner.into()],
                                &target,
                            ))
                        } else {
                            print(r)
                        }
                    })?,
                },
            );
        }
        Some(Action::GoToSource) => {
            let edges = crate::connections::edges(&binding)?;
            if edges.len() != 1 {
                return Err(
                    "Expression has multiple sources; inspect its connections in Network".into(),
                );
            }
            go_to = match edges[0].target {
                Reference::Parameter { parameter, .. } => definition
                    .nodes
                    .iter()
                    .find(|n| n.parameters.values().any(|p| p.id == parameter))
                    .map(|n| n.id)
                    .or_else(|| {
                        definition
                            .inputs
                            .iter()
                            .any(|i| i.id == parameter)
                            .then_some(definition.id)
                    }),
                Reference::Node { node } => Some(node),
                Reference::Input { .. } => Some(definition.id),
                Reference::Instance { instance, .. } => Some(instance),
            };
        }
        Some(Action::RemoveReference) => {
            let value = sampled
                .ok_or("Wait for the current parameter value before removing its reference")?
                .clone();
            replace(graph, parameter, Binding::Literal { value })?;
            drafts.0.remove(&key);
            changed = true;
        }
        None => {}
    }
    let mut close = false;
    if let Some(draft) = drafts.0.get_mut(&key) {
        let stale = draft.base != parameter.value;
        if stale {
            ui.text_wrapped(
                "Parameter changed. Cancel this draft before editing the current value.",
            );
        }
        let text = &mut draft.text;
        ui.set_next_item_width(ui.content_region_avail()[0]);
        ui.input_text("##expression", text).build();
        let apply = {
            let _disabled = ui.begin_disabled_with_cond(stale || !editable);
            ui.button("Apply expression")
        };
        if apply {
            let expression = crate::expression_source::parse(text, &mut |name| {
                if name.starts_with("channel:") {
                    fold_graph::channel::resolve_definition(definition, Some(owner), name)
                } else {
                    crate::source::resolve_reference_name(graph, definition, name)
                }
            })
            .map_err(|e| e.message)?
            .expression;
            replace(graph, parameter, Binding::Expression { expression })?;
            changed = true;
            close = true;
        }
        ui.same_line();
        if ui.button("Cancel") {
            close = true;
        }
    }
    if close {
        drafts.0.remove(&key);
    }
    Ok((changed, go_to))
}

pub(super) fn display(value: &crate::types::Literal) -> String {
    use crate::types::{Literal, Unit};
    match value {
        Literal::Bool { value } => value.to_string(),
        Literal::Integer { value } => value.to_string(),
        Literal::Scalar { value } => format!("{value:.4}"),
        Literal::Distance { value, unit } => format!(
            "{value:.4} {}",
            match unit {
                Unit::Canvas => "canvas units",
                Unit::Meters => "m",
                Unit::Unitless => "",
            }
        ),
        Literal::Duration { value } => format!("{}/{} s", value.numerator(), value.denominator()),
        Literal::Text { value } => value.clone(),
        Literal::Vector { value, .. } => value
            .iter()
            .map(|v| format!("{v:.4}"))
            .collect::<Vec<_>>()
            .join(", "),
        Literal::Color { value, .. } | Literal::Rotation { value, .. } => value
            .iter()
            .map(|v| format!("{v:.4}"))
            .collect::<Vec<_>>()
            .join(", "),
        Literal::AssetRef { .. } => "Asset".into(),
    }
}

pub(super) fn input_menu(
    ui: &Ui,
    document: DocumentId,
    definition: ObjectId,
    input: &mut crate::model::Input,
    editable: bool,
    response: &mut sdk::EditResponse,
) {
    if let Some(_popup) = ui.begin_popup_context_item_with_label(Some("container-parameter")) {
        if ui.menu_item("Copy Parameter") {
            sdk::parameter_menu::copy_parameter(
                serde_json::to_string(&Clipboard {
                    document,
                    definition,
                    parameter: input.id,
                })
                .expect("parameter clipboard"),
            );
        }
        let _disabled = ui.begin_disabled_with_cond(!editable);
        let previous = input.name.clone();
        ui.input_text("Name", &mut input.name).build();
        response.changed |= previous != input.name;
        response.finished |= ui.is_item_deactivated_after_edit();
        use crate::types::{Alpha, Encoding, Literal, Space, Unit};
        let choices = [
            ("Number", Literal::Scalar { value: 0. }),
            ("Integer", Literal::Integer { value: 0 }),
            ("Toggle", Literal::Bool { value: false }),
            (
                "Text",
                Literal::Text {
                    value: String::new(),
                },
            ),
            (
                "Duration",
                Literal::Duration {
                    value: fold_foundation::Time::ZERO,
                },
            ),
            (
                "Canvas distance",
                Literal::Distance {
                    value: 0.,
                    unit: Unit::Canvas,
                },
            ),
            (
                "World distance",
                Literal::Distance {
                    value: 0.,
                    unit: Unit::Meters,
                },
            ),
            (
                "Screen vector",
                Literal::Vector {
                    value: vec![0.; 2],
                    unit: Unit::Canvas,
                    space: Space::Screen,
                },
            ),
            (
                "World vector",
                Literal::Vector {
                    value: vec![0.; 3],
                    unit: Unit::Meters,
                    space: Space::World,
                },
            ),
            (
                "Color",
                Literal::Color {
                    value: [1.; 4],
                    encoding: Encoding::LinearRec709,
                    alpha: Alpha::Straight,
                },
            ),
            (
                "Rotation",
                Literal::Rotation {
                    value: [0., 0., 0., 1.],
                    space: Space::World,
                },
            ),
        ];
        let current = choices
            .iter()
            .find(|(_, v)| v.ty() == input.ty)
            .map_or("Other", |(label, _)| *label);
        if let Some(_combo) = ui.begin_combo("Type", current) {
            for (label, value) in choices {
                if ui
                    .selectable_config(label)
                    .selected(value.ty() == input.ty)
                    .build()
                    && value.ty() != input.ty
                {
                    input.ty = value.ty();
                    input.default = value;
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
    }
}
