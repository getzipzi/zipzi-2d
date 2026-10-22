use super::*;
fn json_input<T: serde::de::DeserializeOwned>(value: &str) -> PyResult<T> {
    serde_json::from_str(value).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
#[pyclass]
pub(crate) struct NativeScenePlan {
    inner: kasane_sdk::SceneEditPlan,
}
#[pymethods]
impl NativeScenePlan {
    #[getter]
    fn cost(&self) -> Option<(usize, usize, usize, usize)> {
        self.inner.cost().map(|c| {
            (
                c.sample_combinations,
                c.payload_bytes,
                c.temporary_bytes,
                c.history_bytes,
            )
        })
    }
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
    #[getter]
    fn is_noop(&self) -> bool {
        self.inner.is_noop()
    }
    #[getter]
    fn affected_ids(&self) -> Vec<String> {
        self.inner.affected_ids().to_vec()
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
        serde_json::to_string(self.inner.effects()).unwrap()
    }
    #[getter]
    fn conversion_report_json(&self) -> Option<String> {
        self.inner
            .conversion_report()
            .map(|report| serde_json::to_string(report).unwrap())
    }
}
#[pyclass]
pub(crate) struct NativeSceneFieldAddress {
    inner: kasane_sdk::SceneFieldAddress,
}
#[pymethods]
impl NativeSceneFieldAddress {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }
    #[getter]
    fn expected_json(&self) -> String {
        serde_json::to_string(self.inner.expected()).unwrap()
    }
}
#[pyclass]
pub(crate) struct NativeDrawingGroupAddress {
    inner: kasane_sdk::DrawingGroupAddress,
}
#[pymethods]
impl NativeDrawingGroupAddress {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }
    #[getter]
    fn range(&self) -> (i32, i32) {
        let v = self.inner.range();
        (v[0], v[1])
    }
}
#[pymethods]
impl NativeSession {
    fn plan_scene_authoring_json(&self, py: Python<'_>, value: &str) -> PyResult<NativeScenePlan> {
        let intent = json_input(value)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeScenePlan {
            inner: session
                .plan_scene_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_scene_plan(
        &self,
        py: Python<'_>,
        plan: &NativeScenePlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "execute_scene_plan")?;
        let mut session = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "execute_scene_plan")?;
        session
            .execute_scene_plan(&plan.inner)
            .map(|r| version_tuple(r.after))
            .map_err(|e| sdk_failure(py, e))
    }
    fn scene_projection_json(&self) -> PyResult<String> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(session.scene_projection().as_ref()).unwrap())
    }
    fn scene_collection_json(&self, py: Python<'_>, value: &str) -> PyResult<Vec<NativeHandle>> {
        let c = json_input(value)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        session
            .scene_collection(&c)
            .map(|handles| {
                handles
                    .into_iter()
                    .map(|inner| NativeHandle { inner })
                    .collect()
            })
            .map_err(|e| sdk_failure(py, e))
    }
    fn scene_capabilities_json(&self, py: Python<'_>, handle: &NativeHandle) -> PyResult<String> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(
            &session
                .scene_capabilities(&handle.inner)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn scene_collection_capabilities_json(&self, py: Python<'_>, value: &str) -> PyResult<String> {
        let c = json_input(value)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(
            &session
                .scene_collection_capabilities(&c)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn scene_field_address_json(
        &self,
        py: Python<'_>,
        id: &str,
        field: &str,
    ) -> PyResult<NativeSceneFieldAddress> {
        let field = json_input(field)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeSceneFieldAddress {
            inner: session
                .scene_field_address(id, field)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_scene_field_patch_json(
        &self,
        py: Python<'_>,
        address: &NativeSceneFieldAddress,
        value: &str,
        clear_pairs: bool,
    ) -> PyResult<NativeScenePlan> {
        let value = json_input(value)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeScenePlan {
            inner: session
                .plan_scene_field_patch(&address.inner, value, clear_pairs)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn drawing_group_address(
        &self,
        py: Python<'_>,
        owner: &str,
    ) -> PyResult<NativeDrawingGroupAddress> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeDrawingGroupAddress {
            inner: session
                .drawing_group_address(owner)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_drawing_range_patch(
        &self,
        py: Python<'_>,
        address: &NativeDrawingGroupAddress,
        range: (i32, i32),
    ) -> PyResult<NativeScenePlan> {
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeScenePlan {
            inner: session
                .plan_drawing_range_patch(&address.inner, [range.0, range.1])
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
}
