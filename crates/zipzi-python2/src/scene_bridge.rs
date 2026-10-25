//! Ownership and token boundary; mechanical field conversion is generated.
use crate::*;
#[pymethods]
impl NativeRuntime {
    fn scene(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        ids: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(self, py, input_budget::strings(Some(ids))?, "query", None);
        let ids = ids.extract()?;
        let client = self.client;
        let input = authoring::SceneQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| self.host.call(move |rt| rt.scene_snapshot(client, input)));
        reply(py, result, "scene.get", |batch| {
            let result = PyDict::new(py);
            result.set_item("source", source(py, batch.source)?)?;
            let items = PyList::empty(py);
            for item in batch.items {
                items.append(generated::scene::write_summary(py, item)?)?;
            }
            result.set_item("items", items)?;
            Ok(result.into_any())
        })
    }
    fn scene_plan(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let session = session.id;
        let edit = edit.map(|e| e.id);
        let _input = input_lease!(
            self,
            py,
            generated::scene::measure_operation(operation)?,
            "scene.plan",
            None
        );
        let operation = generated::scene::read_operation(operation)?;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.plan_scene_authoring(client, session, edit, operation))
        });
        reply(py, result, "scene.plan", |value| {
            Ok(generated::resources::write_plansnapshot(py, value)?.into_any())
        })
    }
}
