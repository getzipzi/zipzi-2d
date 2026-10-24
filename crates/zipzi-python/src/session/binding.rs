use super::*;
/// Independent cancellation remains callable while a planning worker owns the session lock.
#[pyclass]
pub(crate) struct NativeConversionControl {
    pub(super) inner: zipzi_sdk::ConversionControl,
}
#[pymethods]
impl NativeConversionControl {
    fn cancel(&self) {
        self.inner.cancel();
    }
    #[getter]
    fn cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    #[getter]
    fn checkpoints(&self) -> u64 {
        self.inner.checkpoints()
    }
}

#[pyclass]
pub(crate) struct NativeBindingPlan {
    inner: zipzi_sdk::BindingEditPlan,
}
#[pymethods]
impl NativeBindingPlan {
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
    #[getter]
    fn affected_ids(&self) -> Vec<String> {
        self.inner.affected_ids()
    }
    #[getter]
    fn losses(&self) -> Vec<String> {
        self.inner.losses().to_vec()
    }
    #[getter]
    fn report_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.report())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
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
#[pymethods]
impl NativeSession {
    fn binding_conversion_control(&self) -> NativeConversionControl {
        NativeConversionControl {
            inner: Default::default(),
        }
    }
    #[pyo3(signature=(value, control=None))]
    fn plan_binding_authoring_json(
        &self,
        py: Python<'_>,
        value: &str,
        control: Option<&NativeConversionControl>,
    ) -> PyResult<NativeBindingPlan> {
        let intent = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let control =
            control.map_or_else(zipzi_sdk::ConversionControl::default, |c| c.inner.clone());
        let session = self.inner.clone();
        let result = py.detach(move || {
            let s = session.lock().map_err(|_| ())?;
            Ok::<_, ()>(s.plan_binding_authoring_with_control(intent, &control))
        });
        match result {
            Ok(Ok(inner)) => Ok(NativeBindingPlan { inner }),
            Ok(Err(e)) => Err(sdk_failure(py, e)),
            Err(()) => Err(poisoned()),
        }
    }
    fn reparent_support_json(&self, py: Python<'_>, value: &str) -> PyResult<String> {
        let edit = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        let support = s.reparent_support(&edit).map_err(|e| sdk_failure(py, e))?;
        serde_json::to_string(&support)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn execute_binding_plan(
        &self,
        py: Python<'_>,
        plan: &NativeBindingPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.ensure_idle(py, "execute_binding_plan")?;
        let mut s = self.inner.lock().map_err(|_| poisoned())?;
        self.ensure_idle(py, "execute_binding_plan")?;
        s.execute_binding_plan(&plan.inner)
            .map(|r| version_tuple(r.after))
            .map_err(|e| sdk_failure(py, e))
    }
}

/// Opaque context address; SDK epochs never rewind with Undo.
#[pyclass]
pub(crate) struct NativeBindingFieldAddress {
    inner: zipzi_sdk::BindingFieldAddress,
}
#[pymethods]
impl NativeBindingFieldAddress {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id()
    }
    #[getter]
    fn owner_id(&self) -> &str {
        self.inner.owner_id()
    }
}
#[pymethods]
impl NativeSession {
    fn binding_field_address(
        &self,
        py: Python<'_>,
        id: &str,
    ) -> PyResult<NativeBindingFieldAddress> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeBindingFieldAddress {
            inner: s
                .binding_field_address(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_binding_field_patch_json(
        &self,
        py: Python<'_>,
        address: &NativeBindingFieldAddress,
        value: &str,
    ) -> PyResult<NativeBindingPlan> {
        let patch = serde_json::from_str(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeBindingPlan {
            inner: s
                .plan_binding_field_patch(&address.inner, patch)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn binding_source_json(&self, id: &str) -> PyResult<Option<String>> {
        use serde_json::json;
        let s = self.inner.lock().map_err(|_| poisoned())?;
        let d = s.document();
        let source = if let Some(b) = d.get_binding(id) {
            json!({"kind":"mesh", "value":b})
        } else if let Some(b) = d.get_scene_binding(id) {
            json!({"kind":"scene", "value":b})
        } else if let Some(g) = d.get_glue(id) {
            json!({"kind":"glue", "value":g})
        } else if let Some(t) = d.get_blend_key_table(id) {
            json!({"kind":"table", "value":t})
        } else if let Some(c) = d.get_blend_constraint(id) {
            json!({"kind":"constraint", "value":c})
        } else if let Some(b) = d.get_blend_binding(id) {
            json!({"kind":"blend", "value":b})
        } else {
            return Ok(None);
        };
        Ok(Some(
            json!({"source":source,"version":version_tuple(s.version())}).to_string(),
        ))
    }
}
