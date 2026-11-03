//! Direct finite structures; no Python JSON or image encoding.
use crate::*;
pub(crate) fn samples<'py>(
    py: Python<'py>,
    value: &zipzi_runtime::observation::ObservationSampleBatch,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    d.set_item(
        "samples",
        generated::modeling::write_observationbatch(py, &value.samples)?,
    )?;
    Ok(d)
}
#[pymethods]
impl NativeRuntime {
    fn estimate_observation(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: &Bound<'_, PyAny>,
        view: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _lease = input_lease!(
            self,
            py,
            input_budget::poses(poses)?
                .saturating_add(generated::modeling::measure_observationview(view)?),
            "observation.estimate",
            None
        );
        let poses = poses.extract()?;
        let view = generated::modeling::read_observationview(view)?;
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.estimate_observation(client, session, edit, poses, view))
        });
        reply(py, result, "observation.estimate", |value| {
            Ok(generated::modeling::write_observationestimate(py, &value)?.into_bound(py))
        })
    }
    fn observation_job(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: &Bound<'_, PyAny>,
        view: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _lease = input_lease!(
            self,
            py,
            input_budget::poses(poses)?
                .saturating_add(generated::modeling::measure_observationview(view)?),
            "observation",
            None
        );
        let poses = poses.extract()?;
        let view = generated::modeling::read_observationview(view)?;
        let expected = expected.map(modeling_bridge::read_source).transpose()?;
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let result = py.detach(|| {
            self.host.call(move |rt| {
                rt.request_observation(client, session, edit, poses, view, expected)
            })
        });
        reply(py, result, "observation", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
}
