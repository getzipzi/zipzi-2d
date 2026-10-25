//! Direct owned conversions for the selected, handwritten parameter slice.
use crate::*;
use zipzi_core::ParameterKind;
use zipzi_runtime::authoring::{ParameterBatch, ParameterQuery};
use zipzi_sdk::{ParameterCreateSpec, ParameterOperation, ParameterPatch};

fn kind(value: Option<String>) -> PyResult<Option<ParameterKind>> {
    value
        .map(|value| match value.as_str() {
            "normal" => Ok(ParameterKind::Normal),
            "blend_shape" => Ok(ParameterKind::BlendShape),
            _ => Err(pyo3::exceptions::PyValueError::new_err(
                "Unknown parameter kind",
            )),
        })
        .transpose()
}
pub(crate) fn read(tag: &str, operation: &Bound<'_, PyAny>) -> PyResult<ParameterOperation> {
    if tag == "create_parameter" {
        let value = operation.getattr("spec")?;
        Ok(ParameterOperation::Create {
            spec: ParameterCreateSpec {
                name: value.getattr("name")?.extract()?,
                minimum: value.getattr("minimum")?.extract()?,
                maximum: value.getattr("maximum")?.extract()?,
                default_value: value.getattr("default_value")?.extract()?,
                id: value.getattr("id")?.extract()?,
                runtime_id: value.getattr("runtime_id")?.extract()?,
                kind: kind(value.getattr("kind")?.extract()?)?,
                decimal_places: value.getattr("decimal_places")?.extract()?,
                repeat: value.getattr("repeat")?.extract()?,
            },
        })
    } else {
        let value = operation.getattr("patch")?;
        Ok(ParameterOperation::Patch {
            id: operation.getattr("id")?.extract()?,
            patch: ParameterPatch {
                name: value.getattr("name")?.extract()?,
                minimum: value.getattr("minimum")?.extract()?,
                maximum: value.getattr("maximum")?.extract()?,
                default_value: value.getattr("default_value")?.extract()?,
                runtime_id: value.getattr("runtime_id")?.extract()?,
                kind: kind(value.getattr("kind")?.extract()?)?,
                decimal_places: value.getattr("decimal_places")?.extract()?,
                repeat: value.getattr("repeat")?.extract()?,
            },
        })
    }
}
fn batch(py: Python<'_>, value: ParameterBatch) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    let values = PyList::empty(py);
    for p in value.parameters {
        let item = PyDict::new(py);
        item.set_item("id", p.id)?;
        item.set_item("runtime_id", p.runtime_id)?;
        item.set_item("name", p.name)?;
        item.set_item("minimum", p.minimum)?;
        item.set_item("maximum", p.maximum)?;
        item.set_item("default_value", p.default_value)?;
        item.set_item(
            "kind",
            match p.kind {
                ParameterKind::Normal => "normal",
                ParameterKind::BlendShape => "blend_shape",
            },
        )?;
        item.set_item("decimal_places", p.decimal_places)?;
        item.set_item("repeat", p.repeat)?;
        values.append(item)?;
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
        ids: Vec<String>,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let input = zipzi_runtime::authoring::MeshBatchQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| self.host.call(move |rt| rt.mesh_snapshots(client, input)));
        reply(py, result, "meshes.get_many", |values| {
            let result = PyList::empty(py);
            for value in values {
                result.append(crate::generated::mesh::write_snapshot(py, value)?)?;
            }
            Ok(result.into_any())
        })
    }
    #[pyo3(signature = (session, edit, ids))]
    fn parameters(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        ids: Option<Vec<String>>,
    ) -> PyResult<Py<PyDict>> {
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
