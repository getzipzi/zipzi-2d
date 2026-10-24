use super::*;
#[pyclass]
pub(crate) struct NativeReferencePlan {
    inner: zipzi_sdk::ReferenceEditPlan,
}
#[pymethods]
impl NativeReferencePlan {
    #[getter]
    fn effects_json(&self) -> String {
        serde_json::to_string(self.inner.effects()).unwrap()
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
}
#[pyclass]
pub(crate) struct NativeReferenceFieldAddress {
    inner: zipzi_sdk::ReferenceFieldAddress,
}
#[pymethods]
impl NativeReferenceFieldAddress {
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
pub(crate) struct NativeReferenceSelectionAddress {
    inner: zipzi_sdk::ReferenceSelectionAddress,
}
#[pymethods]
impl NativeReferenceSelectionAddress {
    #[getter]
    fn owner_id(&self) -> &str {
        self.inner.owner_id()
    }
    #[getter]
    fn expected(&self) -> Vec<String> {
        self.inner.expected().to_vec()
    }
}
fn input<T: serde::de::DeserializeOwned>(s: &str) -> PyResult<T> {
    serde_json::from_str(s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
#[pymethods]
impl NativeSession {
    fn plan_reference_authoring_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<NativeReferencePlan> {
        let intent = input(value)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeReferencePlan {
            inner: s
                .plan_reference_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_reference_plan(
        &self,
        py: Python<'_>,
        p: &NativeReferencePlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_reference_plan", |session| {
            session.execute_reference_plan(&p.inner)
        })
    }
    fn reference_collection_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<Vec<NativeHandle>> {
        let c = input(value)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        s.reference_collection(&c)
            .map(|h| h.into_iter().map(|inner| NativeHandle { inner }).collect())
            .map_err(|e| sdk_failure(py, e))
    }
    fn reference_capabilities_json(&self, py: Python<'_>, h: &NativeHandle) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(
            &s.reference_capabilities(&h.inner)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn reference_collection_capabilities_json(
        &self,
        py: Python<'_>,
        value: &str,
    ) -> PyResult<String> {
        let c = input(value)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(
            &s.reference_collection_capabilities(&c)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn reference_projection_json(&self) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(serde_json::to_string(s.reference_projection().as_ref()).unwrap())
    }
    fn reference_field_address(
        &self,
        py: Python<'_>,
        id: &str,
    ) -> PyResult<NativeReferenceFieldAddress> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeReferenceFieldAddress {
            inner: s
                .reference_field_address(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_reference_field_patch_json(
        &self,
        py: Python<'_>,
        a: &NativeReferenceFieldAddress,
        value: &str,
    ) -> PyResult<NativeReferencePlan> {
        let f = input(value)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeReferencePlan {
            inner: s
                .plan_reference_field_patch(&a.inner, f)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn reference_selection_address_json(
        &self,
        py: Python<'_>,
        owner: &str,
        field: &str,
    ) -> PyResult<NativeReferenceSelectionAddress> {
        let f = input(field)?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeReferenceSelectionAddress {
            inner: s
                .reference_selection_address(owner, f)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_reference_selection_patch(
        &self,
        py: Python<'_>,
        a: &NativeReferenceSelectionAddress,
        targets: Vec<String>,
    ) -> PyResult<NativeReferencePlan> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeReferencePlan {
            inner: s
                .plan_reference_selection_patch(&a.inner, targets)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
}
