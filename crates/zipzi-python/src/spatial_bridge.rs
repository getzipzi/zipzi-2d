//! Owned request projection; spatial algorithms stay in the shared SDK.
use crate::*;

pub(crate) fn read_queries(value: &Bound<'_, PyAny>) -> PyResult<Vec<zipzi_sdk::SpatialQuery>> {
    value
        .try_iter()?
        .map(|q| generated::modeling::read_spatialquery(&q?))
        .collect()
}
pub(crate) fn measure_queries(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    if value.len()? > 64 {
        return Ok(usize::MAX);
    }
    value.try_iter()?.try_fold(1024usize, |n, q| {
        Ok(n.saturating_add(generated::modeling::measure_spatialquery(&q?)?))
    })
}
pub(crate) fn samples<'py>(
    py: Python<'py>,
    value: &zipzi_runtime::spatial::SpatialSampleBatch,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    d.set_item(
        "samples",
        generated::modeling::write_spatialsamples(py, &value.samples)?,
    )?;
    Ok(d)
}
#[pymethods]
impl NativeRuntime {
    fn spatial_job(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: &Bound<'_, PyAny>,
        queries: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _lease = input_lease!(
            self,
            py,
            input_budget::poses(poses)?.saturating_add(measure_queries(queries)?),
            "spatial",
            None
        );
        let poses = poses.extract()?;
        let queries = read_queries(queries)?;
        let expected = expected.map(modeling_bridge::read_source).transpose()?;
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let result = py.detach(|| {
            self.host.call(move |rt| {
                rt.request_spatial_samples(client, session, edit, poses, queries, expected)
            })
        });
        reply(py, result, "spatial", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
}
