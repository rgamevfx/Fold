use super::definition::Definition;
use crate::nodes;
use std::sync::OnceLock;
pub fn definitions() -> &'static [Definition] {
    static REGISTRY: OnceLock<Vec<Definition>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut result = vec![
            nodes::content::rectangle::definition(),
            nodes::content::background::definition_background(),
            nodes::content::ellipse::definition(),
            nodes::content::path::definition(),
            nodes::content::text::definition_text(),
        ];
        result.extend(nodes::distribution::definitions());
        result.extend(nodes::path_motion::definitions());
        result.push(nodes::path_points::definition_points());
        result.extend(nodes::values::definitions());
        result.extend(nodes::animation::definitions());
        result.extend(nodes::influence::definitions());
        result.extend(nodes::response::definitions());
        result.extend(nodes::scene::definitions());
        result.extend(nodes::interface::definitions());
        result
    })
}
pub fn find(id: &str) -> Result<&'static Definition, String> {
    definitions()
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("unavailable motion node: {id}"))
}
