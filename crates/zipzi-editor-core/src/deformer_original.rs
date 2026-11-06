use crate::{Document, Editor, ObjectKind, ObjectRef, PropertyScope};
use anyhow::{Result, bail, ensure};
use zipzi_core::{DeformerOriginalAction, DeformerOriginalShape, DeformerOriginalSource};

#[derive(Clone, Debug, PartialEq)]
pub struct DeformerOriginalTarget {
    pub object: ObjectRef,
    session: zipzi_runtime::SessionId,
    version: zipzi_sdk::Version,
    preview_revision: u64,
    scope: PropertyScope,
    selection: Vec<ObjectRef>,
}
#[derive(Clone, Debug)]
pub struct DeformerOriginalInfo {
    pub source: Option<DeformerOriginalSource>,
    pub capture_error: Option<String>,
    /// Current and recorded [columns, rows].
    pub conversion_grid: Option<([u32; 2], [u32; 2])>,
    pub controller_grid: Option<([u32; 2], [u32; 2])>,
    pub interpolation_change: bool,
}
impl DeformerOriginalInfo {
    pub fn requires_conversion(&self) -> bool {
        self.conversion_grid.is_some()
            || self.controller_grid.is_some()
            || self.interpolation_change
    }
}
impl Document {
    pub fn deformer_original_target(&self, object: &ObjectRef) -> Option<DeformerOriginalTarget> {
        if self.has_pending_edit() {
            return None;
        }
        (object.kind == ObjectKind::Deformer).then_some(())?;
        self.model().get_transform(&object.id)?;
        Some(DeformerOriginalTarget {
            object: object.clone(),
            session: self.session.id,
            version: self.session.version(),
            preview_revision: self.preview_revision(),
            scope: self.property_scope(object),
            selection: self.context().selections.clone(),
        })
    }
    pub fn deformer_original_info(&self, object: &ObjectRef) -> Option<DeformerOriginalInfo> {
        let model = self.model();
        let t = model.get_transform(&object.id)?;
        let record = t.original_shape.as_ref();
        let snapshot = record.and_then(|r| r.snapshot.as_ref());
        let mut info = DeformerOriginalInfo {
            source: snapshot.map(|s| s.source),
            capture_error: record.and_then(|r| r.capture_error.clone()),
            conversion_grid: None,
            controller_grid: None,
            interpolation_change: false,
        };
        if let Some(DeformerOriginalShape::Warp { warp: saved, .. }) = snapshot.map(|s| &s.shape) {
            if let Some(current) = t.warp() {
                if (current.columns, current.rows) != (saved.columns, saved.rows) {
                    info.conversion_grid =
                        Some(([current.columns, current.rows], [saved.columns, saved.rows]));
                }
                info.interpolation_change = current.quad != saved.quad;
                if let Some(controller) = &saved.bezier {
                    if let Some(grid) = t
                        .warp_authoring
                        .as_ref()
                        .and_then(|s| s.bezier_grid)
                        .filter(|g| (g.columns, g.rows) != (controller.columns, controller.rows))
                    {
                        info.controller_grid = Some((
                            [grid.columns, grid.rows],
                            [controller.columns, controller.rows],
                        ));
                    }
                }
            }
        }
        Some(info)
    }
    pub(crate) fn apply_deformer_original(
        &mut self,
        target: &DeformerOriginalTarget,
        action: DeformerOriginalAction,
    ) -> Result<()> {
        self.finish_edit()?;
        ensure!(
            self.deformer_original_target(&target.object).as_ref() == Some(target),
            "Original shape target changed"
        );
        self.ensure_editable(std::slice::from_ref(&target.object))?;
        let shape = match (&target.scope, action) {
            (_, DeformerOriginalAction::AutoUpdate { .. }) | (PropertyScope::Object, _) => {
                zipzi_contract::document::ShapeTarget::Base
            }
            (PropertyScope::DeformerKeyform { binding, samples }, _) => {
                zipzi_contract::document::ShapeTarget::Ordinary {
                    binding_id: binding.clone(),
                    samples: samples.clone(),
                }
            }
            _ => bail!("Select a stored deformer shape before updating or restoring its original"),
        };
        let context = self
            .snapshot()?
            .viewport_frame
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.value))
            .collect();
        let plan = self.plan_scene_content(
            zipzi_contract::document::SceneContentEdit::DeformerOriginal {
                transform_id: target.object.id.clone(),
                target: shape,
                action,
                context,
            },
        )?;
        self.execute_scene_content(&plan)?;
        Ok(())
    }
}
impl Editor {
    pub fn deformer_original_target(&self, object: &ObjectRef) -> Option<DeformerOriginalTarget> {
        self.document()?.deformer_original_target(object)
    }
}
