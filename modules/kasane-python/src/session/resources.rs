use super::*;
#[pyclass]
pub(crate) struct NativeResourcePlan {
    inner: kasane_sdk::ResourceEditPlan,
}
#[pymethods]
impl NativeResourcePlan {
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
pub(crate) struct NativeDocumentPlan {
    inner: kasane_sdk::DocumentEditPlan,
}
#[pymethods]
impl NativeDocumentPlan {
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
    fn losses(&self) -> Vec<String> {
        self.inner.losses().to_vec()
    }
}
#[pyclass]
pub(crate) struct NativeAttachmentPlan {
    inner: kasane_sdk::AttachmentEditPlan,
}
#[pymethods]
impl NativeAttachmentPlan {
    #[getter]
    fn version(&self) -> (u64, u64, u64) {
        version_tuple(self.inner.version())
    }
}
#[pyclass]
pub(crate) struct NativeResourceNameAddress {
    inner: kasane_sdk::ResourceNameAddress,
}
#[pyclass]
pub(crate) struct NativeDocumentFieldAddress {
    inner: kasane_sdk::DocumentFieldAddress,
}
#[pyclass]
pub(crate) struct NativeImageLoadToken {
    inner: kasane_sdk::ImageLoadToken,
}
#[pymethods]
impl NativeImageLoadToken {
    fn cancel(&self) {
        self.inner.cancel()
    }
}
#[pyclass]
pub(crate) struct NativeAttachmentLoadToken {
    inner: kasane_sdk::AttachmentLoadToken,
}
#[pymethods]
impl NativeAttachmentLoadToken {
    fn cancel(&self) {
        self.inner.cancel()
    }
}
#[pyclass]
pub(crate) struct NativeAttachmentPathAddress {
    inner: kasane_sdk::AttachmentPathAddress,
}
#[pyclass]
pub(crate) struct NativePreparedImageContent {
    inner: kasane_sdk::PreparedImageContent,
}
#[pymethods]
impl NativePreparedImageContent {
    #[getter]
    fn width(&self) -> u32 {
        self.inner.content().width
    }
    #[getter]
    fn height(&self) -> u32 {
        self.inner.content().height
    }
}
fn parse_resource<T: serde::de::DeserializeOwned>(s: &str) -> PyResult<T> {
    serde_json::from_str(s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
#[pymethods]
impl NativeSession {
    fn plan_resource_authoring_json(
        &self,
        py: Python<'_>,
        text: &str,
    ) -> PyResult<NativeResourcePlan> {
        let intent = parse_resource(text)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeResourcePlan {
            inner: session
                .plan_resource_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_resource_plan(
        &self,
        py: Python<'_>,
        plan: &NativeResourcePlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_resource_plan", |session| {
            session.execute_resource_plan(&plan.inner)
        })
    }
    fn plan_document_authoring_json(
        &self,
        py: Python<'_>,
        text: &str,
    ) -> PyResult<NativeDocumentPlan> {
        let intent = parse_resource(text)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeDocumentPlan {
            inner: session
                .plan_document_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_document_plan(
        &self,
        py: Python<'_>,
        plan: &NativeDocumentPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_document_plan", |session| {
            session.execute_document_plan(&plan.inner)
        })
    }
    fn plan_attachment_authoring_json(
        &self,
        py: Python<'_>,
        text: &str,
    ) -> PyResult<NativeAttachmentPlan> {
        let intent = parse_resource(text)?;
        let session = self.inner.lock().map_err(|_| poisoned())?;
        Ok(NativeAttachmentPlan {
            inner: session
                .plan_attachment_edit(intent)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn execute_attachment_plan(
        &self,
        py: Python<'_>,
        plan: &NativeAttachmentPlan,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "execute_attachment_plan", |session| {
            session.execute_attachment_plan(&plan.inner)
        })
    }
    fn image_load_token(&self, py: Python<'_>, id: &str) -> PyResult<NativeImageLoadToken> {
        self.ensure_idle(py, "image_load_token")?;
        Ok(NativeImageLoadToken {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .image_load_token(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn attachment_load_token(
        &self,
        py: Python<'_>,
        id: &str,
    ) -> PyResult<NativeAttachmentLoadToken> {
        self.ensure_idle(py, "attachment_load_token")?;
        Ok(NativeAttachmentLoadToken {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .attachment_load_token(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn replace_image_content_json(
        &self,
        py: Python<'_>,
        token: &NativeImageLoadToken,
        text: &str,
    ) -> PyResult<(u64, u64, u64)> {
        let content = parse_resource(text)?;
        self.execute_authoring(py, "replace_image_content", |session| {
            session.replace_image_content(&token.inner, content)
        })
    }
    fn replace_attachment_content(
        &self,
        py: Python<'_>,
        token: &NativeAttachmentLoadToken,
        bytes: Option<Vec<u8>>,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "replace_attachment_content", |session| {
            session.replace_attachment_content(&token.inner, bytes.map(Arc::new))
        })
    }
    fn prepare_image_content_json(
        &self,
        py: Python<'_>,
        id: String,
        path: String,
    ) -> PyResult<String> {
        let result = py.detach(move || kasane_sdk::prepare_png_asset(&id, "", Path::new(&path)));
        let asset = result.map_err(|e| sdk_failure(py, e))?;
        Ok(serde_json::to_string(&asset.content).unwrap())
    }
    fn resource_collection(&self, py: Python<'_>) -> PyResult<Vec<NativeHandle>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .resource_collection()
            .map(|v| v.into_iter().map(|inner| NativeHandle { inner }).collect())
            .map_err(|e| sdk_failure(py, e))
    }
    fn resource_capabilities_json(&self, py: Python<'_>, h: &NativeHandle) -> PyResult<String> {
        Ok(serde_json::to_string(
            &self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .resource_capabilities(&h.inner)
                .map_err(|e| sdk_failure(py, e))?,
        )
        .unwrap())
    }
    fn attachment_records_json(&self) -> PyResult<String> {
        Ok(serde_json::to_string(
            &self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .package_attachments(),
        )
        .unwrap())
    }
    fn resource_name_address(
        &self,
        py: Python<'_>,
        id: &str,
    ) -> PyResult<NativeResourceNameAddress> {
        Ok(NativeResourceNameAddress {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .resource_name_address(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_resource_name_patch(
        &self,
        py: Python<'_>,
        a: &NativeResourceNameAddress,
        name: String,
    ) -> PyResult<NativeResourcePlan> {
        Ok(NativeResourcePlan {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .plan_resource_name_patch(&a.inner, name)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn attachment_path_address(
        &self,
        py: Python<'_>,
        id: &str,
    ) -> PyResult<NativeAttachmentPathAddress> {
        Ok(NativeAttachmentPathAddress {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .attachment_path_address(id)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_attachment_path_patch(
        &self,
        py: Python<'_>,
        a: &NativeAttachmentPathAddress,
        path: String,
    ) -> PyResult<NativeAttachmentPlan> {
        Ok(NativeAttachmentPlan {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .plan_attachment_path_patch(&a.inner, path)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn document_field_address_json(
        &self,
        py: Python<'_>,
        text: &str,
    ) -> PyResult<NativeDocumentFieldAddress> {
        let field = parse_resource(text)?;
        Ok(NativeDocumentFieldAddress {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .document_field_address(field)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn plan_document_field_patch_json(
        &self,
        py: Python<'_>,
        a: &NativeDocumentFieldAddress,
        text: &str,
    ) -> PyResult<NativeDocumentPlan> {
        let value = parse_resource(text)?;
        Ok(NativeDocumentPlan {
            inner: self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .plan_document_field_patch(&a.inner, value)
                .map_err(|e| sdk_failure(py, e))?,
        })
    }
    fn document_field_capability_json(&self, text: &str) -> PyResult<String> {
        let field = parse_resource(text)?;
        Ok(serde_json::to_string(
            &self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .document_field_capability(field),
        )
        .unwrap())
    }
    fn attachment_collection(&self, py: Python<'_>) -> PyResult<Vec<NativeHandle>> {
        self.inner
            .lock()
            .map_err(|_| poisoned())?
            .attachment_collection()
            .map(|v| v.into_iter().map(|inner| NativeHandle { inner }).collect())
            .map_err(|e| sdk_failure(py, e))
    }
    fn attachment_capabilities_json(&self, py: Python<'_>, h: &NativeHandle) -> PyResult<String> {
        let s = self.inner.lock().map_err(|_| poisoned())?;
        let c = s
            .attachment_capabilities(&h.inner)
            .map_err(|e| sdk_failure(py, e))?;
        Ok(serde_json::json!({"inspect":c.inspect,"patch":c.patch,"duplicate":c.duplicate,"delete":c.delete,"load_content":c.load_content}).to_string())
    }
    fn resource_collection_capabilities_json(&self) -> PyResult<String> {
        Ok(serde_json::to_string(
            &self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .resource_collection_capabilities(),
        )
        .unwrap())
    }
    fn attachment_collection_capabilities_json(&self) -> PyResult<String> {
        Ok(serde_json::to_string(
            &self
                .inner
                .lock()
                .map_err(|_| poisoned())?
                .attachment_collection_capabilities(),
        )
        .unwrap())
    }
    fn prepare_image_load(
        &self,
        py: Python<'_>,
        path: String,
    ) -> PyResult<NativePreparedImageContent> {
        let image = py
            .detach(move || kasane_sdk::prepare_image_load(Path::new(&path)))
            .map_err(|e| sdk_failure(py, e))?;
        Ok(NativePreparedImageContent { inner: image })
    }
    fn replace_prepared_image_content(
        &self,
        py: Python<'_>,
        token: &NativeImageLoadToken,
        image: &NativePreparedImageContent,
    ) -> PyResult<(u64, u64, u64)> {
        self.execute_authoring(py, "replace_prepared_image_content", |session| {
            session.replace_prepared_image_content(&token.inner, &image.inner)
        })
    }
}
