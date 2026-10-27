//! Admission before roxmltree/Graph allocation. Text-heavy XML must not be
//! charged as though every byte introduced another node and graph entry.
use super::{invalid, Result};
use std::collections::HashMap;

pub(super) fn check_xml(
    xml: &[u8],
    resources: &HashMap<String, Vec<u8>>,
    workspace: usize,
    kind: &str,
) -> Result<()> {
    // A '<' bounds element/text/comment nodes; '=' bounds attributes and
    // identity-map entries. Count occurrences inside strings too: overcounting
    // is safe. DTD expansion remains disabled by roxmltree's default parser.
    // Include Vec/HashMap capacity growth, normalized strings and the CAN3 copy.
    let tree = xml.iter().fold(xml.len().saturating_mul(2), |bytes, byte| {
        bytes.saturating_add(match byte {
            b'<' => 320,
            b'=' => 192,
            _ => 0,
        })
    });
    let archive = resources.values().fold(0usize, |bytes, value| {
        bytes.saturating_add(value.len().saturating_mul(2))
    });
    // The last quarter reserves one eighth each for the document and one RGBA
    // decode; image validation frees that decode before reading the next image.
    if tree.saturating_add(archive).saturating_add(workspace / 4) > workspace {
        return Err(invalid(format!("{kind} XML workspace budget exceeded")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_heavy_xml_fits_without_admitting_dense_nodes_or_attributes() {
        let text = format!("<main>{}</main>", "0 ".repeat(600_000)).into_bytes();
        let resources = HashMap::from([("main.xml".into(), text.clone())]);
        assert!(check_xml(&text, &resources, 16 << 20, "Cubism").is_ok());
        for dense in ["<n/>".repeat(40_000), " a='v'".repeat(40_000)] {
            assert!(check_xml(dense.as_bytes(), &HashMap::new(), 4 << 20, "Cubism").is_err());
        }
        assert!(check_xml(&text, &resources, 1 << 20, "CAN3").is_err());
    }
}
