//! Owned numeric inputs and direct CPU-frame conversion. No JSON intermediary.
use crate::*;
use zipzi_runtime::authoring::{
    MeshBindingBatch, MeshBindingQuery, ModelingSampleBatch, ModelingSampleQuery,
};
use zipzi_sdk::{
    MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation, MeshKeyformSpec,
};

fn keyform(value: &Bound<'_, PyAny>) -> PyResult<MeshKeyformSpec> {
    Ok(MeshKeyformSpec {
        keys: value.getattr("keys")?.extract()?,
        positions: generated::mesh::read_points(value.getattr("positions")?)?,
        opacity: value.getattr("opacity")?.extract()?,
        multiply: value.getattr("multiply")?.extract()?,
        screen: value.getattr("screen")?.extract()?,
        draw_order: value.getattr("draw_order")?.extract()?,
    })
}
pub(crate) fn read(tag: &str, operation: &Bound<'_, PyAny>) -> PyResult<MeshBindingOperation> {
    if tag == "create_mesh_binding" {
        let value = operation.getattr("spec")?;
        let axes = value
            .getattr("axes")?
            .try_iter()?
            .map(|a| {
                let a = a?;
                Ok(MeshBindingAxisSpec {
                    parameter_id: a.getattr("parameter_id")?.extract()?,
                    keys: a.getattr("keys")?.extract()?,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        let keyforms = value
            .getattr("keyforms")?
            .try_iter()?
            .map(|k| keyform(&k?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(MeshBindingOperation::Create {
            spec: MeshBindingCreateSpec {
                mesh_id: value.getattr("mesh_id")?.extract()?,
                id: value.getattr("id")?.extract()?,
                axes,
                keyforms,
            },
        })
    } else {
        Ok(MeshBindingOperation::SetKeyform {
            id: operation.getattr("id")?.extract()?,
            keyform: keyform(&operation.getattr("keyform")?)?,
        })
    }
}
fn binding_batch(py: Python<'_>, value: MeshBindingBatch) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    let bindings = PyList::empty(py);
    for binding in value.bindings {
        bindings.append(generated::modeling::write_meshbinding(py, &binding)?)?;
    }
    d.set_item("bindings", bindings)?;
    Ok(d)
}
pub(crate) fn sample_batch<'py>(
    py: Python<'py>,
    value: &ModelingSampleBatch,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    d.set_item("state", value.state)?;
    d.set_item(
        "samples",
        generated::modeling::write_modelingsamples(py, &value.samples)?,
    )?;
    Ok(d)
}
fn read_source(value: &Bound<'_, PyAny>) -> PyResult<SourceRef> {
    let runtime_value = value.getattr("runtime")?;
    let runtime = runtime_value
        .extract::<&str>()?
        .parse::<RuntimeInstanceId>()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let session = SessionId::from_parts(runtime, value.getattr("session")?.extract()?);
    let v = value.getattr("version")?;
    let version = Version {
        session_id: v.getattr("session_id")?.extract()?,
        generation: v.getattr("generation")?.extract()?,
        revision: v.getattr("revision")?.extract()?,
    };
    Ok(match value.getattr("edit")?.extract::<Option<u64>>()? {
        None => SourceRef::Formal { session, version },
        Some(edit) => SourceRef::Edit {
            session,
            edit: EditId::from_parts(runtime, edit),
            base: version,
            sequence: value.getattr("sequence")?.extract()?,
        },
    })
}
#[pymethods]
impl NativeRuntime {
    #[pyo3(signature = (session, edit, ids))]
    fn bindings(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        ids: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(self, py, input_budget::strings(ids)?, "query", None);
        let ids = ids.map(|v| v.extract()).transpose()?;
        let client = self.client;
        let input = MeshBindingQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| {
            self.host.call(move |rt| {
                let lease = rt.reserve_transfer(rt.binding_read_bytes(client, &input)?)?;
                Ok((rt.mesh_binding_snapshot(client, input)?, lease))
            })
        });
        reply(py, result, "bindings.get", |(value, _lease)| {
            Ok(binding_batch(py, value)?.into_any())
        })
    }
    #[pyo3(signature = (session, edit, poses, checks, expected))]
    fn sample(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: &Bound<'_, PyAny>,
        checks: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            input_budget::poses(poses)?,
            "modeling.sample",
            None
        );
        let poses = poses.extract()?;
        let input = sample_input(session.id, edit.map(|e| e.id), poses, checks, expected)?;
        let client = self.client;
        let result = py.detach(|| self.host.call(move |rt| rt.sample_modeling(client, input)));
        reply(py, result, "modeling.sample", |value| {
            Ok(sample_batch(py, &value)?.into_any())
        })
    }
}

#[pymethods]
impl NativeRuntime {
    fn modeling_records(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_modelingquery(query)?,
            "modeling.records",
            None
        );
        let query = generated::modeling::read_modelingquery(query)?;
        let client = self.client;
        let session = session.id;
        let edit = edit.map(|e| e.id);
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.modeling_snapshot(client, session, edit, query))
        });
        reply(py, result, "modeling.records", |batch| {
            let out = PyDict::new(py);
            out.set_item("source", source(py, batch.source)?)?;
            out.set_item(
                "records",
                generated::modeling::write_ownedmodelingrecords(py, &batch.records)?,
            )?;
            out.set_item(
                "_lease",
                Py::new(
                    py,
                    TransferLease {
                        _guard: batch.lease,
                    },
                )?,
            )?;
            Ok(out.into_any())
        })
    }
    fn modeling_plan(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        intent: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_modelingintent(intent)?,
            "modeling.plan",
            None
        );
        let intent = generated::modeling::read_modelingintent(intent)?;
        let client = self.client;
        let session = session.id;
        let edit = edit.map(|e| e.id);
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.plan_modeling(client, session, edit, intent))
        });
        reply(py, result, "modeling.plan", |value| {
            Ok(generated::resources::write_plansnapshot(
                py,
                authoring::PlanSnapshot {
                    plan: value.0,
                    source: value.1,
                    report: value.2,
                },
            )?
            .into_any())
        })
    }
}

