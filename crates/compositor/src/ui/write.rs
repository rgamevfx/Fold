//! Write exports its own input branch; Output remains the internal document endpoint.
use super::controls::{check, combo, row};
use fold_platform::delivery::*;
use fold_ui::sdk::{EditResponse, imgui::Ui};
pub(super) fn draw(
    ui: &Ui,
    settings: &mut Settings,
    choices: &fold_platform::color::Choices,
    response: &mut EditResponse,
) -> (bool, bool) {
    let width = row(ui, "Destination");
    let button = ui.frame_height();
    ui.set_next_item_width((width - button - ui.clone_style().item_spacing()[0]).max(30.));
    let edited = ui
        .input_text("##destination", &mut settings.destination)
        .build();
    response.item(ui, edited);
    ui.same_line();
    let browse = ui.button_with_size("…##destination", [button, button]);
    fold_ui::sdk::delivery_controls::file_type(ui, settings, false, response);
    if settings.file_type == FileType::OpenExr {
        combo(
            ui,
            "Channels",
            &mut settings.channels,
            &[
                (Channels::Rgba, "rgba"),
                (Channels::Rgb, "rgb"),
                (Channels::Alpha, "alpha"),
            ],
            response,
        );
        combo(
            ui,
            "Data type",
            &mut settings.precision,
            &[
                (Precision::Half, "16-bit half"),
                (Precision::Float, "32-bit float"),
            ],
            response,
        );
        combo(
            ui,
            "Compression",
            &mut settings.compression,
            &[
                (Compression::Zip1, "ZIP (1 scanline)"),
                (Compression::Zip16, "ZIP (16 scanlines)"),
                (Compression::Piz, "PIZ"),
                (Compression::Rle, "RLE"),
                (Compression::None, "None"),
            ],
            response,
        );
        row(ui, "Color space");
        if let Some(_combo) = ui.begin_combo("##color-space", &settings.color_space) {
            for name in choices
                .inputs
                .iter()
                .map(String::as_str)
                .chain(std::iter::once("Linear Rec.709 (sRGB)"))
            {
                if ui
                    .selectable_config(name)
                    .selected(name == settings.color_space)
                    .build()
                {
                    settings.color_space = name.to_owned();
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        check(ui, "Premultiplied", &mut settings.premultiplied, response);
    } else {
        fold_ui::sdk::delivery_controls::movie_options(ui, settings, response);
        if choices.outputs.is_empty() {
            ui.text_disabled("Output color: sRGB");
        } else {
            let mut movie_choices = choices.clone();
            movie_choices.outputs.retain(|c| {
                matches!(
                    c.display.as_str(),
                    "sRGB - Display" | "Rec.1886 Rec.709 - Display"
                )
            });
            if fold_ui::sdk::color_controls::output(ui, &movie_choices, &mut settings.movie_color) {
                response.changed = true;
                response.finished = true;
            }
        }
    }
    let width = row(ui, "Range");
    let spacing = ui.clone_style().item_spacing()[0];
    let label = ui.calc_text_size("OUT")[0];
    let field = ((width - 2. * label - 3. * spacing) * 0.5).max(25.);
    ui.align_text_to_frame_padding();
    ui.text("IN");
    ui.same_line();
    ui.set_next_item_width(field);
    let changed = fold_ui::sdk::imgui::Drag::new("##first")
        .range(0, settings.last)
        .build(ui, &mut settings.first);
    response.item(ui, changed);
    ui.same_line();
    ui.align_text_to_frame_padding();
    ui.text("OUT");
    ui.same_line();
    ui.set_next_item_width(field);
    let changed = fold_ui::sdk::imgui::Drag::new("##last")
        .range(settings.first, i32::MAX as u32)
        .build(ui, &mut settings.last);
    response.item(ui, changed);
    if settings.file_type == FileType::OpenExr {
        row(ui, "Step");
        let changed = fold_ui::sdk::imgui::Drag::new("##step")
            .range(1, i32::MAX as u32)
            .build(ui, &mut settings.step);
        response.item(ui, changed);
    }
    check(ui, "Create folders", &mut settings.create_folders, response);
    let valid = settings.path(settings.first);
    let _disabled = ui.begin_disabled_with_cond(valid.is_err());
    let render = ui.button_with_size("Render", [ui.content_region_avail()[0], 0.]);
    drop(_disabled);
    if !settings.destination.is_empty()
        && let Err(error) = valid
    {
        ui.text_wrapped(error);
    }
    (browse, render)
}
