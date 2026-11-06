//! Immutable persistent model, isolated typed construction and net-write summaries.
mod cost;
mod domains;
mod records;
mod validation;
mod writes;
pub use domains::OwnedDocument;

pub use writes::{RecordChange, RecordDomain, RecordWrite, RootDomain, WriteSet};
pub use zipzi_contract::kernel::{AssetContent, AssetRecord, ContentId, ResourceKey};

use crate::document::*;
use crate::draw_order::DrawOrderGroup;
use crate::types::*;
use crate::{
    document::valid_uuid, document::validate_parameter_range, ModelSetup, Parameter, Status,
};
use records::Records;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

#[derive(Debug, Clone)]
struct Content {
    id: String,
    setup: ModelSetup,
    assets: Records<AssetRecord>,
    parameters: Records<Parameter>,
    parts: Records<Part>,
    transforms: Records<Transform>,
    meshes: Records<Mesh>,
    bindings: Records<MeshBinding>,
    scene_bindings: Records<SceneBinding>,
    blend_key_tables: Records<BlendShapeKeyTable>,
    blend_constraints: Records<BlendShapeConstraint>,
    blend_bindings: Records<BlendShapeBinding>,
    glues: Records<Glue>,
    offscreens: Records<Offscreen>,
    expressions: Records<ExpressionAsset>,
    motions: Records<MotionClip>,
    draw_order_groups: Arc<Option<Vec<DrawOrderGroup>>>,
    display_info: Arc<DisplayInfo>,
    hierarchy_order: Arc<HierarchyOrder>,
    editor_state: Arc<EditorState>,
    motion_groups: Arc<Vec<MotionGroup>>,
    motion_roots: Arc<Vec<MotionRegistration>>,
    pose: Arc<Option<Arc<PoseAsset>>>,
    physics: Arc<Option<Arc<PhysicsAsset>>>,
    missing_attachments: Arc<Vec<String>>,
    model3_settings: Arc<Model3Settings>,
    package_attachments: Arc<Vec<PackageAttachment>>,
    // A recovery index travels with its content; readers never see a stale map.
    parameter_runtime_ids: Arc<HashMap<String, String>>,
}

/// Frozen persistent content. Clones only retain shared roots.
#[derive(Debug, Clone)]
pub struct DocumentState {
    content_id: ContentId,
    content: Arc<Content>,
}

impl DocumentState {
    pub fn new(id: impl Into<String>, setup: ModelSetup) -> Result<Self, Status> {
        let id = id.into();
        if !valid_uuid(&id) {
            return Err(Status::error(
                "INVALID_ID",
                "Document requires a canonical UUID",
            ));
        }
        if !setup.is_valid() {
            return Err(Status::error(
                "INVALID_MODEL_SETUP",
                "Invalid model dimensions or export settings",
            ));
        }
        Ok(Self {
            content_id: ContentId::new(),
            content: Arc::new(Content {
                id,
                setup,
                assets: Records::default(),
                parameters: Records::default(),
                parameter_runtime_ids: Arc::default(),
                parts: Records::default(),
                transforms: Records::default(),
                meshes: Records::default(),
                bindings: Records::default(),
                scene_bindings: Records::default(),
                blend_key_tables: Records::default(),
                blend_constraints: Records::default(),
                blend_bindings: Records::default(),
                glues: Records::default(),
                offscreens: Records::default(),
                expressions: Records::default(),
                motions: Records::default(),
                draw_order_groups: Arc::default(),
                display_info: Arc::default(),
                hierarchy_order: Arc::default(),
                editor_state: Arc::default(),
                motion_groups: Arc::default(),
                motion_roots: Arc::default(),
                pose: Arc::default(),
                physics: Arc::default(),
                missing_attachments: Arc::default(),
                model3_settings: Arc::default(),
                package_attachments: Arc::default(),
            }),
        })
    }

