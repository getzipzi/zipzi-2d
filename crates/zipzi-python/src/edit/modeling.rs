use super::*;

#[pymethods]
impl NativeEdit {
    fn apply_modeling_json(&mut self, py: Python<'_>, commands: &str) -> PyResult<()> {
        self.ensure_open(py, "apply_modeling")?;
        let commands: Vec<zipzi_runtime::modeling::ModelingCommand> =
            serde_json::from_str(commands).map_err(|e| {
                self.failed = true;
                PyValueError::new_err(e.to_string())
            })?;
        let edit = self.commands.workspace.as_mut().expect("edit is open");
        let result = commands
            .into_iter()
            .try_for_each(|command| command.apply(edit));
        if let Err(error) = result {
            self.commands.error = Some(error.clone());
            return Err(sdk_failure(py, error));
        }
        self.commands.sequence += 1;
        Ok(())
    }
    fn modeling_query_json(&self, py: Python<'_>, query: &str) -> PyResult<String> {
        self.ensure_open(py, "modeling_query")?;
        let query: zipzi_sdk::ModelingQuery =
            serde_json::from_str(query).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let workspace = self.commands.workspace.as_ref().expect("edit is open");
        serde_json::to_string(&serde_json::json!({"source": {"kind": "edit", "base": workspace.base_version(), "edit": self.modeling_identity, "sequence": self.commands.sequence}, "version": workspace.base_version(), "state": "ready", "records": zipzi_sdk::modeling_records(workspace.candidate_document(), &query)})).map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn sample_modeling_json(
        &self,
        py: Python<'_>,
        parameters: &str,
        checks: &str,
    ) -> PyResult<String> {
        self.ensure_open(py, "sample_modeling")?;
        let parameters =
            serde_json::from_str(parameters).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let checks =
            serde_json::from_str(checks).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let snapshot = self
            .session
            .lock()
            .map_err(|_| poisoned())?
            .candidate_snapshot(self.commands.workspace.as_ref().expect("edit is open"))
            .map_err(|e| sdk_failure(py, e))?;
        let result = py
            .detach(move || snapshot.sample_modeling(parameters, checks))
            .map_err(|e| sdk_failure(py, e))?;
        serde_json::to_string(&serde_json::json!({"source": {"kind": "edit", "base": result.version, "edit": self.modeling_identity, "sequence": self.commands.sequence}, "state": "ready", "samples": result}))
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn apply_rgba(
        &mut self,
        py: Python<'_>,
        id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        replace: bool,
    ) -> PyResult<()> {
        self.ensure_open(py, "apply_rgba")?;
        let budget = self
            .session
            .lock()
            .map_err(|_| poisoned())?
            .image_memory_budget()
            .clone();
        let image = zipzi_sdk::PreparedImageContent::from_rgba_bytes(width, height, rgba, &budget)
            .map_err(|s| {
                self.failed = true;
                edit_failure(py, &s.code, "apply_rgba", &s.message)
            })?;
        let edit = self.commands.workspace.as_mut().expect("edit is open");
        let result = if replace {
            edit.replace_prepared_image(&id, &image)
        } else {
            edit.create_prepared_image(id, name, &image)
        };
        if let Err(error) = result {
            self.commands.error = Some(error.clone());
            return Err(sdk_failure(py, error));
        }
        self.commands.sequence += 1;
        Ok(())
    }
}

#[pymethods]
impl NativeEdit {
    fn plan_modeling_json(
        &self,
        py: Python<'_>,
        intent: &str,
    ) -> PyResult<crate::session::NativeModelingPlan> {
        self.ensure_open(py, "plan_modeling")?;
        let intent =
            serde_json::from_str(intent).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let session = self.session.lock().map_err(|_| poisoned())?;
        let snapshot = session
            .candidate_snapshot(self.commands.workspace.as_ref().expect("edit is open"))
            .map_err(|e| sdk_failure(py, e))?;
        let limits = session.authoring_limits();
        drop(session);
        let (plan, snapshot) = py
            .detach(move || {
                snapshot
                    .plan_modeling(intent, limits, &Default::default())
                    .map(|p| (p, snapshot))
            })
            .map_err(|e| sdk_failure(py, e))?;
        Ok(crate::session::NativeModelingPlan {
            plan: Some((plan, snapshot)),
            edit_source: Some((self.modeling_identity, self.commands.sequence)),
        })
    }
    fn apply_modeling_plan(
        &mut self,
        py: Python<'_>,
        plan: &crate::session::NativeModelingPlan,
    ) -> PyResult<()> {
        self.ensure_open(py, "apply_modeling_plan")?;
        let (plan_value, _) = plan
            .plan
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("Plan is released"))?;
        let source_valid = match plan.edit_source {
            Some((edit, sequence)) => {
                edit == self.modeling_identity && sequence == self.commands.sequence
            }
            None => self.commands.sequence == 0,
        };
        if !source_valid {
            self.failed = true;
            return Err(edit_failure(
                py,
                "STALE_PLAN",
                "apply_modeling_plan",
                "Plan source no longer matches this candidate",
            ));
        }
        let result = plan_value.apply(self.commands.workspace.as_mut().expect("edit is open"));
        if let Err(error) = result {
            self.commands.error = Some(error.clone());
            return Err(sdk_failure(py, error));
        }
        self.commands.sequence += 1;
        Ok(())
    }
}
