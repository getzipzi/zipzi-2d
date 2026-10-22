use super::*;

#[pymethods]
impl NativeSession {
    fn pose_json(&self) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .pose()
            .map(|pose| {
                serde_json::to_string(&pose)
                    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
            })
            .transpose()
    }

    fn export_pose3(&self, py: Python<'_>) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .export_pose3()
            .map_err(|error| sdk_failure(py, error))
    }
}

#[pyclass]
pub(crate) struct NativePosePlan {
    inner: kasane_sdk::PoseEditPlan,
}
#[pymethods]
impl NativePosePlan {
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
    fn plan_pose_authoring_json(&self, py: Python<'_>, value: &str) -> PyResult<NativePosePlan> {
        let intent = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativePosePlan {
            inner: s.plan_pose_edit(intent).map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_pose_plan(
        &self,
        py: Python<'_>,
        plan: &NativePosePlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_pose_plan", |session| {
            session.execute_pose_plan(&plan.inner)
        })
    }
}
