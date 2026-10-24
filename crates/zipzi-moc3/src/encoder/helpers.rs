use std::collections::HashMap;

use zipzi_core::evaluation::parent_positions;
use zipzi_core::types::{Appearance, Status, Vec2, VertexId};
use zipzi_core::Document;

use crate::layout::{checked, Layout};

pub(super) fn is_representable(id: &str) -> bool {
    !id.is_empty() && id.len() <= 63 && id.bytes().all(|c| (0x20..=0x7e).contains(&c))
}

pub(super) fn index_map(ids: &[String]) -> HashMap<String, usize> {
    ids.iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect()
}

pub(super) fn index_of(ids: &HashMap<String, usize>, id: &str) -> i32 {
    ids.get(id).map(|&index| index as i32).unwrap_or(-1)
}

pub(super) fn find_vertex_pos(
    mesh: &zipzi_core::types::Mesh,
    vid: VertexId,
) -> Result<u16, Status> {
    let index = mesh
        .vertex_ids
        .iter()
        .position(|&v| v == vid)
        .ok_or_else(|| {
            Status::error("MISSING_VERTEX", format!("{}: Glue vertex {vid}", mesh.id))
        })?;
    u16::try_from(index).map_err(|_| {
        Status::error(
            "CAPACITY",
            format!("{}: Glue index {index} exceeds u16", mesh.id),
        )
    })
}

pub(super) fn write_id(
    l: &mut Layout,
    field: usize,
    field_name: &str,
    value: &str,
) -> Result<(), Status> {
    if !is_representable(value) {
        return Err(Status::error(
            "UNREPRESENTABLE_ID",
            format!("{field_name}: requires 1..63 printable ASCII bytes"),
        ));
    }
    let bytes = l.section(field)?;
    let start = bytes.len();
    bytes.extend_from_slice(value.as_bytes());
    bytes.resize(start + 64, 0);
    Ok(())
}

pub(super) fn write_colors(
    l: &mut Layout,
    prefix: &str,
    appearance: &Appearance,
) -> Result<(), Status> {
    let offset = checked(l.field("keyform_mul_color_src.r")?.len() / 4, "colors")?;
    l.integer(&format!("{prefix}.key_mul_color_off"), offset)?;
    l.integer(&format!("{prefix}.key_scr_color_off"), offset)?;
    let channels = ["r", "g", "b"];
    for (c, &ch) in channels.iter().enumerate() {
        l.scalar(
            &format!("keyform_mul_color_src.{ch}"),
            appearance.multiply[c],
        )?;
        l.scalar(&format!("keyform_scr_color_src.{ch}"), appearance.screen[c])?;
    }
    Ok(())
}

pub(super) fn write_bs_colors(
    l: &mut Layout,
    prefix: &str,
    multiply: Option<[f32; 3]>,
    screen: Option<[f32; 3]>,
) -> Result<(), Status> {
    let mul = multiply.unwrap_or([0.0, 0.0, 0.0]);
    let scr = screen.unwrap_or([0.0, 0.0, 0.0]);
    let offset = checked(l.field("keyform_mul_color_src.r")?.len() / 4, "colors")?;
    l.integer(
        &format!("{prefix}.key_mul_color_off"),
        if multiply.is_some() { offset } else { -1 },
    )?;
    l.integer(
        &format!("{prefix}.key_scr_color_off"),
        if screen.is_some() { offset } else { -1 },
    )?;
    let channels = ["r", "g", "b"];
    for c in 0..3 {
        l.scalar(&format!("keyform_mul_color_src.{}", channels[c]), mul[c])?;
        l.scalar(&format!("keyform_scr_color_src.{}", channels[c]), scr[c])?;
    }
    Ok(())
}

/// Convert pixel-sized model/rotation units to Cubism units at the file boundary.
pub(super) fn format_units(doc: &Document, parent: &str, point: Vec2) -> Result<Vec2, Status> {
    let point = if parent.is_empty()
        || doc
            .get_transform(parent)
            .is_some_and(|t| t.rotation().is_some())
    {
        let ppu = doc.export_settings().pixels_per_unit;
        Vec2::new(point.x / ppu, point.y / ppu)
    } else {
        point
    };
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            "Cubism coordinate conversion overflows float32",
        ));
    }
    Ok(point)
}

pub(super) fn format_delta(doc: &Document, parent: &str, point: Vec2) -> Result<Vec2, Status> {
    format_units(doc, parent, point)
}

pub(super) fn write_positions(
    l: &mut Layout,
    doc: &Document,
    prefix: &str,
    parent: &str,
    positions: &[Vec2],
) -> Result<(), Status> {
    let converted = parent_positions(doc, parent, positions)?;
    let offset = checked(l.field("key_pos_src.xy")?.len() / 4, "positions")?;
    l.integer(&format!("{prefix}.key_pos_off"), offset)?;
    for &p in converted {
        let p = format_units(doc, parent, p)?;
        l.scalar("key_pos_src.xy", p.x)?;
        l.scalar("key_pos_src.xy", p.y)?;
    }
    Ok(())
}

pub(super) fn write_delta_positions(
    l: &mut Layout,
    doc: &Document,
    prefix: &str,
    parent: &str,
    positions: &[Vec2],
    expected_len: usize,
) -> Result<(), Status> {
    let offset = checked(l.field("key_pos_src.xy")?.len() / 4, "positions")?;
    l.integer(&format!("{prefix}.key_pos_off"), offset)?;
    if positions.is_empty() {
        for _ in 0..expected_len {
            l.scalar("key_pos_src.xy", 0.0)?;
            l.scalar("key_pos_src.xy", 0.0)?;
        }
    } else {
        for p in positions {
            let p = format_delta(doc, parent, *p)?;
            l.scalar("key_pos_src.xy", p.x)?;
            l.scalar("key_pos_src.xy", p.y)?;
        }
    }
    Ok(())
}
