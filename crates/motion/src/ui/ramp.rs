//! Stop controls use the same draft, gesture and commit lifecycle as other values.
use super::literal;
use crate::{ramp::Ramp, types::Literal};
use fold_ui::sdk::{EditResponse, imgui::Ui};

pub(super) fn draw(
    ui: &Ui,
    label: &str,
    ramp: &mut Ramp,
    response: &mut EditResponse,
    controls: &mut literal::Controls,
) {
    ui.text(label);
    let mut removed = None;
    let can_remove = ramp.stops.len() > 1;
    for (index, stop) in ramp.stops.iter_mut().enumerate() {
        let _id = ui.push_id(&stop.id.to_string());
        if let Some(_tree) = ui
            .tree_node_config("stop")
            .label(format!("Stop {}", index + 1))
            .default_open(true)
            .push()
        {
            let mut position = Literal::Scalar {
                value: stop.position,
            };
            literal::draw(ui, "Position", &mut position, response, controls);
            if let Literal::Scalar { value } = position {
                stop.position = value;
            }
            literal::draw(ui, "Value", &mut stop.value, response, controls);
            let _disabled = ui.begin_disabled_with_cond(!can_remove);
            if ui.small_button("Remove stop") {
                removed = Some(stop.id);
            }
        }
    }
    if let Some(id) = removed {
        ramp.stops.retain(|stop| stop.id != id);
        response.changed = true;
        response.finished = true;
    }
    ramp.stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    let error = ramp.validate().err();
    {
        let _disabled = ui.begin_disabled_with_cond(
            error.is_some() || controls.invalid || ramp.stops.len() >= 256,
        );
        if ui.button("Add stop") {
            let boundaries: Vec<_> = std::iter::once(0.)
                .chain(ramp.stops.iter().map(|s| s.position))
                .chain(std::iter::once(1.))
                .collect();
            if let Some(gap) = boundaries
                .windows(2)
                .max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
            {
                match ramp.insert((gap[0] + gap[1]) * 0.5) {
                    Ok(_) => {
                        response.changed = true;
                        response.finished = true;
                    }
                    Err(message) => ui.text_wrapped(message),
                }
            }
        }
    }
    if let Some(error) = error {
        ui.text_wrapped(error);
        controls.invalid = true;
        response.finished = false;
    }
}
