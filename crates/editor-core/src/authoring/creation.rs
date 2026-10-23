//! Editor creation defaults and stable insertion locations; domain validation
//! remains in the organization planner.
use super::*;
use kasane_sdk::{OrganizationCollection as Collection, OrganizationEdit as Edit};

impl Document {
    /// Build a single creation transaction from a captured panel selection.
    /// Names may be localized by the host; identities and runtime defaults are
    /// allocated here, using the same rules as the other core creation APIs.
    pub fn plan_parameter_object_creation(
        &self,
        kind: ObjectKind,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<kasane_sdk::OrganizationEditPlan> {
        self.ensure_authoring_idle()?;
        anyhow::ensure!(
            matches!(kind, ObjectKind::Parameter | ObjectKind::ParameterGroup),
            "Expected a parameter or parameter folder"
        );
        let (owner, before) = self.parameter_creation_location(kind, selection)?;
        let intent = if kind == ObjectKind::Parameter {
            let draft = NewParameter {
                name,
                runtime_id: self.suggested_parameter_id(),
                group_id: owner.clone(),
                ..Default::default()
            };
            self.validate_new_parameter(&draft)?;
            Edit::CreateParameter {
                parameter: draft.parameter(&Uuid::new_v4().to_string()),
                group_id: owner,
                before,
            }
        } else {
            let mut intent = self.group_intent(
                None,
                &ParameterGroupDraft {
                    name,
                    runtime_id: self.suggested_parameter_group_id(),
                    parent_id: owner,
                },
            )?;
            if let Edit::CreateGroup {
                before: location, ..
            } = &mut intent
            {
                *location = before;
            }
            intent
        };
        self.plan_organization_authoring(intent)
    }

    pub fn parameter_object_creation_capability(
        &self,
        kind: ObjectKind,
        selection: &[ObjectRef],
    ) -> anyhow::Result<kasane_sdk::CapabilityState> {
        anyhow::ensure!(
            matches!(kind, ObjectKind::Parameter | ObjectKind::ParameterGroup),
            "Expected a parameter or folder"
        );
        let owner = self.parameter_creation_owner(selection)?;
        let collection = if kind == ObjectKind::Parameter {
            Collection::Parameters { group_id: owner }
        } else {
            Collection::ParameterGroups { parent_id: owner }
        };
        Ok(self
            .session
            .organization_collection_capabilities(&collection)?
            .create)
    }
    fn parameter_creation_owner(&self, selection: &[ObjectRef]) -> anyhow::Result<Option<String>> {
        let mut owners = Vec::new();
        for object in selection {
            let owner = match object.kind {
                ObjectKind::Parameter => {
                    self.session
                        .handle(kasane_sdk::ObjectKind::Parameter, &object.id)?;
                    self.model()
                        .parameter_group_for(&object.id)
                        .map(str::to_owned)
                }
                ObjectKind::ParameterGroup => {
                    self.session
                        .handle(kasane_sdk::ObjectKind::CdiParameterGroup, &object.id)?;
                    let group = self.model().get_parameter_group(&object.id).unwrap();
                    if selection.len() == 1 {
                        return Ok(Some(group.id.clone()));
                    }
                    group.parent_id.clone()
                }
                _ => return Ok(None),
            };
            owners.push(owner);
        }
        Ok(owners
            .first()
            .filter(|owner| owners.iter().all(|other| other == *owner))
            .cloned()
            .flatten())
    }
    fn parameter_creation_location(
        &self,
        kind: ObjectKind,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let owner = self.parameter_creation_owner(selection)?;
        if selection.len() == 1 && selection[0].kind == ObjectKind::ParameterGroup {
            return Ok((owner, None));
        }
        if selection.iter().any(|s| {
            !matches!(s.kind, ObjectKind::Parameter | ObjectKind::ParameterGroup)
                || self.model().organization_owner(&s.id) != owner.as_deref()
        }) {
            return Ok((owner, None));
        }
        let collection = if kind == ObjectKind::Parameter {
            Collection::Parameters {
                group_id: owner.clone(),
            }
        } else {
            Collection::ParameterGroups {
                parent_id: owner.clone(),
            }
        };
        let order = self.session.organization_collection(collection)?;
        let before = order
            .iter()
            .rposition(|id| selection.iter().any(|object| object.id == id.id()))
            .and_then(|at| order.get(at + 1))
            .map(|h| h.id().to_owned());
        Ok((owner, before))
    }
}
