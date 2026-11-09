use std::collections::HashMap;

use zipzi_core::evaluation::Drawable;
use zipzi_core::state::DocumentState;
use zipzi_core::types::Status;

use crate::layout::{Layout, checked};
use crate::schema::section;
use crate::types::{Moc3Artifact, TextureSlot};

use super::helpers::index_map;

pub(super) struct BindingView<'a> {
    pub(super) id: &'a str,
    pub(super) axes: &'a [zipzi_core::types::BindingAxis],
}

pub(super) struct Moc3EncoderContext<'a> {
    pub(super) doc: &'a DocumentState,
    pub(super) export_version: u8,
    pub(super) drawables: &'a [Drawable],
    pub(super) parts: Vec<String>,
    pub(super) transforms: Vec<String>,
    pub(super) part_indices: HashMap<String, usize>,
    pub(super) transform_indices: HashMap<String, usize>,
    pub(super) mesh_indices: HashMap<String, usize>,
    pub(super) parameter_indices: HashMap<String, usize>,
    pub(super) glue_indices: HashMap<String, usize>,
    pub(super) offscreen_indices: HashMap<String, usize>,
    pub(super) all_bindings: Vec<BindingView<'a>>,
    pub(super) binding_indices: HashMap<&'a str, i32>,
    pub(super) table_indices: HashMap<&'a str, Vec<i32>>,
    pub(super) param_bkts: HashMap<&'a str, Vec<&'a str>>,
    pub(super) ordered_bkts: Vec<&'a str>,
    pub(super) bkt_indices: HashMap<&'a str, i32>,
    pub(super) constraint_index_map: HashMap<&'a str, i32>,
    pub(super) os_key_bases: HashMap<&'a str, i32>,
    pub(super) warp_local_indices: HashMap<String, usize>,
    pub(super) rotation_local_indices: HashMap<String, usize>,
    pub(super) l: Layout,
    pub(super) keyform_offset: i32,
}

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn new(
        doc: &'a DocumentState,
        export_version: u8,
        drawables: &'a [Drawable],
    ) -> Result<Self, Status> {
        let parts = doc.sorted_parts();
        let transforms = doc.sorted_transforms();
        let part_indices = index_map(&parts);
        let transform_indices = index_map(&transforms);
        let mesh_indices = index_map(doc.mesh_order());
        let parameter_indices = index_map(doc.parameter_order());
        let glue_indices = index_map(doc.glue_order());
        let offscreen_indices = index_map(doc.offscreen_order());

        let mut all_bindings = Vec::new();
        for id in doc.binding_order() {
            all_bindings.push(BindingView {
                id,
                axes: &doc.binding(id).unwrap().axes,
            });
        }
        for id in doc.scene_binding_order() {
            all_bindings.push(BindingView {
                id,
                axes: &doc.scene_binding(id).unwrap().axes,
            });
        }
        for id in doc.glue_order() {
            if let Some(binding) = &doc.glue(id).unwrap().binding {
                all_bindings.push(BindingView {
                    id,
                    axes: &binding.axes,
                });
            }
        }

        let mut l = Layout::with_version(export_version);
        let n = checked(drawables.len(), "art_meshes")?;
        l.counts[4] = n as u32;
        l.counts[19] = n as u32;
        l.counts[12] = checked(all_bindings.len() + 1, "bindings")? as u32; // Binding 0 is static.
        l.counts[5] = checked(doc.parameter_order().len(), "parameters")? as u32;
        l.counts[0] = checked(parts.len(), "parts")? as u32;
        l.counts[1] = checked(transforms.len(), "deformers")? as u32;

        if export_version >= 6 {
            l.counts[35] = doc.offscreen_order().len() as u32;
        }

        let mut table_indices: HashMap<&'a str, Vec<i32>> = HashMap::new();
        for b in &all_bindings {
            table_indices.insert(b.id, vec![0; b.axes.len()]);
        }

        let mut param_bkts: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        for bkt_id in doc.blend_key_table_order() {
            let bkt = doc.blend_key_table(bkt_id).unwrap();
            param_bkts
                .entry(bkt.parameter_id.as_str())
                .or_default()
                .push(bkt_id);
        }

        let mut ordered_bkts: Vec<&'a str> = Vec::new();
        let mut bkt_indices: HashMap<&'a str, i32> = HashMap::new();
        for id in doc.parameter_order() {
            if let Some(bkts) = param_bkts.get(id.as_str()) {
                for &bkt_id in bkts {
                    bkt_indices.insert(bkt_id, ordered_bkts.len() as i32);
                    ordered_bkts.push(bkt_id);
                }
            }
        }
        for bkt_id in doc.blend_key_table_order() {
            if !bkt_indices.contains_key(bkt_id.as_str()) {
                bkt_indices.insert(bkt_id, ordered_bkts.len() as i32);
                ordered_bkts.push(bkt_id);
            }
        }

        Ok(Self {
            doc,
            export_version,
            drawables,
            parts,
            transforms,
            part_indices,
            transform_indices,
            mesh_indices,
            parameter_indices,
            glue_indices,
            offscreen_indices,
            all_bindings,
            binding_indices: HashMap::new(),
            table_indices,
            param_bkts,
            ordered_bkts,
            bkt_indices,
            constraint_index_map: HashMap::new(),
            os_key_bases: HashMap::new(),
            warp_local_indices: HashMap::new(),
            rotation_local_indices: HashMap::new(),
            l,
            keyform_offset: 0,
        })
    }

    pub(super) fn encode_canvas(&mut self) -> Result<(), Status> {
        // Normal evaluation uses placeholders for invisible meshes. Include
        // their actual geometry in bounds, since they can become visible later.
        let mut frame = zipzi_core::DrawableFrame::default();

        let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
            self.doc,
            &Default::default(),
            &mut frame,
        );
        if !status.is_ok() {
            return Err(status);
        }

        let c = zipzi_core::geometry::finite_export_canvas(
            self.doc.setup().export,
            frame
                .drawables
                .iter()
                .flat_map(|d| d.positions.iter().copied()),
        )?;
        let canvas = self.l.section(section::CANVAS_INFO)?;
        canvas.extend_from_slice(&c.pixels_per_unit.to_le_bytes());
        canvas.extend_from_slice(&c.origin.x.to_le_bytes());
        canvas.extend_from_slice(&(c.height - c.origin.y).to_le_bytes());
        canvas.extend_from_slice(&c.width.to_le_bytes());
        canvas.extend_from_slice(&c.height.to_le_bytes());
        canvas.resize(24, 0);
        canvas[20] = c.flag; // Core applies this direction to stored geometry, UVs and winding.

        self.l.integer("binding_src.key_table_idx_off", 0)?;
        self.l.integer("binding_src.key_table_idx_len", 0)?;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<Moc3Artifact, Status> {
        self.l.counts[22] = checked(
            self.l.field("glue_key_src.intensity")?.len() / 4,
            "glue keyforms",
        )? as u32;

        self.l.counts[29] = checked(
            self.l
                .field("blend_constraint_idx_src.constraint_idx")?
                .len()
                / 4,
            "bs_constraint_idx",
        )? as u32;

        let colors_count =
            checked(self.l.field("keyform_mul_color_src.r")?.len() / 4, "colors")? as u32;
        self.l.counts[23] = colors_count;
        self.l.counts[24] = colors_count;
        self.l.counts[17] =
            checked(self.l.field("mask_src.art_mesh_idx")?.len() / 4, "masks")? as u32;
        self.l.counts[10] =
            checked(self.l.field("key_pos_src.xy")?.len() / 4, "keyform_pos")? as u32;
        self.l.counts[15] = checked(self.l.field("uv_src.xy")?.len() / 4, "uvs")? as u32;
        self.l.counts[16] = checked(self.l.field("idx_src.idx")?.len() / 2, "idx")? as u32;

        let mut result = Moc3Artifact {
            bytes: self.l.finish()?,
            model3_json:
                "{\n  \"Version\": 3,\n  \"FileReferences\": {\n    \"Moc\": \"model.moc3\",\n    \"Textures\": ["
                    .to_string(),
            textures: Vec::new(),
        };

        for id in self.doc.asset_order() {
            let asset = self.doc.asset(id).unwrap();
            let zipzi_core::state::AssetContent::Image { width, height, .. } = asset.content else {
                return Err(Status::error("MISSING_IMAGE_CONTENT", id));
            };
            let path = format!("textures/{}.png", result.textures.len());
            if !result.textures.is_empty() {
                result.model3_json.push(',');
            }
            result.model3_json.push_str(&format!("\n      \"{path}\""));
            result.textures.push(TextureSlot {
                asset_id: id.clone(),
                package_path: path,
                width,
                height,
            });
        }

        result.model3_json.push_str("\n    ]\n  }");
        if let Some(bindings) = &self.doc.editor_state().cursor_preview_bindings {
            let by_runtime_id: std::collections::BTreeMap<_, _> = bindings
                .iter()
                .map(|(id, binding)| (&self.doc.parameter(id).unwrap().runtime_id, binding))
                .collect();
            let value = serde_json::to_string(&by_runtime_id)
                .map_err(|error| Status::error("INVALID_CURSOR_PREVIEW", error.to_string()))?;
            result.model3_json.push_str(&format!(
                ",\n  \"{}\": {value}",
                crate::CURSOR_PREVIEW_MODEL3_FIELD,
            ));
        }
        result.model3_json.push_str("\n}\n");
        Ok(result)
    }
}
