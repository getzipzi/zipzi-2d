use super::*;

#[pymethods]
impl NativeSession {
    fn package_attachments(&self) -> PyResult<Vec<(String, Option<Vec<u8>>)>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| poisoned())?
            .package_attachments()
            .into_iter()
            .map(|attachment| {
                (
                    attachment.path,
                    attachment.bytes.map(|b| b.as_ref().clone()),
                )
            })
            .collect())
    }
    fn model3_settings_json(&self) -> PyResult<String> {
        let settings = self.inner.lock().map_err(|_| poisoned())?.model3_settings();
        serde_json::to_string(&settings)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }
}
