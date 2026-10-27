//! Direct typed Preview bridge. Inputs are owned before releasing the GIL.
use crate::*;

pub(crate) fn samples<'py>(
    py: Python<'py>,
    value: &zipzi_runtime::preview::PreviewSampleBatch,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    d.set_item(
        "samples",
        generated::modeling::write_previewbatch(py, &value.samples)?,
    )?;
    Ok(d)
}
fn info<'py>(
    py: Python<'py>,
    value: (SourceRef, zipzi_runtime::preview::PreviewState),
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.0)?)?;
    d.set_item(
        "value",
        generated::modeling::write_previewstate(py, &value.1)?,
    )?;
    Ok(d)
}
#[pymethods]
impl NativeRuntime {
    fn create_preview(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let expected = expected.map(modeling_bridge::read_source).transpose()?;
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.create_preview(client, session, edit, expected))
        });
        reply(py, result, "preview.create", |(id, identity)| {
            let d = PyDict::new(py);
            d.set_item("preview", Py::new(py, PreviewToken { id })?)?;
            d.set_item("serial", id.serial())?;
            d.set_item("source", source(py, identity)?)?;
            Ok(d.into_any())
        })
    }
    fn preview_state(&self, py: Python<'_>, preview: &PreviewToken) -> PyResult<Py<PyDict>> {
        let (client, id) = (self.client, preview.id);
        let result = py.detach(|| self.host.call(move |rt| rt.preview_state(client, id)));
        reply(py, result, "preview.info", |value| {
            Ok(info(py, value)?.into_any())
        })
    }
    fn apply_preview(
        &self,
        py: Python<'_>,
        preview: &PreviewToken,
        command: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _lease = input_lease!(
            self,
            py,
            generated::modeling::measure_previewcommand(command)?,
            "preview.apply",
            None
        );
        let command = generated::modeling::read_previewcommand(command)?;
        let (client, id) = (self.client, preview.id);
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.apply_preview(client, id, command))
        });
        reply(py, result, "preview.apply", |value| {
            Ok(info(py, value)?.into_any())
        })
    }
    fn sample_preview(
        &self,
        py: Python<'_>,
        preview: &PreviewToken,
        times: &Bound<'_, PyAny>,
        geometry: bool,
        incremental: bool,
        queries: &Bound<'_, PyAny>,
        view: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _lease = input_lease!(
            self,
            py,
            times
                .len()?
                .saturating_mul(16)
                .saturating_add(spatial_bridge::measure_queries(queries)?),
            "preview.sample",
            None
        );
        let times = times.extract::<Vec<f32>>()?;
        let queries = spatial_bridge::read_queries(queries)?;
        let view = view
            .map(generated::modeling::read_observationview)
            .transpose()?;
        let (client, id) = (self.client, preview.id);
        let result = py.detach(|| {
            self.host.call(move |rt| {
                rt.request_preview_frames(client, id, times, geometry, incremental, queries, view)
            })
        });
        reply(py, result, "preview.sample", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
}
