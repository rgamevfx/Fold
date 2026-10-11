//! Native image-source declarations. Source identities are static so the project
//! can validate dependencies before evaluation; only source time is animated.
use crate::{
    binding::Binding,
    model::{Construction, Node},
    types::{AssetReference, Literal},
};
use fold_foundation::{AssetId, DocumentId};
use fold_platform::image_source::Source;
use fold_project::DocumentRef;
use std::collections::BTreeSet;

pub fn source(node: &Node) -> Result<Source, String> {
    let literal = |name: &str| -> Result<Literal, String> {
        match node
            .parameters
            .get(name)
            .ok_or_else(|| format!("Missing image {name}"))?
            .binding()?
        {
            Binding::Literal { value } => Ok(value),
            _ => Err("Image source identity must be a literal; animate Source Time instead".into()),
        }
    };
    match node.mode.as_str() {
        "asset" => match literal("asset")? {
            Literal::AssetRef {
                value: AssetReference::Resource(id),
            } => Ok(Source::Asset(id)),
            _ => Err("Image Texture requires an imported image or video asset".into()),
        },
        "output" => {
            let Literal::Text { value: document } = literal("document")? else {
                return Err("Image document must be a reference".into());
            };
            let Literal::Text { value: output } = literal("output")? else {
                return Err("Image output must be a port".into());
            };
            let document: DocumentId = serde_json::from_value(serde_json::json!(document))
                .map_err(|_| "Invalid image document ID")?;
            if output.is_empty() {
                return Err("Select an image output".into());
            }
            Ok(Source::Output(DocumentRef {
                document,
                output,
                extensions: Default::default(),
            }))
        }
        _ => Err("Unsupported Image Texture mode".into()),
    }
}
pub fn declarations(graph: &Construction) -> Result<(Vec<DocumentRef>, Vec<AssetId>), String> {
    let mut dependencies = vec![];
    let mut assets = BTreeSet::new();
    for node in graph.definitions.iter().flat_map(|d| &d.nodes) {
        if node.operation != "fold.motion.image_texture" || node.version != 1 {
            continue;
        }
        match source(node)? {
            Source::Asset(id) => {
                assets.insert(id);
            }
            Source::Output(reference) => {
                if !dependencies.contains(&reference) {
                    dependencies.push(reference);
                }
            }
        }
    }
    Ok((dependencies, assets.into_iter().collect()))
}
