//! Python-facing authoring session and read/IO operations.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::conversion::*;
use crate::edit::NativeEdit;
use crate::error::{edit_failure, poisoned, sdk_failure};
use crate::handle::NativeHandle;
use pyo3::prelude::*;
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{
    AuthoringSession, GeometryBounds, GeometryChecks, GeometryDiagnosticKind, GeometrySpace,
    HistoryLimits, MeshProperties, SdkError,
};

#[pyclass]
pub(crate) struct NativeSession {
    pub(crate) inner: Arc<Mutex<AuthoringSession>>,
    active_edit: Arc<AtomicBool>,
}

impl NativeSession {
    /// Keep both checks: an edit may start while this call waits for the lock.
    fn execute_authoring(
        &self,
        py: Python<'_>,
        operation: &str,
        execute: impl FnOnce(&mut AuthoringSession) -> Result<zipzi_sdk::EditReceipt, SdkError>,
    ) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, operation)?;
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, operation)?;
        execute(&mut session)
            .map(|receipt| version_tuple(receipt.after))
            .map_err(|error| sdk_failure(py, error))
    }

    fn ensure_idle(&self, py: Python<'_>, operation: &str) -> PyResult<()> {
        if self.active_edit.load(Ordering::Acquire) {
            Err(edit_failure(
                py,
                "EDIT_ACTIVE",
                operation,
                "An edit is active",
            ))
        } else {
            Ok(())
        }
    }
}

fn active_edit_error(operation: &'static str) -> SdkError {
    SdkError {
        code: "EDIT_ACTIVE".into(),
        message: "An edit is active".into(),
        operation,
        object_ids: Vec::new(),
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    }
}

mod cdi;
mod diagnostics;
mod editing;
mod evaluation;
mod expression;
mod ids;
mod lifecycle;
mod model3;
mod motion;
mod objects;
mod physics;
mod pose;
mod project_io;

pub(crate) use motion::NativeMotionPlan;
pub(crate) use pose::NativePosePlan;

mod binding;
pub(crate) use binding::{NativeBindingFieldAddress, NativeBindingPlan, NativeConversionControl};

mod organization;
pub(crate) use organization::NativeOrganizationPlan;

mod scene_authoring;
pub(crate) use scene_authoring::{
    NativeDrawingGroupAddress, NativeSceneFieldAddress, NativeScenePlan,
};

mod scene_content;
pub(crate) use scene_content::{NativeSceneContentAddress, NativeSceneContentPlan};

mod reference_authoring;
pub(crate) use reference_authoring::{
    NativeReferenceFieldAddress, NativeReferencePlan, NativeReferenceSelectionAddress,
};

mod resources;
pub(crate) use resources::{
    NativeAttachmentLoadToken, NativeAttachmentPathAddress, NativeAttachmentPlan,
    NativeDocumentFieldAddress, NativeDocumentPlan, NativeImageLoadToken,
    NativePreparedImageContent, NativeResourceNameAddress, NativeResourcePlan,
};
