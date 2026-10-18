//! Test fixture relocation without decoding/re-encoding authored model or scene data.
use super::{archive, graph::Graph, invalid, Result};

/// Copy a CAN3 archive with model references made relative to its own directory.
/// Callers write the returned bytes to a temporary fixture, never over the source.
pub fn animation_with_sibling_models(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut resources = archive::read(bytes)?;
    let mut xml = String::from_utf8(
        resources
            .remove("main.xml")
            .ok_or_else(|| invalid("Missing main.xml"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let parsed = roxmltree::Document::parse(&xml).map_err(|e| invalid(e.to_string()))?;
    let graph = Graph::new(&parsed)?;
    let mut edits = Vec::new();
    for model in parsed
        .descendants()
        .filter(|n| n.has_tag_name("CResource_Linked_Model") && n.attribute("xs.ref").is_none())
    {
        let file = graph.req(model, "srcFile")?;
        let (path, range) = if let Some(attribute) = file.attribute_node("path") {
            (attribute.value(), attribute.range_value())
        } else {
            let text = file
                .children()
                .find(|n| n.is_text())
                .ok_or_else(|| invalid("Fixture model has no path"))?;
            (text.text().unwrap(), text.range())
        };
        let portable = path.replace('\\', "/");
        let filename = portable
            .rsplit('/')
            .next()
            .filter(|n| !n.is_empty())
            .ok_or_else(|| invalid("Empty fixture model name"))?;
        let escaped = filename
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;");
        edits.push((range, escaped));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    edits.dedup_by(|a, b| a.0 == b.0);
    drop(parsed);
    for (range, filename) in edits {
        xml.replace_range(range, &filename);
    }
    let mut resources: Vec<_> = resources.into_iter().collect();
    resources.sort_by(|a, b| a.0.cmp(&b.0));
    crate::caff::encode(resources, xml.as_bytes(), 0)
}
