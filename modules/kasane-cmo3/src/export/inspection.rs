use super::*;

pub(crate) fn inspect(doc: &Document) -> Result<Cmo3Report, Cmo3Error> {
    inspect_with_animation(doc, false)
}

pub(crate) fn inspect_with_animation(
    doc: &Document,
    animation: bool,
) -> Result<Cmo3Report, Cmo3Error> {
    if let Some(issue) = doc.validate_structure().first() {
        return Err(Cmo3Error::InvalidDocument(format!(
            "{}: {}: {}",
            issue.object_id, issue.status.code, issue.status.message
        )));
    }
    if doc.get_transform(ROOT_DEFORMER).is_some()
        || doc
            .display_info()
            .parameter_groups
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .any(|g| g.id == ROOT_PARAMETER_GROUP)
    {
        return Err(Cmo3Error::InvalidDocument(
            "An authored object uses a reserved Cubism root GUID".into(),
        ));
    }
    let mut report = Cmo3Report::default();
    let mut loss = |id: &str, code: &str, message: &str| {
        report.diagnostics.push(Cmo3Diagnostic {
            severity: DiagnosticSeverity::Loss,
            code: code.into(),
            object_id: id.into(),
            message: message.into(),
        })
    };
    let mut targets = BTreeSet::new();
    for id in doc.blend_binding_order() {
        let binding = doc.get_blend_binding(id).unwrap();
        let table = doc.get_blend_key_table(&binding.key_table_id).unwrap();
        if table.keys[table.base_key_idx] != 0. {
            loss(
                id,
                "NONZERO_BLEND_NEUTRAL",
                "CMO3 morph targets require a neutral key at zero; this binding is omitted",
            );
        }
        let target = if binding.target_kind == BlendShapeTargetKind::Offscreen {
            self::offscreen_owner(doc, &binding.target_id)
        } else {
            binding.target_id.as_str()
        };
        if !targets.insert((target.to_string(), table.parameter_id.clone())) {
            loss(id,"DUPLICATE_BLEND_DRIVER", "Multiple blend curves on the same target and parameter require baking; later curves are omitted");
        }
        let fractional = match &binding.keyforms {
            DeltaKeyforms::Mesh(forms) => forms
                .iter()
                .any(|f| f.draw_order.is_some_and(|v| v.fract() != 0.)),
            DeltaKeyforms::Part(forms) => forms.iter().any(|f| f.draw_order.fract() != 0.),
            _ => false,
        };
        if fractional {
            loss(
                id,
                "FRACTIONAL_DRAW_ORDER",
                "Cubism stores integral draw-order deltas; values will be rounded",
            );
        }
    }

    if let Some(asset) = doc.physics() {
        use kasane_core::physics::PhysicsAxis;
        if asset.data.settings.iter().any(|rig| {
            rig.inputs.iter().any(|v| v.kind == PhysicsAxis::Y)
                || rig.outputs.iter().any(|v| v.kind == PhysicsAxis::Y)
        }) {
            loss(&asset.id,"PHYSICS_Y", "Cubism editable physics supports X and angle inputs/outputs; Y channels are omitted");
        }
        if asset.data.settings.iter().any(|g| {
            g.inputs
                .iter()
                .any(|v| v.source.disabled_reason().is_some())
                || g.outputs
                    .iter()
                    .any(|v| v.destination.disabled_reason().is_some())
        }) {
            return Err(Cmo3Error::InvalidDocument(
                "Physics has unresolved parameter references".into(),
            ));
        }
        let meta = &asset.data.meta;
        if meta.forces.gravity.x != 0.
            || meta.forces.gravity.y != -1.
            || meta.forces.wind.x != 0.
            || meta.forces.wind.y != 0.
        {
            loss(
                &asset.id,
                "PHYSICS_FORCES",
                "CMO3 physics uses the editor's gravity and wind defaults",
            );
        }
        if meta.fps.is_some_and(|fps| fps.fract() != 0.) {
            loss(
                &asset.id,
                "PHYSICS_FPS",
                "CMO3 physics stores an integral FPS",
            );
        }
        if !asset.data.extensions.is_empty()
            || !meta.extensions.is_empty()
            || !meta.forces.extensions.is_empty()
            || meta.dictionary.iter().any(|d| !d.extensions.is_empty())
            || asset.data.settings.iter().any(|rig| {
                !rig.extensions.is_empty()
                    || !rig.normalization.extensions.is_empty()
                    || rig
                        .inputs
                        .iter()
                        .any(|v| !v.extensions.is_empty() || !v.reference_extensions.is_empty())
                    || rig
                        .outputs
                        .iter()
                        .any(|v| !v.extensions.is_empty() || !v.reference_extensions.is_empty())
                    || rig.vertices.iter().any(|v| !v.extensions.is_empty())
            })
        {
            loss(
                &asset.id,
                "PHYSICS_EXTENSIONS",
                "Opaque physics JSON extensions have no editable CMO3 mapping",
            );
        }
    }
    for id in doc.mesh_order() {
        let mesh = doc.get_mesh(id).unwrap();
        if mesh
            .raw_blend_mode
            .is_some_and(|mode| mode >> 16 != 0 || mode & 255 > 17 || (mode >> 8) & 255 > 4)
        {
            return Err(Cmo3Error::InvalidDocument(format!(
                "Unsupported packed blend mode on {id}"
            )));
        }
        if mesh.vertex_ids.len() > i16::MAX as usize {
            return Err(Cmo3Error::InvalidDocument(format!(
                "Mesh {id} exceeds Cubism's editable edge index limit"
            )));
        }

        let fractional = mesh.draw_order.is_some_and(|v| v.fract() != 0.0)
            || doc.binding_for_mesh(id).is_some_and(|b| {
                b.keyforms
                    .iter()
                    .any(|f| f.draw_order.is_some_and(|v| v.fract() != 0.0))
            });
        if fractional {
            loss(
                id,
                "FRACTIONAL_DRAW_ORDER",
                "Cubism stores integral draw orders; values will be rounded",
            );
        }
    }
    for id in doc.part_order() {
        let p = doc.get_part(id).unwrap();
        if p.draw_order.fract() != 0.0
            || doc
                .binding_for_scene(id)
                .is_some_and(|b| b.track.samples().any(|f| f.draw_order.fract() != 0.0))
        {
            loss(
                id,
                "FRACTIONAL_DRAW_ORDER",
                "Cubism stores integral draw orders; values will be rounded",
            );
        }
    }
    let canvas = crate::canvas::for_document(doc)?;
    if canvas.width.fract() != 0.0
        || canvas.height.fract() != 0.0
        || canvas.origin.x.fract() != 0.0
        || canvas.origin.y.fract() != 0.0
    {
        loss(
            doc.id(),
            "FRACTIONAL_CANVAS",
            "Cubism stores integer canvas dimensions and origin; values will be rounded",
        );
    }
    let model3 = doc.model3_settings();
    if model3.groups.as_ref().is_some_and(|groups| {
        groups.iter().any(|group| {
            !matches!(group.name.as_str(), "EyeBlink" | "LipSync")
                || group
                    .parameters
                    .iter()
                    .any(|target| target.object_id().is_none())
        })
    }) || model3.layout.as_ref().is_some_and(|v| !v.is_empty())
        || model3.hit_areas.as_ref().is_some_and(|v| !v.is_empty())
        || model3.user_data.is_some()
        // Name is the model label, supplied through Cmo3Options::model_name.
        || model3.extensions.iter().any(|(key, value)| key != "Name" || !value.is_string())
        || model3.groups.iter().flatten().any(|group| !group.extensions.is_empty())
        || model3.hit_areas.iter().flatten().any(|area| !area.extensions.is_empty())
    {
        loss(doc.id(),"MODEL3_METADATA","Runtime effect groups, layout, hit areas and UserData require a separate model3 package export");
    }
    if (!animation
        && (!doc.motion_order().is_empty()
            || !doc.expression_order().is_empty()
            || !doc.motion_groups().is_empty()))
        || doc.pose().is_some()
        || !doc.package_attachments().is_empty()
    {
        loss(
            doc.id(),
            "RUNTIME_SIDECARS",
            "Motion, expression, pose and package sidecars require a separate package export",
        );
    }
    if !doc.missing_attachments().is_empty() {
        loss(
            doc.id(),
            "MISSING_ATTACHMENTS",
            "Project contains unresolved attachments",
        );
    }
    let display = doc.display_info();
    if display.has_extensions()
        || display
            .combined_parameters
            .as_ref()
            .is_some_and(|v| !v.is_empty())
    {
        loss(
            doc.id(),
            "DISPLAY_METADATA",
            "Opaque CDI extensions and combined parameter metadata are not exported",
        );
    }
    if display.parameters.as_ref().is_some_and(|items| {
        items.iter().any(|entry| {
            matches!(
                entry,
                kasane_core::document::CdiParameterEntry::Unresolved { .. }
            )
        })
    }) || display.parts.as_ref().is_some_and(|items| {
        items.iter().any(|entry| {
            matches!(
                entry,
                kasane_core::document::CdiPartEntry::Unresolved { .. }
            )
        })
    }) {
        loss(
            doc.id(),
            "UNRESOLVED_DISPLAY_METADATA",
            "Unresolved CDI entries cannot become editable Cubism objects",
        );
    }
    if let Some(groups) = doc.draw_order_groups() {
        let owners: BTreeSet<_> = groups.iter().map(|g| g.owner.as_str()).collect();
        for group in groups {
            for id in &group.items {
                let mut parent = if let Some(mesh) = doc.get_mesh(id) {
                    mesh.part_id.as_str()
                } else {
                    doc.get_part(id).unwrap().parent_id.as_str()
                };
                while !parent.is_empty() && !owners.contains(parent) {
                    parent = &doc.get_part(parent).unwrap().parent_id;
                }
                if parent != group.owner {
                    loss(
                        id,
                        "DRAW_ORDER_HIERARCHY",
                        "The drawing-group tree differs from the Cubism part hierarchy",
                    );
                }
            }
            let mut flattened = Vec::new();
            flatten_drawing_children(doc, &group.owner, &owners, &group.items, &mut flattened);
            if flattened != group.items {
                loss(
                    &group.owner,
                    "DRAW_ORDER_TIES",
                    "Drawing-group tie order cannot be expressed by the Cubism part tree",
                );
            }
        }
    }
    if [
        canvas.width,
        canvas.height,
        canvas.origin.x.abs(),
        canvas.origin.y.abs(),
    ]
    .iter()
    .any(|v| *v >= i32::MAX as f32)
        || canvas.width.round() < 1.
        || canvas.height.round() < 1.
    {
        return Err(Cmo3Error::InvalidDocument(
            "Canvas is outside Cubism's integer dimensions".into(),
        ));
    }
    for id in doc.offscreen_order() {
        let os = doc.get_offscreen(id).unwrap();
        if os.blend_mode >> 16 != 0 || os.blend_mode & 255 > 17 || (os.blend_mode >> 8) & 255 > 4 {
            return Err(Cmo3Error::InvalidDocument(format!(
                "Unsupported packed blend mode on {id}"
            )));
        }
    }
    Ok(report)
}
