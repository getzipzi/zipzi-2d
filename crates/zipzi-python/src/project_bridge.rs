//! Typed lifecycle requests; SDK owns project conversion and generation.
use crate::*;
#[pymethods]
impl NativeRuntime {
    fn project(
        &self,
        py: Python<'_>,
        session: Option<&SessionToken>,
        request: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_projectrequest(request)?,
            "project",
            None
        );
        let request = generated::modeling::read_projectrequest(request)?;
        let client = self.client;
        let session = session.map(|s| s.id);
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.request_project_target(client, session, request))
        });
        reply(py, result, "project", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
}
