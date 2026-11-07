//! Scene creation defaults and independent organization/deformation positions.
//! The SDK validates owners, locks and object data and publishes one transaction.
use super::moving::PaletteProjection;
use super::*;
use zipzi_contract::document::OrganizationCollection;
use zipzi_contract::document::OrganizationEdit;
use zipzi_contract::document::SceneCollection;
use zipzi_contract::document::SceneCreate;
use zipzi_contract::document::SceneEdit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneObjectType {
    Part,
    Mesh,
    Warp,
    Rotation,
    Offscreen,
}
impl SceneObjectType {
    pub fn object_kind(self) -> ObjectKind {
        match self {
            Self::Part => ObjectKind::Part,
            Self::Mesh => ObjectKind::Mesh,
            Self::Warp | Self::Rotation => ObjectKind::Deformer,
            Self::Offscreen => ObjectKind::Offscreen,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SceneObjectCreationPlan {
    projection: PaletteProjection,
    plan: CreationPlan,
}
impl SceneObjectCreationPlan {
    pub fn projection(&self) -> PaletteProjection {
        self.projection
    }
}
#[derive(Clone, Debug)]
enum CreationPlan {
    Organization(crate::EditPreview<zipzi_core::state::OrganizationOutcome>),
    Scene(crate::EditPreview<zipzi_core::state::SceneOutcome>),
}

impl Document {
    /// Lightweight menu availability. Query native destination locks and
    /// Offscreen validation without allocating identities or building plans.
    /// Candidates are settled by the host before the final creation planner.
    pub fn validate_scene_object_creation_context(
        &self,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<()> {
        self.validate_scene_object_creation_in(PaletteProjection::Organization, kind, selection)
    }

    pub fn validate_scene_object_creation_in(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<()> {
        let host = self.runtime_host();
        if host
            .read()
            .saving_job(host.client_id(), self.runtime_session_id())?
            .is_some()
        {
            return Err(
                zipzi_core::Status::error("PROJECT_BUSY", "A project save is outstanding").into(),
            );
        }
        if matches!(kind, SceneObjectType::Warp | SceneObjectType::Rotation) {
            self.ensure_editable(&self.deformer_creation_members(selection)?)?;
        }
        let (part_id, parent_id) = match projection {
            PaletteProjection::Organization => (self.scene_creation_container(selection)?.0, None),
            PaletteProjection::Deformation => {
                anyhow::ensure!(
                    matches!(
                        kind,
                        SceneObjectType::Mesh | SceneObjectType::Warp | SceneObjectType::Rotation
                    ),
                    AuthoringPlanningError::new("NO_DEFORMATION_COLLECTION")
                );
                let (part, parent, _) = self.deformation_creation_container(selection)?;
                (part, parent)
            }
            PaletteProjection::Parameters => {
                anyhow::bail!(AuthoringPlanningError::new("NO_SCENE_COLLECTION"))
            }
        };
        let state = self
            .model()
            .collection_capabilities(&CollectionTarget::Organization(
                OrganizationCollection::PartMembers {
                    part_id: part_id.clone(),
                },
            ))?
            .create;
        if let zipzi_contract::authoring::CapabilityState::Unavailable { code }
        | zipzi_contract::authoring::CapabilityState::Unsupported { code } = state
        {
            anyhow::bail!(AuthoringPlanningError::new(code));
        }
        if projection == PaletteProjection::Deformation {
            let state = self
                .model()
                .collection_capabilities(&CollectionTarget::Scene(SceneCollection::Deformation {
                    parent_id,
                }))?
                .create;
            if let zipzi_contract::authoring::CapabilityState::Unavailable { code }
            | zipzi_contract::authoring::CapabilityState::Unsupported { code } = state
            {
                anyhow::bail!(AuthoringPlanningError::new(code));
            }
        }
        if kind == SceneObjectType::Offscreen {
            let part_id = part_id.ok_or_else(|| {
                anyhow::anyhow!(AuthoringPlanningError::new("OFFSCREEN_REQUIRES_PART"))
            })?;
            // The document UUID cannot be another scene object's identity.
            // Validation uses native defaults, as the final SceneCreate does.
            let status = self.model().validate_offscreen(&zipzi_core::Offscreen {
                id: self.model().id().to_owned(),
                part_id,
                ..Default::default()
            });
            if !status.is_ok() {
                return Err(AuthoringPlanningError::from_status(status).into());
            }
        }
        Ok(())
    }

    /// Capture organization placement. New deformers also wrap the selected
    /// deformation roots while keeping existing organization ownership intact.
    pub fn plan_scene_object_creation(
        &self,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.plan_scene_object_creation_in(PaletteProjection::Organization, kind, name, selection)
    }

    pub fn plan_scene_object_creation_in(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.plan_scene_object_creation_at(projection, kind, name, selection, None)
    }

    fn plan_scene_object_creation_at(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
        empty_origin: Option<crate::ModelPoint>,
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.ensure_authoring_idle()?;
        anyhow::ensure!(
            projection != PaletteProjection::Parameters,
            AuthoringPlanningError::new("NO_SCENE_COLLECTION")
        );
        if projection == PaletteProjection::Deformation {
            anyhow::ensure!(
                selection
                    .iter()
                    .all(|o| matches!(o.kind, ObjectKind::Mesh | ObjectKind::Deformer)),
                AuthoringPlanningError::new("EXPECTED_DEFORMATION_MEMBERS")
            );
        }
        if matches!(kind, SceneObjectType::Warp | SceneObjectType::Rotation) {
            let members = self.deformer_creation_members(selection)?;
            if !members.is_empty() {
                self.ensure_editable(&members)?;
                let (part_id, before) = self.scene_creation_location(kind, selection)?;
                let name = self.scene_creation_name(name, part_id.as_deref())?;
                let create = SceneEdit::Create {
                    id: Uuid::new_v4().to_string(),
                    name,
                    object: if kind == SceneObjectType::Warp {
                        SceneCreate::Warp { bounds: None }
                    } else {
                        SceneCreate::Rotation { origin: None }
                    },
                    part_id,
                    parent_id: None,
                    before,
                    deformation_before: None,
                };
                let source = self.session.capture_authoring();
                let limits = zipzi_core::state::ConversionLimits::default();
                let plan = crate::EditPreview::convert(
                    &source,
                    "Create deformer group",
                    limits,
                    &Default::default(),
                    move |commands, control| {
                        commands.group_deformer(
                            create,
                            members.into_iter().map(|object| object.id).collect(),
                            limits,
                            control,
                        )
                    },
                )?;
                return Ok(SceneObjectCreationPlan {
                    projection,
                    plan: CreationPlan::Scene(plan),
                });
            }
        }
        let (part_id, parent_id, before, deformation_before) = match projection {
            PaletteProjection::Organization => {
                let (part, before) = self.scene_creation_location(kind, selection)?;
                (part, None, before, None)
            }
            PaletteProjection::Deformation => {
                anyhow::ensure!(
                    matches!(
                        kind,
                        SceneObjectType::Mesh | SceneObjectType::Warp | SceneObjectType::Rotation
                    ),
                    AuthoringPlanningError::new("NO_DEFORMATION_COLLECTION")
                );
                let (part, parent, after) = self.deformation_creation_container(selection)?;
                let before = if after {
                    let members = self
                        .model()
                        .scene_collection(&SceneCollection::Deformation {
                            parent_id: parent.clone(),
                        })?;
                    let members: Vec<_> = members
                        .iter()
                        .filter(|object| {
                            parent.is_some()
                                || self.model().transform(object).is_some()
                                    == (kind != SceneObjectType::Mesh)
                        })
                        .collect();
                    let selected: std::collections::HashSet<_> =
                        selection.iter().map(|object| object.id.as_str()).collect();
                    members
                        .iter()
                        .rposition(|object| selected.contains(object.as_str()))
                        .and_then(|i| members.get(i + 1))
                        .map(|object| object.to_string())
                } else {
                    None
                };
                (part, parent, None, before)
            }
            PaletteProjection::Parameters => {
                anyhow::bail!(AuthoringPlanningError::new("NO_SCENE_COLLECTION"))
            }
        };
        let name = self.scene_creation_name(name, part_id.as_deref())?;
        let id = Uuid::new_v4().to_string();
        Ok(SceneObjectCreationPlan {
            projection,
            plan: if kind == SceneObjectType::Part {
                CreationPlan::Organization(self.plan_organization_authoring(
                    OrganizationEdit::CreatePart {
                        part: zipzi_core::Part {
                            id,
                            name,
                            parent_id: part_id.unwrap_or_default(),
                            ..Default::default()
                        },
                        before,
                    },
                )?)
            } else {
                CreationPlan::Scene(self.plan_scene_authoring(SceneEdit::Create {
                    id,
                    name,
                    object: match kind {
                        SceneObjectType::Mesh => SceneCreate::Mesh {
                            texture: Default::default(),
                        },
                        SceneObjectType::Warp => SceneCreate::Warp { bounds: None },
                        SceneObjectType::Rotation => SceneCreate::Rotation {
                            origin: empty_origin.map(|p| {
                                zipzi_core::PreciseVec2::new(f64::from(p.x), f64::from(p.y))
                            }),
                        },
                        SceneObjectType::Offscreen => SceneCreate::Offscreen,
                        SceneObjectType::Part => unreachable!(),
                    },
                    part_id,
                    parent_id,
                    before,
                    deformation_before,
                })?)
            },
        })
    }

    fn deformation_creation_container(
        &self,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>, bool)> {
        let mut parents = Vec::new();
        let mut owners = Vec::new();
        for object in selection {
            anyhow::ensure!(
                matches!(object.kind, ObjectKind::Mesh | ObjectKind::Deformer),
                AuthoringPlanningError::new("EXPECTED_DEFORMATION_MEMBERS")
            );
            // Only validate identity here. Full capabilities also scan deletion
            // dependencies; destination availability is checked by the caller.
            self.model()
                .require_object(object.kind.sdk_kind().unwrap(), &object.id)?;
            let owner = self
                .model()
                .organization_owner(&object.id)
                .map(str::to_owned);
            if selection.len() == 1 && object.kind == ObjectKind::Deformer {
                return Ok((owner, Some(object.id.clone()), false));
            }
            let model = self.model();
            let parent = model.scene_deformer_parent(&object.id).ok_or_else(|| {
                anyhow::anyhow!(AuthoringPlanningError::new("EXPECTED_DEFORMATION_MEMBERS"))
            })?;
            parents.push((!parent.is_empty()).then(|| parent.to_owned()));
            owners.push(owner);
        }
        let Some(parent) = parents.first().cloned() else {
            return Ok((None, None, false));
        };
        if parents.iter().any(|other| *other != parent) {
            return Ok((None, None, false));
        }
        let owner = owners.first().cloned().flatten();
        let owner = if owners.iter().all(|other| *other == owner) {
            owner
        } else {
            None
        };
        Ok((owner, parent, true))
    }

    fn deformer_creation_members(&self, selection: &[ObjectRef]) -> anyhow::Result<Vec<ObjectRef>> {
        let mut members = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut parts = std::collections::HashSet::new();
        for object in selection {
            self.model().require_object(
                object
                    .kind
                    .sdk_kind()
                    .ok_or_else(|| anyhow::anyhow!("Expected a scene object"))?,
                &object.id,
            )?;
            if matches!(object.kind, ObjectKind::Mesh | ObjectKind::Deformer)
                && seen.insert(object.id.clone())
            {
                members.push(object.clone());
            } else if object.kind == ObjectKind::Part {
                parts.insert(object.id.as_str());
            }
        }
        if !parts.is_empty() {
            let model = self.model();
            for id in model.transform_order().iter().chain(model.mesh_order()) {
                let mut owner = model.organization_owner(id).unwrap_or("");
                while !owner.is_empty() {
                    if parts.contains(owner) {
                        if seen.insert(id.clone()) {
                            members.push(ObjectRef::new(
                                if model.mesh(id).is_some() {
                                    ObjectKind::Mesh
                                } else {
                                    ObjectKind::Deformer
                                },
                                id,
                            ));
                        }
                        break;
                    }
                    owner = model.part(owner).map_or("", |part| part.parent_id.as_str());
                }
            }
        }
        Ok(members)
    }

    fn scene_creation_name(&self, name: String, owner: Option<&str>) -> anyhow::Result<String> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "Object name is required");
        let index = self.object_index();
        let names: std::collections::HashSet<_> = index
            .objects
            .iter()
            .filter(|info| {
                matches!(
                    info.reference.kind,
                    ObjectKind::Part
                        | ObjectKind::Mesh
                        | ObjectKind::Deformer
                        | ObjectKind::Offscreen
                ) && self.model().organization_owner(&info.reference.id) == owner
            })
            .map(|info| info.name.as_str())
            .collect();
        if !names.contains(name) {
            return Ok(name.to_owned());
        }
        for suffix in 2.. {
            let candidate = format!("{name} {suffix}");
            if !names.contains(candidate.as_str()) {
                return Ok(candidate);
            }
        }
        unreachable!()
    }

    // `after_selection` distinguishes sibling insertion from container append.
    // Availability only needs the owner; collection order is queried on creation.
    fn scene_creation_container(
        &self,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, bool)> {
        let mut owners = Vec::new();
        for object in selection {
            anyhow::ensure!(
                matches!(
                    object.kind,
                    ObjectKind::Part
                        | ObjectKind::Mesh
                        | ObjectKind::Deformer
                        | ObjectKind::Offscreen
                ),
                "Expected organization tree members"
            );
            // Only validate identity here. Full capabilities also scan deletion
            // dependencies; destination availability is checked by the caller.
            self.model()
                .require_object(object.kind.sdk_kind().unwrap(), &object.id)?;
            if selection.len() == 1 && object.kind == ObjectKind::Part {
                return Ok((Some(object.id.clone()), false));
            }
            owners.push(
                self.model()
                    .organization_owner(&object.id)
                    .map(str::to_owned),
            );
        }
        let Some(owner) = owners.first().cloned() else {
            return Ok((None, false));
        };
        if owners.iter().any(|candidate| *candidate != owner) {
            return Ok((None, false));
        }
        Ok((owner, true))
    }

    fn scene_creation_location(
        &self,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let (owner, after_selection) = self.scene_creation_container(selection)?;
        if !after_selection {
            return Ok((owner, None));
        }
        let order = self
            .model()
            .organization_collection(&OrganizationCollection::PartMembers {
                part_id: owner.clone(),
            })?;
        // The root projection displays Parts and unowned scene objects in
        // separate branches; each branch retains the SDK's complete order.
        let order: Vec<_> = order
            .iter()
            .filter(|handle| {
                owner.is_some()
                    || (self.model().part(handle).is_some()) == (kind == SceneObjectType::Part)
            })
            .collect();
        let selected: std::collections::HashSet<_> =
            selection.iter().map(|object| object.id.as_str()).collect();
        let before = order
            .iter()
            .rposition(|handle| selected.contains(handle.as_str()))
            .and_then(|index| order.get(index + 1))
            .map(|handle| (*handle).clone());
        Ok((owner, before))
    }

    pub fn execute_scene_object_creation(
        &mut self,
        plan: &SceneObjectCreationPlan,
    ) -> anyhow::Result<OperationResult> {
        match &plan.plan {
            CreationPlan::Organization(plan) => self.execute_organization_authoring(plan),
            CreationPlan::Scene(plan) => self.execute_scene_authoring(plan),
        }
    }
}

impl crate::Editor {
    /// UI creation uses the last active Modeling viewport for empty placement.
    /// Grouping still computes its origin from the frozen evaluated selection.
    pub fn plan_scene_object_creation_in(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        let binding = self
            .active_session()
            .and_then(|s| s.modeling.active_viewport)
            .and_then(|viewport| self.viewport_binding(viewport))
            .filter(|b| self.viewport_binding_valid(*b))
            .or_else(|| self.primary_viewport_binding());
        let center = binding.and_then(|binding| {
            let metrics = self.viewport_metrics(binding.viewport)?;
            let transform = crate::ViewportTransform::new(metrics, self.viewport_view(binding)?)?;
            Some(transform.to_model(crate::ViewportPoint {
                x: metrics.width / 2.,
                y: metrics.height / 2.,
            }))
        });
        self.document()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?
            .plan_scene_object_creation_at(projection, kind, name, selection, center)
    }

    pub fn execute_scene_object_creation(
        &mut self,
        document: crate::DocumentId,
        plan: &SceneObjectCreationPlan,
    ) -> anyhow::Result<OperationResult> {
        self.ensure_authoring_target(document, "Scene creation")?;
        anyhow::ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling),
            "Modeling context changed"
        );
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_object_creation(plan))
    }
}