    pub fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub fn id(&self) -> &str {
        &self.content.id
    }
    pub fn setup(&self) -> ModelSetup {
        self.content.setup
    }
    pub fn asset(&self, id: &str) -> Option<&AssetRecord> {
        self.content.assets.get(id)
    }
    pub fn asset_order(&self) -> &[String] {
        self.content.assets.order()
    }
    pub fn parameter(&self, id: &str) -> Option<&Parameter> {
        self.content.parameters.get(id)
    }
    pub fn parameter_order(&self) -> &[String] {
        self.content.parameters.order()
    }
    pub fn parameter_by_runtime_id(&self, id: &str) -> Option<&Parameter> {
        self.parameter(self.content.parameter_runtime_ids.get(id)?)
    }
    /// Resource requirements only; Core never acquires files, pixels or leases.
    pub fn image_resources(&self) -> impl Iterator<Item = (ResourceKey, u32, u32)> + '_ {
        self.asset_order()
            .iter()
            .filter_map(|id| match &self.asset(id)?.content {
                AssetContent::Image {
                    resource,
                    width,
                    height,
                } => Some((*resource, *width, *height)),
                _ => None,
            })
    }
    pub fn builder(&self) -> DocumentBuilder {
        DocumentBuilder::new(self.clone())
    }
}

/// Exclusive candidate workspace. All writes are typed and automatically tracked.
/// A failed command poisons this workspace, so earlier candidate writes cannot
/// accidentally be frozen after a batch failure. SDK owns the Draft lifecycle.
#[derive(Debug)]
pub struct DocumentBuilder {
    base: DocumentState,
    content: Content,
    touched_assets: BTreeSet<String>,
    touched_parameters: BTreeSet<String>,
    setup_touched: bool,
    touched: BTreeMap<RecordDomain, BTreeSet<String>>,
    roots_touched: BTreeSet<RootDomain>,
    failure: Option<Status>,
}

/// Successful preparation, not publication. SDK pairs the model with resources.
#[derive(Debug)]
pub struct FrozenDocument {
    pub state: DocumentState,
    pub writes: WriteSet,
}

impl DocumentBuilder {
    fn new(base: DocumentState) -> Self {
        Self {
            content: (*base.content).clone(),
            base,
            touched_assets: BTreeSet::new(),
            touched_parameters: BTreeSet::new(),
            setup_touched: false,
            touched: BTreeMap::new(),
            roots_touched: BTreeSet::new(),
            failure: None,
        }
    }

    pub fn asset(&self, id: &str) -> Option<&AssetRecord> {
        self.content.assets.get(id)
    }
    pub fn parameter(&self, id: &str) -> Option<&Parameter> {
        self.content.parameters.get(id)
    }
    pub fn asset_order(&self) -> &[String] {
        self.content.assets.order()
    }
    pub fn parameter_order(&self) -> &[String] {
        self.content.parameters.order()
    }
    pub fn setup(&self) -> ModelSetup {
        self.content.setup
    }

