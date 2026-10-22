use super::*;
type MeshSample<'a> = (&'a [f32], &'a [Vec2], Appearance, Option<f32>);

impl Exporter<'_> {
    pub(super) fn meshes(&mut self, textures: &HashMap<String, TextureBinding>) -> Vec<N> {
        let mut sources = Vec::new();
        for id in self.doc.mesh_order() {
            let m = self.doc.get_mesh(id).unwrap();
            let owner = self.source(id);
            let uvs: Vec<_> = m.uvs.iter().map(|uv| Vec2::new(uv.x, 1. - uv.y)).collect();
            let binding = self.doc.binding_for_mesh(id).filter(|b| !b.axes.is_empty());
            let rest = self.rest[id].clone();
            let mut guids = Vec::new();
            let mut forms = Vec::new();
            let samples: Vec<MeshSample<'_>> = binding
                .map(|v| {
                    v.keyforms
                        .iter()
                        .map(|f| (&f.keys[..], &f.positions[..], f.appearance, f.draw_order))
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![(
                        &[] as &[f32],
                        m.base_positions.as_slice(),
                        m.appearance,
                        m.draw_order,
                    )]
                });
            let default_order = m.draw_order.unwrap_or_else(|| {
                self.doc.mesh_order().iter().position(|v| v == id).unwrap() as f32
            });
            for (_, positions, a, order) in &samples {
                let (base, g) = self.form_base(&owner);
                guids.push(g);
                let base = appearance(
                    N::new("ACDrawableForm").field("super", base).field(
                        "drawOrder",
                        i(order.unwrap_or(default_order).round() as i32),
                    ),
                    *a,
                )
                .field("coordType", coord(&m.deformer_id));
                forms.push(
                    self.graph
                        .add(N::new("CArtMeshForm").field("super", base).field(
                            "positions",
                            self.source_positions(&m.deformer_id, positions),
                        )),
                );
            }
            let grid = self.grid(
                binding.map_or(&[], |v| &v.axes),
                &guids,
                Some(samples.iter().map(|v| v.0.to_vec()).collect()),
            );
            let vertices: HashMap<_, _> = m
                .vertex_ids
                .iter()
                .enumerate()
                .map(|(idx, id)| (*id, idx))
                .collect();
            let mut indices: Vec<_> = m
                .triangles
                .iter()
                .flat_map(|t| t.iter().map(|v| vertices[v]))
                .collect();
            {
                for triangle in indices.as_chunks_mut::<3>().0 {
                    triangle.swap(1, 2);
                }
            }
            let mut edges = BTreeSet::new();
            for t in indices.as_chunks::<3>().0 {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    edges.insert((a.min(b), a.max(b)));
                }
            }
            let mesh_guid = self.new_guid("GEditableMeshGuid");
            let editable = N::new("GEditableMesh2")
                .attr("nextPointUid", m.vertex_ids.len())
                .attr("useDelaunayTriangulation", false)
                .field("point", points(&rest))
                .field(
                    "pointPriority",
                    N::array("byte-array", std::iter::repeat_n(10, m.vertex_ids.len())),
                )
                .field(
                    "edge",
                    N::array("short-array", edges.iter().flat_map(|(a, b)| [*a, *b])),
                )
                .field(
                    "edgePriority",
                    N::array("byte-array", std::iter::repeat_n(30, edges.len())),
                )
                .field("pointUid", N::array("int-array", 0..m.vertex_ids.len()))
                .field("meshGuid", mesh_guid)
                .field(
                    "coordType",
                    N::new("CoordType").field("coordName", s("Basic Coord")),
                );
            let editable_base = self.extension_base(&owner);
            let editable = N::new("CEditableMeshExtension")
                .field("super", editable_base)
                .field("editableMesh", editable);
            let TextureBinding {
                texture,
                atlas_guid,
            } = &textures[m.texture_asset_id.as_str()];
            let input = self.graph.reserve("CTextureInputExtension");
            let asset = self.doc.get_asset(&m.texture_asset_id).unwrap();
            let region = self.graph.add(
                N::new("CTextureInput_TextureAtlasRegion")
                    .field(
                        "super",
                        N::new("ACTextureInput")
                            .field("optionalTransformOnCanvas", identity())
                            .field("_owner", input.clone()),
                    )
                    .field("textureAtlasGuid", atlas_guid.clone())
                    .field(
                        "inputImageLocalToCanvasTransform",
                        affine(fit_texture(
                            &uvs,
                            &rest,
                            asset.content.as_ref().unwrap().width,
                            asset.content.as_ref().unwrap().height,
                        )),
                    ),
            );
            let input_base = self.extension_base(&owner);
            self.graph.set(
                &input,
                N::new("CTextureInputExtension")
                    .field("super", input_base)
                    .field("_textureInputs", N::list("carray_list", [region.clone()]))
                    .field("currentTextureInputData", region),
            );
            let generator_base = self.extension_base(&owner);
            let generator = N::new("CMeshGeneratorExtension")
                .field("super", generator_base)
                .field(
                    "meshGenerateSetting",
                    N::new("MeshGenerateSetting")
                        .field("polygonOuterDensity", i(100))
                        .field("polygonInnerDensity", i(100))
                        .field("polygonMargin", i(20))
                        .field("polygonInnerMargin", i(20))
                        .field("polygonMinMargin", i(5))
                        .field("polygonMinBoundsPt", i(5))
                        .field("thresholdAlpha", i(0)),
                );
            let controllable = self.controllable(
                &m.name,
                m.enabled,
                &m.part_id,
                grid,
                vec![input, editable, generator],
            );
            let drawable = N::new("ACDrawableSource")
                .field("super", controllable)
                .field("id", ident("CDrawableId", &m.runtime_id))
                .field("guid", guid("CDrawableGuid", id))
                .field("targetDeformerGuid", self.parent_deformer(&m.deformer_id))
                .field(
                    "clipGuidList",
                    N::list(
                        "carray_list",
                        m.masks.iter().map(|id| guid("CDrawableGuid", id)),
                    ),
                )
                .field("invertClippingMask", b(m.inverted_mask))
                .field("icon32", N::null())
                .field("icon16", N::null());
            let source = N::new("CArtMeshSource")
                .field("super", drawable)
                .field("indices", N::array("int-array", indices))
                .field("keyforms", N::list("carray_list", forms))
                .field("positions", points(&rest))
                .field("uvs", points(&uvs))
                .field("texture", texture.clone())
                .field(
                    "colorComposition",
                    N::enumeration(
                        "ColorComposition",
                        color_mode(m.raw_blend_mode.unwrap_or(match m.blend_mode {
                            BlendMode::Normal => 0,
                            BlendMode::Additive => 1,
                            BlendMode::Multiplicative => 2,
                        })),
                    ),
                )
                .field("culling", b(!m.double_sided))
                .field(
                    "textureState",
                    N::enumeration("TextureState", "TEXTURE_ATLAS"),
                )
                .field("userData", s(""))
                .field(
                    "alphaComposition",
                    N::enumeration(
                        "AlphaComposition",
                        alpha_mode(m.raw_blend_mode.unwrap_or(0)),
                    ),
                );
            self.graph.set(&owner, source);
            sources.push(owner);
        }
        sources
    }
    pub(super) fn glues(&mut self) -> Vec<N> {
        let mut sources = Vec::new();
        for id in self.doc.glue_order() {
            let glue = self.doc.get_glue(id).unwrap();
            let owner = self.source(id);
            let mut forms = Vec::new();
            let mut guids = Vec::new();
            let intensities: Vec<_> = glue
                .binding
                .as_ref()
                .map(|b| b.keyforms.iter().map(|f| f.intensity).collect())
                .unwrap_or_else(|| vec![glue.intensity]);
            for intensity in intensities {
                let (base, g) = self.form_base(&owner);
                guids.push(g);
                forms.push(
                    self.graph.add(
                        N::new("CGlueForm")
                            .field("super", N::new("ACAffecterForm").field("super", base))
                            .field("intensity", f(intensity)),
                    ),
                );
            }
            let grid = self.grid(glue.binding.as_ref().map_or(&[], |b| &b.axes), &guids, None);
            let mesh_a = self.doc.get_mesh(&glue.mesh_a_id).unwrap();
            let mesh_b = self.doc.get_mesh(&glue.mesh_b_id).unwrap();
            let uids: Vec<_> = glue
                .pairs
                .iter()
                .flat_map(|p| {
                    [
                        mesh_a
                            .vertex_ids
                            .iter()
                            .position(|v| Some(*v) == p.vertex_a)
                            .unwrap(),
                        mesh_b
                            .vertex_ids
                            .iter()
                            .position(|v| Some(*v) == p.vertex_b)
                            .unwrap(),
                    ]
                })
                .collect();
            let base = N::new("ACAffecterSource")
                .field(
                    "super",
                    self.controllable(
                        &glue.name,
                        true,
                        &self.doc.get_mesh(&glue.mesh_a_id).unwrap().part_id,
                        grid,
                        vec![],
                    ),
                )
                .field("guid", guid("CAffecterGuid", id))
                .field("id", ident("CAffecterId", &glue.runtime_id))
                .field("targetDeformerGuid", self.parent_deformer(""))
                .field("editVersion", i(1));
            let source = N::new("CGlueSource")
                .field("super", base)
                .field(
                    "weights",
                    N::array(
                        "float-array",
                        glue.pairs.iter().flat_map(|p| [p.weight_a, p.weight_b]),
                    ),
                )
                .field("keyforms", N::list("carray_list", forms))
                .field(
                    "targetArtMeshA_guid",
                    guid("CDrawableGuid", &glue.mesh_a_id),
                )
                .field(
                    "targetArtMeshB_guid",
                    guid("CDrawableGuid", &glue.mesh_b_id),
                )
                .field("bindVertexUids", N::array("long-array", uids))
                .field(
                    "tabPosOnCanvas",
                    N::new("GVector2").field("x", f(0)).field("y", f(0)),
                );
            self.graph.set(&owner, source);
            sources.push(owner);
        }
        sources
    }
}