fn sample_input(
    session: SessionId,
    edit: Option<EditId>,
    poses: Vec<zipzi_core::PreviewValues>,
    checks: &Bound<'_, PyAny>,
    expected: Option<&Bound<'_, PyAny>>,
) -> PyResult<ModelingSampleQuery> {
    let bounds = checks
        .getattr("model_bounds")?
        .extract::<Option<((f32, f32), (f32, f32))>>()?
        .map(|(min, max)| zipzi_sdk::GeometryBounds {
            min: zipzi_core::Vec2::new(min.0, min.1),
            max: zipzi_core::Vec2::new(max.0, max.1),
        });
    Ok(ModelingSampleQuery {
        session,
        edit,
        parameters: poses,
        checks: zipzi_sdk::GeometryChecks {
            min_triangle_area: checks.getattr("min_triangle_area")?.extract()?,
            model_bounds: bounds,
        },
        expected: expected.map(read_source).transpose()?,
    })
}

#[pymethods]
impl NativeRuntime {
    fn sample_job(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: &Bound<'_, PyAny>,
        checks: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            input_budget::poses(poses)?,
            "modeling.sample",
            None
        );
        let poses = poses.extract()?;
        let input = sample_input(session.id, edit.map(|e| e.id), poses, checks, expected)?;
        let client = self.client;
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.request_modeling_samples(client, input))
        });
        reply(py, value, "modeling.sample_job", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
    fn modeling_plan_job(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        intent: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_modelingintent(intent)?,
            "modeling.plan",
            None
        );
        let intent = generated::modeling::read_modelingintent(intent)?;
        let client = self.client;
        let session = session.id;
        let edit = edit.map(|e| e.id);
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.request_modeling_plan(client, session, edit, intent))
        });
        reply(py, value, "modeling.plan_job", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
}
