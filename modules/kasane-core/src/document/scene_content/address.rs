use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneContentTarget {
    VertexPosition {
        vertex_id: VertexId,
        #[serde(default)]
        shape: ShapeTarget,
    },
    VertexUv {
        vertex_id: VertexId,
    },
    Triangle {
        slot: usize,
    },
    WarpPoint {
        row: u32,
        column: u32,
        #[serde(default)]
        shape: ShapeTarget,
    },
    /// Component 0 is the anchor; 1..4 are left, right, up, down handles.
    WarpBezierControl {
        node: usize,
        component: usize,
        #[serde(default)]
        shape: ShapeTarget,
    },
    GluePair {
        pair_id: String,
    },
    OffscreenKeyform {
        slot: usize,
    },
    OffscreenMapping {
        slot: PartKeyformSlot,
    },
}
impl SceneContentTarget {
    pub fn domain(&self) -> SceneContentDomain {
        match self {
            Self::VertexPosition { .. } | Self::VertexUv { .. } | Self::Triangle { .. } => {
                SceneContentDomain::MeshTopology
            }
            Self::WarpPoint { .. } | Self::WarpBezierControl { .. } => {
                SceneContentDomain::WarpTopology
            }
            Self::GluePair { .. } => SceneContentDomain::GluePairs,
            _ => SceneContentDomain::OffscreenKeyforms,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SceneContentValue {
    Point(Vec2),
    Triangle([VertexId; 3]),
    GluePair(GlueVertexPair),
    OffscreenKeyform(OffscreenKeyform),
    Mapping(i32),
}
impl Document {
    pub fn scene_content_value(
        &self,
        owner: &str,
        target: &SceneContentTarget,
    ) -> Result<SceneContentValue, Status> {
        use SceneContentTarget as T;
        use SceneContentValue as V;
        Ok(match target {
            T::VertexPosition { vertex_id, shape } => {
                let i = self
                    .vertex_slot(owner, *vertex_id)
                    .ok_or_else(|| Status::error("MISSING_VERTEX", owner))?;
                V::Point(self.mesh_content_shape(owner, shape)?[i])
            }
            T::VertexUv { vertex_id } => {
                let i = self
                    .vertex_slot(owner, *vertex_id)
                    .ok_or_else(|| Status::error("MISSING_VERTEX", owner))?;
                V::Point(self.get_mesh(owner).unwrap().uvs[i])
            }
            T::Triangle { slot } => V::Triangle(
                *self
                    .get_mesh(owner)
                    .and_then(|m| m.triangles.get(*slot))
                    .ok_or_else(|| Status::error("INVALID_TRIANGLE_SLOT", owner))?,
            ),
            T::WarpPoint { row, column, shape } => {
                let w = self
                    .get_transform(owner)
                    .and_then(|t| t.warp())
                    .ok_or_else(|| Status::error("NOT_WARP", owner))?;
                if *row > w.rows || *column > w.columns {
                    return Err(Status::error("INVALID_WARP_POINT", owner));
                }
                V::Point(
                    self.warp_content_shape(owner, shape)?.0
                        [(row * (w.columns + 1) + column) as usize],
                )
            }
            T::WarpBezierControl {
                node,
                component,
                shape,
            } => {
                let b = self.content_bezier(owner, shape)?;
                let n = b
                    .nodes
                    .get(*node)
                    .ok_or_else(|| Status::error("INVALID_BEZIER_CONTROL", owner))?;
                V::Point(if *component == 0 {
                    n.position
                } else if *component <= 4 && b.has_handle(*node, component - 1) {
                    n.handles[component - 1]
                } else {
                    return Err(Status::error("INVALID_BEZIER_CONTROL", owner));
                })
            }
            T::GluePair { pair_id } => V::GluePair(
                self.get_glue(owner)
                    .and_then(|g| g.pairs.iter().find(|p| p.id == *pair_id))
                    .ok_or_else(|| Status::error("MISSING_GLUE_PAIR", pair_id))?
                    .clone(),
            ),
            T::OffscreenKeyform { slot } => V::OffscreenKeyform(
                self.get_offscreen(owner)
                    .and_then(|o| o.keyforms.get(*slot))
                    .ok_or_else(|| Status::error("INVALID_KEYFORM_SLOT", owner))?
                    .clone(),
            ),
            T::OffscreenMapping { slot } => {
                let os = self
                    .get_offscreen(owner)
                    .ok_or_else(|| Status::error("MISSING_OFFSCREEN", owner))?;
                let i = self.part_keyform_slot(&os.part_id, slot)?;
                let bound = self
                    .binding_for_scene(&os.part_id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                    .map(|b| b.track.len());
                V::Mapping(
                    os.part_keyform_indices
                        .get(i)
                        .copied()
                        .unwrap_or(os.keyform_index(i, bound)?.map_or(-1, |i| i as i32)),
                )
            }
        })
    }
    pub fn scene_content_patch(
        &self,
        owner: &str,
        target: &SceneContentTarget,
        value: SceneContentValue,
    ) -> Result<SceneContentEdit, Status> {
        self.scene_content_value(owner, target)?;
        use SceneContentEdit as E;
        use SceneContentTarget as T;
        use SceneContentValue as V;
        Ok(match (target, value) {
            (T::VertexPosition { vertex_id, shape }, V::Point(p)) => E::MeshVertices {
                mesh_id: owner.into(),
                target: shape.clone(),
                updates: vec![VertexContentPatch {
                    vertex_id: *vertex_id,
                    position: Some(p),
                    uv: None,
                }],
            },
            (T::VertexUv { vertex_id }, V::Point(p)) => E::MeshVertices {
                mesh_id: owner.into(),
                target: ShapeTarget::Base,
                updates: vec![VertexContentPatch {
                    vertex_id: *vertex_id,
                    position: None,
                    uv: Some(p),
                }],
            },
            (T::Triangle { slot }, V::Triangle(vertices)) => E::PatchTriangle {
                mesh_id: owner.into(),
                slot: *slot,
                vertices,
            },
            (T::WarpPoint { row, column, shape }, V::Point(point)) => E::WarpPoints {
                transform_id: owner.into(),
                target: shape.clone(),
                updates: vec![WarpPointPatch {
                    row: *row,
                    column: *column,
                    point,
                }],
                clear_bezier: false,
            },
            (
                T::WarpBezierControl {
                    node,
                    component,
                    shape,
                },
                V::Point(point),
            ) => {
                let mut controller = self.content_bezier(owner, shape)?;
                let n = &mut controller.nodes[*node];
                if *component == 0 {
                    let delta = Vec2::new(point.x - n.position.x, point.y - n.position.y);
                    n.position = point;
                    for h in &mut n.handles {
                        h.x += delta.x;
                        h.y += delta.y;
                    }
                } else {
                    n.handles[component - 1] = point;
                }
                E::WarpBezier {
                    transform_id: owner.into(),
                    target: shape.clone(),
                    controller: Some(controller),
                }
            }
            (T::GluePair { pair_id }, V::GluePair(pair)) if *pair_id == pair.id => {
                E::PatchGluePair {
                    glue_id: owner.into(),
                    pair,
                }
            }
            (T::OffscreenKeyform { slot }, V::OffscreenKeyform(value)) => {
                E::PatchOffscreenKeyform {
                    offscreen_id: owner.into(),
                    slot: *slot,
                    value,
                }
            }
            (T::OffscreenMapping { slot }, V::Mapping(keyform)) => E::SetOffscreenMapping {
                offscreen_id: owner.into(),
                updates: vec![OffscreenMappingPatch {
                    slot: slot.clone(),
                    keyform,
                }],
            },
            _ => return Err(Status::error("WRONG_CONTENT_FIELD", owner)),
        })
    }
    fn mesh_content_shape(&self, owner: &str, target: &ShapeTarget) -> Result<&[Vec2], Status> {
        match target {
            ShapeTarget::Base => self.get_mesh(owner).map(|m| m.base_positions.as_slice()),
            ShapeTarget::Ordinary {
                binding_id,
                samples,
            } => self
                .get_binding(binding_id)
                .filter(|b| b.mesh_id == owner)
                .and_then(|b| {
                    b.keyforms
                        .iter()
                        .find(|f| super::mesh::same_samples(&f.samples, samples))
                })
                .map(|f| f.positions.as_slice()),
            ShapeTarget::Blend {
                binding_id,
                sample_id,
            } => self
                .get_blend_binding(binding_id)
                .filter(|b| b.target_id == owner)
                .and_then(|b| {
                    if let crate::DeltaKeyforms::Mesh(forms) = &b.keyforms {
                        forms
                            .iter()
                            .find(|f| f.sample_id == *sample_id)
                            .map(|f| f.positions.as_slice())
                    } else {
                        None
                    }
                }),
        }
        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", owner))
    }
    fn warp_content_shape(
        &self,
        owner: &str,
        target: &ShapeTarget,
    ) -> Result<(&[Vec2], Option<&WarpBezier>), Status> {
        match target {
            ShapeTarget::Base => self
                .get_transform(owner)
                .and_then(|t| t.warp())
                .map(|w| (w.points.as_slice(), w.bezier.as_ref())),
            ShapeTarget::Ordinary {
                binding_id,
                samples,
            } => self
                .get_scene_binding(binding_id)
                .filter(|b| b.target_id() == owner)
                .and_then(|b| {
                    if let crate::SceneTrack::Warp { keyforms, .. } = &b.track {
                        keyforms
                            .iter()
                            .find(|f| super::mesh::same_samples(&f.samples, samples))
                            .map(|f| (f.positions.as_slice(), f.bezier.as_ref()))
                    } else {
                        None
                    }
                }),
            ShapeTarget::Blend {
                binding_id,
                sample_id,
            } => self
                .get_blend_binding(binding_id)
                .filter(|b| b.target_id == owner)
                .and_then(|b| {
                    if let crate::DeltaKeyforms::Warp(forms) = &b.keyforms {
                        forms
                            .iter()
                            .find(|f| f.sample_id == *sample_id)
                            .map(|f| (f.points.as_slice(), None))
                    } else {
                        None
                    }
                }),
        }
        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", owner))
    }
    fn content_bezier(&self, owner: &str, target: &ShapeTarget) -> Result<WarpBezier, Status> {
        if matches!(target, ShapeTarget::Blend { .. }) {
            return Err(Status::error("BLEND_HAS_NO_BEZIER_CONTROLLER", owner));
        }
        let w = self
            .get_transform(owner)
            .and_then(|t| t.warp())
            .ok_or_else(|| Status::error("NOT_WARP", owner))?;
        let (points, controller) = self.warp_content_shape(owner, target)?;
        Ok(controller
            .cloned()
            .unwrap_or_else(|| WarpBezier::from_lattice(w.rows, w.columns, w.quad, points)))
    }
}
