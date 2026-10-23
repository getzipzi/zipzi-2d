use super::*;
impl Document {
    /// Compare native fields without cloning geometry. Context also includes
    /// topology that invalidates addresses even when its reference targets agree.
    pub fn visit_scene_field_changes(
        &self,
        after: &Document,
        id: &str,
        mut changed: impl FnMut(SceneField),
    ) {
        use SceneField as F;
        macro_rules! fields {
            ($a:ident, $b:ident; $($tag:ident => $value:expr),+ $(,)?) => {$(
                if $a.map($value) != $b.map($value) {
                    changed(F::$tag);
                }
            )+};
        }
        let a = self.get_mesh(id);
        let b = after.get_mesh(id);
        fields!(a, b;
            Label => |v| &v.name,
            RuntimeId => |v| &v.runtime_id,
            Enabled => |v| v.enabled,
            Appearance => |v| v.appearance,
            Texture => |v| &v.texture_asset_id,
            Masks => |v| &v.masks,
            MeshDrawing => |v| (v.draw_order, v.blend_mode, v.raw_blend_mode, v.double_sided, v.inverted_mask),
            Context => |v| (&v.part_id, &v.deformer_id, &v.vertex_ids, &v.triangles),
        );
        let a = self.get_transform(id);
        let b = after.get_transform(id);
        fields!(a, b;
            Label => |v| &v.name,
            RuntimeId => |v| &v.runtime_id,
            Enabled => |v| v.enabled,
            Appearance => |v| v.appearance,
        );
        if a.and_then(Transform::warp) != b.and_then(Transform::warp) {
            changed(F::Warp);
        }
        if a.and_then(Transform::rotation) != b.and_then(Transform::rotation) {
            changed(F::Rotation);
        }
        fields!(a, b;
            Context => |v| (&v.part_id, &v.parent_id, v.warp().map(|w| (w.rows, w.columns, w.quad))),
        );
        let a = self.get_offscreen(id);
        let b = after.get_offscreen(id);
        fields!(a, b;
            Label => |v| &v.name,
            RuntimeId => |v| &v.runtime_id,
            Enabled => |v| v.enabled,
            Masks => |v| &v.masks,
            Offscreen => |v| (v.blend_mode, v.flags, &v.keyforms, &v.part_keyform_indices),
            Context => |v| &v.part_id,
        );
        let a = self.get_glue(id);
        let b = after.get_glue(id);
        fields!(a, b;
            Label => |v| &v.name,
            RuntimeId => |v| &v.runtime_id,
            GlueIntensity => |v| v.intensity,
            GlueEndpoints => |v| (&v.mesh_a_id, &v.mesh_b_id),
            Context => |v| (&v.pairs, &v.mesh_a_id, &v.mesh_b_id),
        );
    }

