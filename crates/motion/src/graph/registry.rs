use super::definition::Definition;
use crate::nodes;
use std::sync::OnceLock;
pub fn definitions() -> &'static [Definition] {
    static REGISTRY: OnceLock<Vec<Definition>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut result = vec![
            crate::wrangle::definition(),
            nodes::expression::definition_expression(),
            nodes::composition::definition_composition(),
            nodes::text_select::definition_select(),
            nodes::align_content::definition_align(),
            nodes::align_content::definition_fit(),
            nodes::content::rectangle::definition(),
            nodes::content::background::definition_background(),
            nodes::content::ellipse::definition(),
            nodes::content::path::definition(),
            nodes::content::text::definition_text(),
        ];
        result.extend(nodes::distribution::definitions());
        result.extend(nodes::copy_colors::definitions());
        result.extend(nodes::color_ramp::definitions());
        result.push(nodes::stagger::definition_stagger());
        result.extend(nodes::path_motion::definitions());
        result.push(nodes::path_points::definition_points());
        result.extend(nodes::values::definitions());
        result.push(nodes::format_text::definition_format());
        result.push(nodes::text_on_points::definition_labels());
        result.extend(nodes::animation::definitions());
        result.push(nodes::oscillator::definition_oscillator());
        result.extend(nodes::timing::definitions());
        result.push(nodes::format::definition_format());
        result.push(nodes::format::definition_anchor());
        result.extend(nodes::influence::definitions());
        result.extend(nodes::response::definitions());
        result.extend(nodes::scene::definitions());
        result.extend(nodes::interface::definitions());
        result.extend(nodes::solver::definitions());
        result.extend(nodes::attributes::definitions());
        result.push(nodes::history::definition_history());
        result.push(nodes::shape_grid::definition_grid());
        result.push(nodes::shape_points::definition_points());
        result.push(nodes::convex_sweep::definition_sweep());
        result.push(nodes::direction::definition_direction());
        result.extend(nodes::particles::definitions());
        result.extend(nodes::image::definitions());
        result
    })
}
pub fn find(id: &str) -> Result<&'static Definition, String> {
    definitions()
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("unavailable motion node: {id}"))
}
