//! Direct owned conversions for the selected, handwritten parameter slice.
use crate::*;
use zipzi_runtime::authoring::{ParameterBatch, ParameterQuery};

pub(crate) fn batch(py: Python<'_>, value: ParameterBatch) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    let values = PyList::empty(py);
    for p in value.parameters {
        values.append(generated::modeling::write_parameter(py, &p)?)?;
    }
    d.set_item("parameters", values)?;
    Ok(d)
}

#[pymethods]
impl NativeRuntime {
    #[pyo3(signature = (session, edit, ids))]
    fn meshes(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        ids: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(self, py, input_budget::strings(Some(ids))?, "query", None);
        let ids = ids.extract()?;
        let client = self.client;
        let input = zipzi_runtime::authoring::MeshBatchQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| {
            self.host.call(move |rt| {
                let lease = rt.reserve_transfer(rt.mesh_read_bytes(
                    client,
                    input.session,
                    input.edit,
                    &input.ids,
                )?)?;
                Ok((rt.mesh_batch(client, input)?, lease))
            })
        });
        reply(py, result, "meshes.get_many", |(batch, _lease)| {
            let result = PyList::empty(py);
            for value in batch.items {
                result.append(crate::generated::mesh::write_mesh(py, value)?)?;
            }
            let response = PyDict::new(py);
            response.set_item("source", source(py, batch.source)?)?;
            response.set_item("items", result)?;
            Ok(response.into_any())
        })
    }
    #[pyo3(signature = (session, edit, ids))]
    fn parameters(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        ids: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(self, py, input_budget::strings(ids)?, "query", None);
        let ids = ids.map(|v| v.extract()).transpose()?;
        let client = self.client;
        let input = ParameterQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.parameter_snapshot(client, input))
        });
        reply(py, result, "parameters.get", |value| {
            Ok(batch(py, value)?.into_any())
        })
    }
}
