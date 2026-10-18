//! Native readers for Cubism Editor CAFF object graphs. No source extraction or
//! dependence on an adjacent runtime export is used to reconstruct the rig.
mod animation;
mod archive;
mod graph;
mod metadata;
mod model;
mod morph;
#[cfg(feature = "test-support")]
pub mod test_support;
use crate::Cmo3Error;
use kasane_core::Document;
use std::collections::HashMap;
type Result<T> = std::result::Result<T, Cmo3Error>;
fn invalid(message: impl Into<String>) -> Cmo3Error {
    Cmo3Error::InvalidDocument(message.into())
}
pub struct ImportedModel {
    pub document: Document,
    /// PNG bytes keyed by the document's draft source paths.
    pub images: HashMap<String, Vec<u8>>,
}
pub fn import_model(bytes: &[u8]) -> Result<ImportedModel> {
    let resources = archive::read(bytes)?;
    let xml = std::str::from_utf8(
        resources
            .get("main.xml")
            .ok_or_else(|| invalid("Missing main.xml"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let parsed = roxmltree::Document::parse(xml).map_err(|e| invalid(e.to_string()))?;
    let graph = graph::Graph::new(&parsed)?;
    let root = parsed
        .root_element()
        .children()
        .find(|n| n.has_tag_name("main"))
        .and_then(|n| n.first_element_child())
        .ok_or_else(|| invalid("Missing Cubism model"))?;
    model::read(&graph, graph.resolve(root)?, &resources)
}
pub fn animation_model_paths(bytes: &[u8]) -> Result<Vec<String>> {
    animation::paths(bytes)
}
pub fn import_animation(bytes: &[u8], document: &mut Document) -> Result<()> {
    animation::read(bytes, document)
}