    fn write(
        &mut self,
        command: impl FnOnce(&mut Self) -> Result<(), Status>,
    ) -> Result<(), Status> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let result = command(self);
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }

    fn check_new_id(&self, id: &str) -> Result<(), Status> {
        if !valid_uuid(id) {
            return Err(Status::error("INVALID_ID", id));
        }
        if self.content.contains_top_id(id) {
            return Err(Status::error("DUPLICATE_ID", id));
        }
        Ok(())
    }

    pub fn set_setup(&mut self, setup: ModelSetup) -> Result<(), Status> {
        self.write(|this| {
            if !setup.is_valid() {
                return Err(Status::error("INVALID_MODEL_SETUP", "Invalid model setup"));
            }
            this.setup_touched = true;
            this.content.setup = setup;
            Ok(())
        })
    }

    pub fn create_asset(&mut self, asset: AssetRecord) -> Result<(), Status> {
        self.write(|this| {
            this.check_new_id(&asset.id)?;
            validate_asset(&asset)?;
            this.touched_assets.insert(asset.id.clone());
            this.content.assets.insert(asset.id.clone(), asset);
            Ok(())
        })
    }

    pub fn replace_asset(&mut self, asset: AssetRecord) -> Result<(), Status> {
        self.write(|this| {
            if this.asset(&asset.id).is_none() {
                return Err(Status::error("MISSING_ASSET", &asset.id));
            }
            validate_asset(&asset)?;
            this.touched_assets.insert(asset.id.clone());
            this.content.assets.insert(asset.id.clone(), asset);
            Ok(())
        })
    }

    pub fn remove_asset(&mut self, id: &str) -> Result<(), Status> {
        self.write(|this| {
            if this.asset(id).is_none() {
                return Err(Status::error("MISSING_ASSET", id));
            }
            this.touched_assets.insert(id.into());
            this.content.assets.remove(id);
            Ok(())
        })
    }

    fn validate_parameter(&self, parameter: &Parameter) -> Result<(), Status> {
        if !valid_uuid(&parameter.id) || parameter.runtime_id.is_empty() {
            return Err(Status::error("INVALID_ID", &parameter.id));
        }
        if self
            .content
            .parameter_runtime_ids
            .get(&parameter.runtime_id)
            .is_some_and(|id| id != &parameter.id)
        {
            return Err(Status::error("DUPLICATE_RUNTIME_ID", &parameter.runtime_id));
        }
        validate_parameter_range(
            parameter.minimum,
            parameter.maximum,
            parameter.default_value,
        )
        .map_err(|error| Status::error("INVALID_PARAMETER", error.to_string()))?;
        if !(0..=9).contains(&parameter.decimal_places) {
            return Err(Status::error(
                "INVALID_PARAMETER",
                "decimal_places must be 0..9",
            ));
        }
        Ok(())
    }

    fn put_parameter(&mut self, parameter: Parameter) {
        let old = self.content.parameters.get(&parameter.id);
        if old.is_none_or(|old| old.runtime_id != parameter.runtime_id) {
            let index = Arc::make_mut(&mut self.content.parameter_runtime_ids);
            if let Some(old) = old {
                index.remove(&old.runtime_id);
            }
            index.insert(parameter.runtime_id.clone(), parameter.id.clone());
        }
        self.touched_parameters.insert(parameter.id.clone());
        self.content
            .parameters
            .insert(parameter.id.clone(), parameter);
    }

    pub fn create_parameter(&mut self, mut parameter: Parameter) -> Result<(), Status> {
        self.write(|this| {
            this.check_new_id(&parameter.id)?;
            if parameter.runtime_id.is_empty() {
                parameter.runtime_id = parameter.id.clone();
            }
            this.validate_parameter(&parameter)?;
            this.put_parameter(parameter);
            Ok(())
        })
    }

    pub fn replace_parameter(&mut self, parameter: Parameter) -> Result<(), Status> {
        self.write(|this| {
            if this.parameter(&parameter.id).is_none() {
                return Err(Status::error("MISSING_PARAMETER", &parameter.id));
            }
            this.validate_parameter(&parameter)?;
            this.put_parameter(parameter);
            Ok(())
        })
    }

    pub fn remove_parameter(&mut self, id: &str) -> Result<(), Status> {
        self.write(|this| {
            let old = this
                .parameter(id)
                .ok_or_else(|| Status::error("MISSING_PARAMETER", id))?;
            let runtime_id = old.runtime_id.clone();
            this.touched_parameters.insert(id.into());
            this.content.parameters.remove(id);
            Arc::make_mut(&mut this.content.parameter_runtime_ids).remove(&runtime_id);
            Ok(())
        })
    }

    /// Net writes compare only touched records; validation checks the whole graph.
    /// No transaction, saved baseline or legacy Document is needed to freeze.
    pub fn freeze(mut self) -> Result<FrozenDocument, Status> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.normalize_touched()?;
        self.content.validate()?;
        let mut writes = WriteSet::default();
        if self.setup_touched && self.base.content.setup != self.content.setup {
            writes.setup = true;
        }
        writes.assets_order = self.content.assets.finish(
            &self.base.content.assets,
            std::mem::take(&mut self.touched_assets),
            RecordDomain::Asset,
            &mut writes.records,
        );
        writes.parameters_order = self.content.parameters.finish(
            &self.base.content.parameters,
            std::mem::take(&mut self.touched_parameters),
            RecordDomain::Parameter,
            &mut writes.records,
        );
        self.finish_domains(&mut writes);
        if !Arc::ptr_eq(
            &self.content.parameter_runtime_ids,
            &self.base.content.parameter_runtime_ids,
        ) && self.content.parameter_runtime_ids == self.base.content.parameter_runtime_ids
        {
            self.content.parameter_runtime_ids = self.base.content.parameter_runtime_ids.clone();
        }
        writes.estimated_bytes = cost::estimate(&self.base.content, &self.content, &writes);
        let state = if writes.is_empty() {
            self.base
        } else {
            DocumentState {
                content_id: ContentId::new(),
                content: Arc::new(self.content),
            }
        };
        Ok(FrozenDocument { state, writes })
    }
}

fn validate_asset(asset: &AssetRecord) -> Result<(), Status> {
    let valid = match &asset.content {
        AssetContent::Empty => true,
        AssetContent::Unresolved { source } => !source.is_empty() && !source.contains('\0'),
        AssetContent::Image { width, height, .. } => *width > 0 && *height > 0,
    };
    if !valid {
        return Err(Status::error("INVALID_ASSET", &asset.id));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod domain_tests;
