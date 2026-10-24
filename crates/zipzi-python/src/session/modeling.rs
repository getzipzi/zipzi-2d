use super::*;

#[pymethods]
impl NativeSession {
    fn modeling_query_json(&self, query: &str) -> PyResult<String> {
        let query: zipzi_sdk::ModelingQuery = serde_json::from_str(query)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        serde_json::to_string(&serde_json::json!({"source": {"kind": "formal", "version": session.version()}, "version": session.version(), "state": "ready", "records": zipzi_sdk::modeling_records(session.document(), &query)}))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn sample_modeling_json(
        &self,
        py: Python<'_>,
        parameters: &str,
        checks: &str,
    ) -> PyResult<String> {
        let parameters = serde_json::from_str(parameters)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let checks = serde_json::from_str(checks)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let snapshot = self.inner.lock().map_err(|_| poisoned())?.read_snapshot();
        let result = py
            .detach(move || snapshot.sample_modeling(parameters, checks))
            .map_err(|e| sdk_failure(py, e))?;
        serde_json::to_string(&serde_json::json!({"source": {"kind": "formal", "version": result.version}, "state": "ready", "samples": result}))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

#[pyclass]
pub(crate) struct NativeModelingPlan {
    pub(crate) plan: Option<(zipzi_sdk::ModelingPlan, zipzi_sdk::AuthoringSnapshot)>,
    pub(crate) edit_source: Option<(u64, u64)>,
}
#[pymethods]
impl NativeModelingPlan {
    fn report_json(&self) -> PyResult<String> {
        let (plan, _) = self
            .plan
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Plan is released"))?;
        serde_json::to_string(&plan.report())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn close(&mut self) {
        self.plan = None;
    }
}
#[pymethods]
impl NativeSession {
    fn plan_modeling_json(&self, py: Python<'_>, intent: &str) -> PyResult<NativeModelingPlan> {
        let intent = serde_json::from_str(intent)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        let snapshot = session.read_snapshot();
        let limits = session.authoring_limits();
        drop(session);
        let (plan, snapshot) = py
            .detach(move || {
                snapshot
                    .plan_modeling(intent, limits, &Default::default())
                    .map(|p| (p, snapshot))
            })
            .map_err(|e| sdk_failure(py, e))?;
        Ok(NativeModelingPlan {
            plan: Some((plan, snapshot)),
            edit_source: None,
        })
    }
}
