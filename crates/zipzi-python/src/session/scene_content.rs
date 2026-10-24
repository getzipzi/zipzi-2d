use super::*;
#[pyclass]
pub(crate) struct NativeSceneContentPlan {
    inner: zipzi_sdk::SceneContentEditPlan,
}
#[pymethods]
impl NativeSceneContentPlan {
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
    fn report_json(&self) -> String {
        serde_json::to_string(self.inner.report()).unwrap()
    }
    #[getter]
    fn cost(&self) -> (usize, usize, usize, usize) {
        let c = self.inner.cost();
        (
            c.sample_combinations,
            c.payload_bytes,
            c.temporary_bytes,
            c.history_bytes,
        )
    }
}
#[pyclass]
pub(crate) struct NativeSceneContentAddress {
    inner: zipzi_sdk::SceneContentAddress,
}
#[pymethods]
impl NativeSceneContentAddress {
    #[getter]
    fn owner_id(&self) -> &str {
        self.inner.owner_id()
    }
    #[getter]
    fn target_json(&self) -> String {
        serde_json::to_string(self.inner.target()).unwrap()
    }
    #[getter]
    fn expected_json(&self) -> String {
        serde_json::to_string(self.inner.expected()).unwrap()
    }
}
fn input<T: serde::de::DeserializeOwned>(s: &str) -> PyResult<T> {
    serde_json::from_str(s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
#[pymethods]
impl NativeSession {
    #[pyo3(signature=(value, control=None))]
    fn plan_scene_content_json(
        &self,
        py: Python<'_>,
        value: &str,
        control: Option<&super::binding::NativeConversionControl>,
    ) -> PyResult<NativeSceneContentPlan> {
        let edit = input(value)?;
        let control = control.map(|c| c.inner.clone()).unwrap_or_default();
        let session = self.inner.clone();
        let result = py.detach(move || {
            let s = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(s.plan_scene_content_with_control(edit, &control))
        });
        match result {
            Ok(Ok(inner)) => Ok(NativeSceneContentPlan { inner }),
            Ok(Err(e)) => Err(sdk_failure(py, e)),
            Err(()) => Err(poisoned()),
        }
    }
    fn execute_scene_content_plan(
        &self,
        py: Python<'_>,
        plan: &NativeSceneContentPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_scene_content_plan", |session| {
            session.execute_scene_content_plan(&plan.inner)
        })
    }
    fn scene_content_address_json(
        &self,
        py: Python<'_>,
        owner: &str,
        target: &str,
    ) -> PyResult<NativeSceneContentAddress> {
        let t = input(target)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeSceneContentAddress {
            inner: s
                .scene_content_address(owner, t)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_scene_content_patch_json(
        &self,
        py: Python<'_>,
        address: &NativeSceneContentAddress,
        value: &str,
    ) -> PyResult<NativeSceneContentPlan> {
        let v = input(value)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeSceneContentPlan {
            inner: s
                .plan_scene_content_patch(&address.inner, v)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn scene_content_capabilities_json(
        &self,
        py: Python<'_>,
        handle: &NativeHandle,
    ) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(
            &s.scene_content_capabilities(&handle.inner)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn glue_pair_handles(&self, py: Python<'_>, owner: &str) -> PyResult<Vec<NativeHandle>> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        s.glue_pairs(owner)
            .map(|h| h.into_iter().map(|inner| NativeHandle { inner }).collect())
            .map_err(|e| sdk_failure(py, e))
    }
    fn glue_pairs_json(&self, py: Python<'_>, owner: &str) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        let g = s
            .document()
            .get_glue(owner)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Missing Glue"))?;
        let _ = py;
        Ok(serde_json::to_string(&g.pairs).unwrap())
    }
}
