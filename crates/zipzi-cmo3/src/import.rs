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
use std::collections::HashMap;
use zipzi_core::Document;
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
    import_model_with_budget(bytes, usize::MAX)
}
/// Bound archive expansion, XML nodes and resulting document before returning
/// ownership. Hosts reserve `workspace` before calling this function.
pub fn import_model_with_budget(bytes: &[u8], workspace: usize) -> Result<ImportedModel> {
    let resources = archive::read_limited(bytes, workspace / 8)?;
    let xml = std::str::from_utf8(
        resources
            .get("main.xml")
            .ok_or_else(|| invalid("Missing main.xml"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    if xml.len() > workspace / 512 {
        return Err(invalid("Cubism XML workspace budget exceeded"));
    }
    let parsed = roxmltree::Document::parse(xml).map_err(|e| invalid(e.to_string()))?;
    let graph = graph::Graph::new(&parsed)?;
    let root = parsed
        .root_element()
        .children()
        .find(|n| n.has_tag_name("main"))
        .and_then(|n| n.first_element_child())
        .ok_or_else(|| invalid("Missing Cubism model"))?;
    let model = model::read(
        &graph,
        graph.resolve(root)?,
        &resources,
        (workspace / 8).min(512 << 20),
    )?;
    if model.document.estimated_content_bytes() > workspace / 4 {
        return Err(invalid("Cubism document workspace budget exceeded"));
    }
    Ok(model)
}
pub fn animation_model_paths(bytes: &[u8]) -> Result<Vec<String>> {
    animation::paths(bytes)
}
pub fn import_animation(bytes: &[u8], document: &mut Document) -> Result<()> {
    animation::read(bytes, document)
}

/// Bounded CAN3 parsing used after a host has reserved its workspace.
pub fn animation_model_paths_with_budget(bytes: &[u8], workspace: usize) -> Result<Vec<String>> {
    animation::paths_with_budget(bytes, workspace)
}
pub fn import_animation_with_budget(
    bytes: &[u8],
    document: &mut Document,
    workspace: usize,
) -> Result<()> {
    animation::read_with_budget(bytes, document, workspace)
}
