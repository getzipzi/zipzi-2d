use super::*;
#[pyclass]
pub(crate) struct NativeOrganizationPlan {
    inner: kasane_sdk::OrganizationEditPlan,
}
#[pymethods]
impl NativeOrganizationPlan {
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
    #[getter]
    fn copied_ids(&self) -> Vec<(String, String)> {
        self.inner
            .copied_ids()
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect()
    }
    #[getter]
    fn effects_json(&self) -> String {
        serde_json::to_string(self.inner.effects()).expect("typed effects serialize")
    }
    #[getter]
    fn affected_ids(&self) -> Vec<String> {
        self.inner.affected_ids().to_vec()
    }
}
#[pymethods]
impl NativeSession {
    fn plan_organization_authoring_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<NativeOrganizationPlan> {
        let intent = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeOrganizationPlan {
            inner: s
                .plan_organization_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_organization_plan(
        &self,
        py: Python<'_>,
        plan: &NativeOrganizationPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_organization_plan", |session| {
            session.execute_organization_plan(&plan.inner)
        })
    }
}
