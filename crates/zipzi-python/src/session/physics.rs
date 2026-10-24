use super::*;
use crate::animation::NativePhysicsPreview;

#[pymethods]
impl NativeSession {
    fn physics_preview(&self, py: Python<'_>) -> PyResult<NativePhysicsPreview> {
        self.ensure_idle(py, "physics_preview")?;
        Ok(NativePhysicsPreview::new(
            self.inner.lock().map_err(|_| poisoned())?.physics_preview(),
        ))
    }
    fn missing_attachments(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .missing_attachments())
    }
    fn physics_json(&self) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .physics()
            .map(|asset| {
                serde_json::to_string(&asset)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn export_physics3(&self, py: Python<'_>) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_physics3()
            .map_err(|error| sdk_failure(py, error))
    }
}

#[pyclass]
pub(crate) struct NativePhysicsPlan {
    inner: zipzi_sdk::PhysicsEditPlan,
}
#[pymethods]
impl NativePhysicsPlan {
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
    #[getter]
    fn affected_ids(&self) -> Vec<String> {
        self.inner.affected_ids().to_vec()
    }
}
#[pymethods]
impl NativeSession {
    fn plan_physics_authoring_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<NativePhysicsPlan> {
        let intent = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativePhysicsPlan {
            inner: s
                .plan_physics_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_physics_plan(
        &self,
        py: Python<'_>,
        plan: &NativePhysicsPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "execute_physics_plan")?;
        let mut s = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "execute_physics_plan")?;
        s.execute_physics_plan(&plan.inner)
            .map(|r| version_tuple(r.after))
            .map_err(|e| sdk_failure(py, e))
    }
}
