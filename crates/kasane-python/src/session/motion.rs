use super::*;
use crate::animation::NativeMotionPreview;

#[pymethods]
impl NativeSession {
    fn motion_preview(&self, py: Python<'_>) -> PyResult<NativeMotionPreview> {
        self.ensure_idle(py, "motion_preview")?;
        Ok(NativeMotionPreview::new(
            self.inner.lock().map_err(|_| poisoned())?.motion_preview(),
        ))
    }
    fn motion_ids(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .motion_ids()
            .to_vec())
    }

    fn motion_json(&self, id: &str) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .motion(id)
            .map(|clip| {
                serde_json::to_string(&clip)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn motion_groups_json(&self) -> PyResult<String> {
        let groups = self.inner.lock().map_err(|_| poisoned())?.motion_groups();
        serde_json::to_string(&groups)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    fn export_motion3(&self, py: Python<'_>, id: &str) -> PyResult<String> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_motion3(id)
            .map_err(|error| sdk_failure(py, error))
    }
}

#[pyclass]
pub(crate) struct NativeMotionPlan {
    inner: kasane_sdk::MotionEditPlan,
}
#[pymethods]
impl NativeMotionPlan {
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
    #[getter]
    fn affected_ids(&self) -> Vec<String> {
        self.inner.affected_ids().to_vec()
    }
    #[getter]
    fn migrated_to_root(&self) -> Vec<String> {
        self.inner.migrated_to_root().to_vec()
    }
}
#[pymethods]
impl NativeSession {
    fn motion_roots_json(&self) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(s.document().motion_roots()).expect("typed roots"))
    }
    fn plan_motion_authoring_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<NativeMotionPlan> {
        let intent = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeMotionPlan {
            inner: s.plan_motion_edit(intent).map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_motion_plan(
        &self,
        py: Python<'_>,
        plan: &NativeMotionPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "execute_motion_plan")?;
        let mut s = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "execute_motion_plan")?;
        s.execute_motion_plan(&plan.inner)
            .map(|r| version_tuple(r.after))
            .map_err(|e| sdk_failure(py, e))
    }
}
