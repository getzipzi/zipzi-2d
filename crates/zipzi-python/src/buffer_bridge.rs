//! Bounded owned buffer copies with admission before each allocation.
use crate::*;
#[pymethods]
impl NativeRuntime {
    fn buffer_create(&self, py: Python<'_>, length: usize) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let result = py.detach(|| self.host.call(move |rt| rt.create_blob(client, length)));
        reply(py, result, "buffer.create", |id| {
            Ok(Py::new(py, BlobToken { id })?.into_bound(py).into_any())
        })
    }
    fn buffer_append(
        &self,
        py: Python<'_>,
        blob: &BlobToken,
        offset: usize,
        bytes: &Bound<'_, PyBytes>,
    ) -> PyResult<Py<PyDict>> {
        let length = bytes.as_bytes().len();
        if length > 1 << 20 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Buffer chunk limit",
            ));
        }
        let lease = py.detach(|| self.host.call(move |rt| rt.reserve_transfer(length)));
        let _lease = match lease {
            Ok(value) => value,
            Err(e) => return reply::<()>(py, Err(e), "buffer.append", |_| unreachable!()),
        };
        let bytes = bytes.as_bytes().to_vec();
        zipzi_runtime::metrics::copy_payload(length);
        let client = self.client;
        let blob = blob.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.append_blob(client, blob, offset, &bytes))
        });
        reply(py, result, "buffer.append", |offset| {
            Ok(offset.into_pyobject(py)?.into_any())
        })
    }
    fn buffer_seal(&self, py: Python<'_>, blob: &BlobToken) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let blob = blob.id;
        let result = py.detach(|| self.host.call(move |rt| rt.seal_blob(client, blob)));
        reply(py, result, "buffer.seal", |size| {
            Ok(size.into_pyobject(py)?.into_any())
        })
    }
    fn buffer_read(
        &self,
        py: Python<'_>,
        blob: &BlobToken,
        offset: usize,
        length: usize,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let blob = blob.id;
        let result = py.detach(|| {
            self.host.call(move |rt| {
                let data = rt.blob(client, blob)?;
                if offset
                    .checked_add(length)
                    .is_none_or(|end| end > data.len())
                {
                    return Err(RuntimeError::InvalidReference);
                }
                let lease = rt.reserve_transfer(length)?;
                Ok((data, lease))
            })
        });
        reply(py, result, "buffer.read", |(data, _lease)| {
            zipzi_runtime::metrics::copy_payload(length);
            Ok(PyBytes::new(py, &data[offset..offset + length]).into_any())
        })
    }
    fn prepare_image_buffer(
        &self,
        py: Python<'_>,
        width: u32,
        height: u32,
        blob: &BlobToken,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let blob = blob.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.prepare_image_blob(client, width, height, blob))
        });
        reply(py, result, "resources.prepare_buffer", |value| {
            Ok(generated::resources::write_preparedimageinfo(py, value)?.into_any())
        })
    }
}

#[pymethods]
impl NativeRuntime {
    fn prepare_image_job(
        &self,
        py: Python<'_>,
        path: Option<&str>,
        blob: Option<&BlobToken>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            path.map_or(0, |p| p.len().saturating_mul(4)),
            "resources.load",
            None
        );
        let path = path.map(std::path::PathBuf::from);
        let blob = blob.map(|b| b.id);
        let client = self.client;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.request_image_preparation(client, path, blob))
        });
        reply(py, result, "resources.load", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
    fn capture_image(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        id: &str,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            id.len().saturating_mul(4),
            "resources.snapshot",
            None
        );
        let id = id.to_owned();
        let session = session.id;
        let edit = edit.map(|e| e.id);
        let client = self.client;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.capture_image(client, session, edit, &id))
        });
        reply(py, result, "resources.snapshot", |(image, origin)| {
            let value = generated::resources::write_preparedimageinfo(py, image)?;
            value.set_item("source", source(py, origin)?)?;
            Ok(value.into_any())
        })
    }
}
