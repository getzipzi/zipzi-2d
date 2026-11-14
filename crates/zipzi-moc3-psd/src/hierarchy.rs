//! Preserve Part paths without changing the evaluated mesh stacking order.
use crate::Error;
use ag_psd::psd::{BlendMode, Layer, LayerAdditionalInfo};
use std::collections::{HashMap, HashSet};
use zipzi_core::Part;

pub(crate) fn display_name(name: &str, runtime: &str, id: &str) -> String {
    [name, runtime, id]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .unwrap_or(id)
        .into()
}

fn children_at<'a>(mut children: &'a mut Vec<Layer>, indices: &[usize]) -> &'a mut Vec<Layer> {
    for &index in indices {
        children = children[index].children.as_mut().unwrap();
    }
    children
}

fn part_path<'a>(mut id: &'a str, parts: &HashMap<&str, &'a Part>) -> Result<Vec<&'a str>, Error> {
    let mut path = Vec::new();
    while !id.is_empty() {
        if path.contains(&id) || path.len() >= 256 {
            return Err(Error::PsdLimit(
                "PSD Part hierarchy is cyclic or exceeds 256 levels".into(),
            ));
        }
        let part = parts
            .get(id)
            .ok_or_else(|| Error::Model(format!("missing Part {id}")))?;
        path.push(part.id.as_str());
        id = &part.parent_id;
    }
    path.reverse();
    Ok(path)
}

fn folder(part: &Part) -> Layer {
    Layer {
        additional_info: LayerAdditionalInfo {
            name: Some(display_name(&part.name, &part.runtime_id, &part.id)),
            ..Default::default()
        },
        blend_mode: Some(BlendMode::PassThrough),
        hidden: Some(!part.enabled),
        opened: Some(true),
        children: Some(Vec::new()),
        ..Default::default()
    }
}

pub(crate) fn organize(
    entries: Vec<(String, Layer)>,
    parts: &[&Part],
    warnings: &mut Vec<String>,
) -> Result<Vec<Layer>, Error> {
    let records: HashMap<_, _> = parts.iter().map(|p| (p.id.as_str(), *p)).collect();
    let mut root = Vec::new();
    let mut previous = Vec::new();
    let mut indices = Vec::new();
    let mut first_location = HashMap::new();
    let mut split = HashSet::new();
    let mut record_count = entries.len();
    for (part_id, layer) in entries {
        let path = part_path(&part_id, &records)?;
        let common = path
            .iter()
            .zip(&previous)
            .take_while(|(a, b)| a == b)
            .count();
        indices.truncate(common);
        for &id in &path[common..] {
            let part = records[id];
            let siblings = children_at(&mut root, &indices);
            indices.push(siblings.len());
            siblings.push(folder(part));
            record_count += 2; // PSD folder start/end records
            if first_location.contains_key(id) {
                if split.insert(id.to_owned()) {
                    warnings.push(format!("Part {} is split into same-named PSD folders to preserve interleaved drawing order", display_name(&part.name, &part.runtime_id, &part.id)));
                }
            } else {
                first_location.insert(id.to_owned(), indices.clone());
            }
        }
        children_at(&mut root, &indices).push(layer);
        // Own the IDs: part_id is a per-entry temporary.
        previous = path.iter().map(|id| (*id).to_owned()).collect();
    }
    // Empty Parts still carry useful organization. Attach them to the first
    // occurrence of their parent without disturbing any raster layer's order.
    for part in parts {
        let mut indices = Vec::new();
        for id in part_path(&part.id, &records)? {
            if let Some(location) = first_location.get(id) {
                indices = location.clone();
                continue;
            }
            let siblings = children_at(&mut root, &indices);
            indices.push(siblings.len());
            siblings.push(folder(records[id]));
            first_location.insert(id.to_owned(), indices.clone());
            record_count += 2;
        }
    }
    if record_count > i16::MAX as usize {
        return Err(Error::PsdLimit(
            "too many PSD layer and folder records".into(),
        ));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn part(id: &str, parent: &str, name: &str) -> Part {
        Part {
            id: id.into(),
            parent_id: parent.into(),
            name: name.into(),
            ..Default::default()
        }
    }
    fn layer(name: &str) -> Layer {
        Layer {
            additional_info: LayerAdditionalInfo {
                name: Some(name.into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn names(layers: &[Layer]) -> Vec<String> {
        layers
            .iter()
            .flat_map(|l| match &l.children {
                Some(children) => names(children),
                None => vec![l.additional_info.name.clone().unwrap()],
            })
            .collect()
    }
    #[test]
    fn preserves_nested_and_empty_parts_and_interleaved_mesh_order() {
        let head = part("head", "", "头部");
        let eyes = part("eyes", "head", "眼睛");
        let empty = part("empty", "eyes", "空分组");
        let mut warnings = Vec::new();
        let layers = organize(
            vec![
                ("eyes".into(), layer("后眼")),
                ("".into(), layer("中间")),
                ("eyes".into(), layer("前眼")),
            ],
            &[&head, &eyes, &empty],
            &mut warnings,
        )
        .unwrap();
        assert_eq!(names(&layers), vec!["后眼", "中间", "前眼"]);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].additional_info.name.as_deref(), Some("头部"));
        assert_eq!(layers[2].additional_info.name.as_deref(), Some("头部"));
        assert_eq!(layers[0].blend_mode, Some(BlendMode::PassThrough));
        let eyes = &layers[0].children.as_ref().unwrap()[0];
        assert_eq!(eyes.additional_info.name.as_deref(), Some("眼睛"));
        assert_eq!(
            eyes.children.as_ref().unwrap()[1]
                .additional_info
                .name
                .as_deref(),
            Some("空分组")
        );
        assert_eq!(warnings.len(), 2);
    }
    #[test]
    fn display_names_preserve_unicode_duplicates_and_fall_back_only_when_blank() {
        assert_eq!(
            display_name(" 左眼 / 高光 ", "ArtMesh17", "id"),
            " 左眼 / 高光 "
        );
        assert_eq!(display_name("  ", "ArtMesh17", "id"), "ArtMesh17");
        assert_eq!(display_name("", "", "id"), "id");
        let layers = organize(
            vec![("".into(), layer("同名")), ("".into(), layer("同名"))],
            &[],
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(names(&layers), vec!["同名", "同名"]);
    }
}
