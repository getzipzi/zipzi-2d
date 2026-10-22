use super::*;
#[pymethods]
impl NativeEdit {
    fn organization_authoring_json(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        self.ensure_open(py, "organization_authoring")?;
        let intent = serde_json::from_str(value).map_err(|e| {
            self.failed = true;
            PyValueError::new_err(e.to_string())
        })?;
        self.commands
            .push(Command::EditOrganization(intent))
            .map_err(|e| sdk_failure(py, e))
    }
}