    pub fn scene_field(&self, id: &str, field: SceneField) -> Result<SceneFieldValue, Status> {
        use SceneField as F;
        use SceneFieldValue as V;
        let m = self.get_mesh(id);
        let t = self.get_transform(id);
        let o = self.get_offscreen(id);
        let g = self.get_glue(id);
        macro_rules! common {
            ($field:ident; $($owner:ident),+) => {
                None $(.or_else(|| $owner.map(|v| &v.$field)))+
            };
        }
        let result = match field {
            F::Label => common!(name; m, t, o, g).cloned().map(V::Label),
            F::RuntimeId => common!(runtime_id; m, t, o, g).cloned().map(V::RuntimeId),
            F::Enabled => common!(enabled; m, t, o).copied().map(V::Enabled),
            F::Appearance => common!(appearance; m, t).copied().map(V::Appearance),
            F::Texture => m.map(|v| V::Texture(v.texture_asset_id.clone())),
            F::Masks => common!(masks; m, o).cloned().map(V::Masks),
            F::MeshDrawing => m.map(|v| V::MeshDrawing {
                draw_order: v.draw_order,
                blend_mode: v.blend_mode,
                raw_blend_mode: v.raw_blend_mode,
                double_sided: v.double_sided,
                inverted_mask: v.inverted_mask,
            }),
            F::Warp => t.and_then(Transform::warp).cloned().map(V::Warp),
            F::Rotation => t.and_then(Transform::rotation).cloned().map(V::Rotation),
            F::GlueEndpoints => g.map(|v| V::GlueEndpoints {
                a: v.mesh_a_id.clone(),
                b: v.mesh_b_id.clone(),
            }),
            F::GlueIntensity => g.map(|v| V::GlueIntensity(v.intensity)),
            F::Offscreen => o.map(|v| V::Offscreen {
                blend_mode: v.blend_mode,
                flags: v.flags,
                keyforms: v.keyforms.clone(),
                part_keyform_indices: v.part_keyform_indices.clone(),
            }),
            F::Context => {
                (m.is_some() || t.is_some() || o.is_some() || g.is_some()).then(|| V::Context {
                    part_id: self.organization_owner(id).map(str::to_owned),
                    parent_id: self
                        .scene_deformer_parent(id)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned),
                })
            }
        };
        result.ok_or_else(|| Status::error("WRONG_SCENE_FIELD", format!("{id}: {field:?}")))
    }
    pub(super) fn validate_scene_field(
        &self,
        id: &str,
        value: &SceneFieldValue,
        clear_pairs: bool,
    ) -> Result<(), Status> {
        use SceneFieldValue as V;
        self.scene_field(id, value.field())?;
        match value {
            V::Label(_) => Ok(()),
            V::RuntimeId(runtime) => {
                if runtime.is_empty() {
                    return Err(Status::error("INVALID_ID", "Runtime ID cannot be empty"));
                }
                let order = if self.get_mesh(id).is_some() {
                    self.mesh_order()
                } else if self.get_transform(id).is_some() {
                    self.transform_order()
                } else if self.get_offscreen(id).is_some() {
                    self.offscreen_order()
                } else {
                    self.glue_order()
                };
                if order.iter().any(|other| {
                    other != id
                        && self
                            .scene_field(other, SceneField::RuntimeId)
                            .is_ok_and(|v| v == *value)
                }) {
                    Err(Status::error("DUPLICATE_RUNTIME_ID", runtime))
                } else {
                    Ok(())
                }
            }
            V::Enabled(_) => Ok(()),
            V::Appearance(appearance) => check(validate_appearance(appearance, id)),
            V::Texture(target) => {
                check(target.validate())?;
                if target
                    .resolved_id()
                    .is_some_and(|target| self.get_asset(target).is_none())
                {
                    Err(Status::error("MISSING_ASSET", target.as_str()))
                } else {
                    Ok(())
                }
            }
            V::Masks(masks) => {
                let mut seen = HashSet::new();
                for mask in masks {
                    if self.get_mesh(mask).is_none() {
                        return Err(Status::error("MISSING_MESH", mask));
                    }
                    if !seen.insert(mask) {
                        return Err(Status::error("INVALID_MASK", id));
                    }
                }
                Ok(())
            }
            V::MeshDrawing { draw_order, .. } => match draw_order {
                Some(value) => check(validate_draw_order(*value, id)),
                None => Ok(()),
            },
            V::Warp(data) => {
                let mut next = self.get_transform(id).unwrap().clone();
                if self.binding_for_scene(id).is_some()
                    || self.iter_blend_bindings_for_target(id).next().is_some()
                {
                    let old = next.warp().unwrap();
                    if (old.rows, old.columns) != (data.rows, data.columns) {
                        return Err(Status::error(
                            "KEYFORMS_REQUIRED",
                            "Use the Warp grid conversion with all keyforms",
                        ));
                    }
                }
                next.data = TransformData::Warp(data.clone());
                check(self.validate_transform(&next))
            }
            V::Rotation(data) => {
                let mut next = self.get_transform(id).unwrap().clone();
                next.data = TransformData::Rotation(data.clone());
                check(self.validate_transform(&next))
            }
            V::GlueEndpoints { a, b } => {
                let mut next = self.get_glue(id).unwrap().clone();
                next.mesh_a_id = a.clone();
                next.mesh_b_id = b.clone();
                if clear_pairs {
                    next.pairs.clear();
                }
                check(self.validate_glue(&next))
            }
            V::GlueIntensity(value) => {
                if value.is_finite() {
                    Ok(())
                } else {
                    Err(Status::error("INVALID_INTENSITY", id))
                }
            }
            V::Offscreen {
                blend_mode,
                flags,
                keyforms,
                part_keyform_indices,
            } => {
                let mut next = self.get_offscreen(id).unwrap().clone();
                next.blend_mode = *blend_mode;
                next.flags = *flags;
                next.keyforms = keyforms.clone();
                next.part_keyform_indices = part_keyform_indices.clone();
                check(self.validate_offscreen(&next))
            }
            V::Context { .. } => Err(Status::error(
                "EXPLICIT_RELATION_EDIT_REQUIRED",
                "Use organization, reparent or Offscreen transfer commands",
            )),
        }
    }
    pub(super) fn install_scene_field(
        &mut self,
        id: &str,
        value: &SceneFieldValue,
        clear_pairs: bool,
    ) {
        use SceneFieldValue as V;
        macro_rules! common {
            ($field:ident, $value:expr; $first:ident $(, $rest:ident)*) => {
                if let Some(v) = self.content.$first.get_mut(id) { v.$field = $value; }
                $(else if let Some(v) = self.content.$rest.get_mut(id) { v.$field = $value; })*
            };
        }
        match value {
            V::Label(name) => {
                common!(name, name.clone(); meshes, transforms, offscreens, glues);
            }
            V::RuntimeId(runtime) => {
                if let Some(m) = self.get_mesh(id) {
                    self.mesh_runtime_ids.remove(&m.runtime_id.clone());
                    self.mesh_runtime_ids.insert(runtime.clone(), id.into());
                }
                common!(runtime_id, runtime.clone(); meshes, transforms, offscreens, glues);
            }
            V::Enabled(enabled) => {
                common!(enabled, *enabled; meshes, transforms, offscreens);
            }
            V::Appearance(appearance) => {
                common!(appearance, *appearance; meshes, transforms);
            }
            V::Texture(target) => {
                self.content.meshes.get_mut(id).unwrap().texture_asset_id = target.clone()
            }
            V::Masks(masks) => {
                common!(masks, masks.clone(); meshes, offscreens);
            }
            V::MeshDrawing {
                draw_order,
                blend_mode,
                raw_blend_mode,
                double_sided,
                inverted_mask,
            } => {
                let m = self.content.meshes.get_mut(id).unwrap();
                m.draw_order = *draw_order;
                m.blend_mode = *blend_mode;
                m.raw_blend_mode = *raw_blend_mode;
                m.double_sided = *double_sided;
                m.inverted_mask = *inverted_mask;
            }
            V::Warp(data) => {
                self.content.transforms.get_mut(id).unwrap().data =
                    TransformData::Warp(data.clone())
            }
            V::Rotation(data) => {
                self.content.transforms.get_mut(id).unwrap().data =
                    TransformData::Rotation(data.clone())
            }
            V::GlueEndpoints { a, b } => {
                let g = self.content.glues.get_mut(id).unwrap();
                g.mesh_a_id = a.clone();
                g.mesh_b_id = b.clone();
                if clear_pairs {
                    for pair in &g.pairs {
                        self.content.editor_state.objects.remove(&pair.id);
                    }
                    g.pairs.clear();
                }
            }
            V::GlueIntensity(intensity) => {
                self.content.glues.get_mut(id).unwrap().intensity = *intensity
            }
            V::Offscreen {
                blend_mode,
                flags,
                keyforms,
                part_keyform_indices,
            } => {
                let os = self.content.offscreens.get_mut(id).unwrap();
                os.blend_mode = *blend_mode;
                os.flags = *flags;
                os.keyforms = keyforms.clone();
                os.part_keyform_indices = part_keyform_indices.clone();
            }
            V::Context { .. } => unreachable!("Read-only context is never installed"),
        }
    }
}
