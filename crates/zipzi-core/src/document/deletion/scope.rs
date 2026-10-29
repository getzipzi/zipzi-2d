use super::*;

impl Document {
    pub(super) fn deletion_scope(
        &self,
        request: &DeletionRequest,
        control: &ConversionControl,
    ) -> Result<(BTreeMap<DeletionObject, DeletionReason>, DeletionReport), Status> {
        let index = self.deletion_index(control)?;
        let mut roots = BTreeSet::new();
        for root in &request.roots {
            control.checkpoint()?;
            roots.insert(root.clone());
        }
        let mut removed = BTreeMap::new();
        let mut requested = BTreeSet::new();
        for root in &roots {
            control.checkpoint()?;
            let key = root.object();
            if matches!(root, DeletionTarget::Record(_))
                && matches!(
                    key.kind,
                    DeletionKind::Mesh
                        | DeletionKind::Glue
                        | DeletionKind::Offscreen
                        | DeletionKind::Part
                        | DeletionKind::Deformer
                )
            {
                return Err(Status::error(
                    "INVALID_DELETION_TARGET_KIND",
                    "Use a named scene scope",
                ));
            }
            if key.owner_id.is_some()
                && !matches!(
                    key.kind,
                    DeletionKind::ExpressionTrack
                        | DeletionKind::ExpressionKey
                        | DeletionKind::MotionEvent
                )
            {
                return Err(Status::error("INVALID_DELETION_NAMESPACE", &key.id));
            }
            let exists = match key.kind {
                DeletionKind::Mesh => self.get_mesh(&key.id).is_some(),
                DeletionKind::Glue => self.get_glue(&key.id).is_some(),
                DeletionKind::Offscreen => self.get_offscreen(&key.id).is_some(),
                DeletionKind::Part => self.get_part(&key.id).is_some(),
                DeletionKind::Deformer => self.get_transform(&key.id).is_some(),
                DeletionKind::DrawingGroup => {
                    self.get_part(&key.id).is_some()
                        && self
                            .draw_order_groups()
                            .into_iter()
                            .flatten()
                            .any(|g| g.owner == key.id)
                }
                DeletionKind::GlueBinding => {
                    self.get_glue(&key.id).is_some_and(|g| g.binding.is_some())
                }
                DeletionKind::Parameter => self.get_parameter(&key.id).is_some(),
                DeletionKind::ParameterFolder => self.get_parameter_group(&key.id).is_some(),
                DeletionKind::Texture => self.get_asset(&key.id).is_some(),
                DeletionKind::Attachment => self.get_attachment(&key.id).is_some(),
                DeletionKind::DisplayInfo | DeletionKind::ModelSettings | DeletionKind::CdiPart => {
                    false
                }
                _ => index.objects.contains(&key),
            };
            if !exists {
                return Err(Status::error("MISSING_DELETION_TARGET", &key.id));
            }
            requested.insert(key.clone());
            removed.insert(key, DeletionReason::Requested);
        }
        let mut report = DeletionReport {
            requested: requested.into_iter().collect(),
            requested_scopes: roots.iter().cloned().collect(),
            ..Default::default()
        };
        // Each explicit root contributes its own scope before union. In particular,
        // an explicitly selected Deformer inside a selected Part is not redundant.
        for (organization, graph) in [(true, &index.organization), (false, &index.deformation)] {
            let mut pending: Vec<_> = roots
                .iter()
                .filter_map(|root| match (organization, root) {
                    (true, DeletionTarget::Part(_)) | (false, DeletionTarget::Deformer(_)) => {
                        Some(root.object())
                    }
                    _ => None,
                })
                .collect();
            let mut seen = BTreeSet::new();
            while let Some(owner) = pending.pop() {
                control.checkpoint()?;
                if !seen.insert(owner.clone()) {
                    continue;
                }
                for child in graph.get(&owner.id).into_iter().flatten() {
                    removed
                        .entry(child.clone())
                        .or_insert(DeletionReason::Scope);
                    report.relations.push(DeletionRelation {
                        owner: owner.clone(),
                        field: DeletionField::RequestedScope,
                        target: child.clone(),
                    });
                    if (organization && child.kind == DeletionKind::Part)
                        || (!organization && child.kind == DeletionKind::Deformer)
                    {
                        pending.push(child.clone());
                    }
                }
            }
        }
        let mut folders: Vec<_> = roots
            .iter()
            .filter_map(|r| match r {
                DeletionTarget::ParameterFolderContents(_) => Some(r.object()),
                _ => None,
            })
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(owner) = folders.pop() {
            control.checkpoint()?;
            if !seen.insert(owner.clone()) {
                continue;
            }
            for child in index.folders.get(&owner.id).into_iter().flatten() {
                removed
                    .entry(child.clone())
                    .or_insert(DeletionReason::Scope);
                report.relations.push(DeletionRelation {
                    owner: owner.clone(),
                    field: DeletionField::RequestedScope,
                    target: child.clone(),
                });
                if child.kind == DeletionKind::ParameterFolder {
                    folders.push(child.clone());
                }
            }
        }
        // A curve cannot survive without its first persistent key. Selecting
        // all its keys explicitly retires that track and reports the expansion.
        for (owner, children) in &index.owned {
            if matches!(
                owner.kind,
                DeletionKind::MotionTrack | DeletionKind::ExpressionTrack
            ) && !children.is_empty()
                && children.iter().all(|key| removed.contains_key(key))
            {
                removed
                    .entry(owner.clone())
                    .or_insert(DeletionReason::Dependency);
            }
        }
        Ok((removed, report))
    }

    pub(super) fn deletion_scene_object(&self, id: &str) -> Option<DeletionObject> {
        let kind = if self.get_part(id).is_some() {
            DeletionKind::Part
        } else if self.get_transform(id).is_some() {
            DeletionKind::Deformer
        } else if self.get_mesh(id).is_some() {
            DeletionKind::Mesh
        } else if self.get_offscreen(id).is_some() {
            DeletionKind::Offscreen
        } else if self.get_glue(id).is_some() {
            DeletionKind::Glue
        } else {
            return None;
        };
        Some(object(kind, id))
    }
}
