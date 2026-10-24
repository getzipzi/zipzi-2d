//! Thin Runtime bridge for zipzi-python2. Public values and errors live in
//! the pure Python client; native calls own their inputs before releasing GIL.
use pyo3::{
    prelude::*,
    types::{PyAny, PyDict, PyList, PyModule},
};
use std::{path::Path, sync::Arc};
use zipzi_core::Vec2;
use zipzi_runtime::{
    host::{HostError, HostResult, LocalHost},
    *,
};
use zipzi_sdk::{MeshCreateSpec, MeshGeometry, MeshOperation, Version};

#[pyclass(frozen, name = "_SessionToken")]
struct SessionToken {
    id: SessionId,
}
#[pyclass(frozen, name = "_EditToken")]
struct EditToken {
    id: EditId,
}
#[pyclass(frozen, name = "_JobToken")]
struct JobToken {
    id: JobId,
}

#[pyclass(frozen, name = "_Runtime")]
struct NativeRuntime {
    host: Arc<LocalHost>,
    client: ClientId,
}
impl Drop for NativeRuntime {
    fn drop(&mut self) {
        // Explicit close and ordinary garbage collection both release the GIL
        // while joining native workers. During interpreter finalization there
        // may be no attachable interpreter; native workers never call Python.
        let _ = Python::try_attach(|py| py.detach(|| self.host.close()));
    }
}

fn version(py: Python<'_>, v: Version) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("session_id", v.session_id)?;
    d.set_item("generation", v.generation)?;
    d.set_item("revision", v.revision)?;
    Ok(d)
}
fn identity(
    py: Python<'_>,
    runtime: RuntimeInstanceId,
    serial: u64,
) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("runtime", runtime.to_string())?;
    d.set_item("serial", serial)?;
    Ok(d)
}
fn source(py: Python<'_>, s: SourceRef) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    let session = s.session();
    d.set_item(
        "session",
        identity(py, session.runtime_instance(), session.serial())?,
    )?;
    match s {
        SourceRef::Formal { version: v, .. } => {
            d.set_item("kind", "formal")?;
            d.set_item("version", version(py, v)?)?;
        }
        SourceRef::Edit {
            edit,
            base,
            sequence,
            ..
        } => {
            d.set_item("kind", "edit")?;
            d.set_item(
                "edit",
                identity(py, edit.runtime_instance(), edit.serial())?,
            )?;
            d.set_item("base", version(py, base)?)?;
            d.set_item("sequence", sequence)?;
        }
    }
    Ok(d)
}
fn error<'py>(
    py: Python<'py>,
    failure: HostError,
    operation: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("message", failure.to_string())?;
    d.set_item("operation", operation)?;
    let code = match failure {
        HostError::Runtime(RuntimeError::Sdk(e)) => {
            d.set_item("message", e.message.as_ref())?;
            d.set_item("operation", e.operation)?;
            d.set_item("object_ids", e.object_ids)?;
            d.set_item("field_path", e.field_path.map(|s| s.to_string()))?;
            d.set_item(
                "expected_version",
                e.expected_version.map(|v| version(py, *v)).transpose()?,
            )?;
            d.set_item(
                "actual_version",
                e.actual_version.map(|v| version(py, *v)).transpose()?,
            )?;
            d.set_item("referrers", e.referrers.into_vec())?;
            d.set_item("code", e.code.to_string())?;
            return Ok(d);
        }
        HostError::Full | HostError::Runtime(RuntimeError::Busy) => "BUSY",
        HostError::Closed => "HOST_CLOSED",
        HostError::Panicked => "HOST_PANICKED",
        HostError::Runtime(e) => match e {
            RuntimeError::Budget(_) => "BUDGET_EXCEEDED",
            RuntimeError::Aborted => "EDIT_ABORTED",
            RuntimeError::InvalidReference => "INVALID_REFERENCE",
            RuntimeError::WrongOwner => "WRONG_OWNER",
            RuntimeError::Faulted => "FAULTED",
            RuntimeError::Modified => "MODIFIED",
            RuntimeError::ResyncRequired => "RESYNC_REQUIRED",
            RuntimeError::Cancelled => "CANCELLED",
            RuntimeError::ExecutorFailure(_) => "EXECUTOR_FAILURE",
            RuntimeError::WorkerPanic(_) => "WORKER_PANIC",
            RuntimeError::Path(_) => "INVALID_PATH",
            RuntimeError::Sdk(_) | RuntimeError::Busy => unreachable!(),
        },
    };
    d.set_item("code", code)?;
    Ok(d)
}
fn reply<'py, T>(
    py: Python<'py>,
    result: HostResult<T>,
    operation: &str,
    convert: impl FnOnce(T) -> PyResult<Bound<'py, PyAny>>,
) -> PyResult<Py<PyDict>> {
    let d = PyDict::new(py);
    match result {
        Ok(value) => d.set_item("ok", convert(value)?)?,
        Err(e) => d.set_item("error", error(py, e, operation)?)?,
    }
    Ok(d.unbind())
}
fn points(py: Python<'_>, values: Vec<Vec2>) -> PyResult<Bound<'_, PyList>> {
    let result = PyList::empty(py);
    for p in values {
        let d = PyDict::new(py);
        d.set_item("x", p.x)?;
        d.set_item("y", p.y)?;
        result.append(d)?;
    }
    Ok(result)
}
fn geometry(py: Python<'_>, value: MeshGeometry) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("vertex_ids", value.vertex_ids)?;
    d.set_item("positions", points(py, value.positions)?)?;
    d.set_item("uvs", points(py, value.uvs)?)?;
    d.set_item("triangles", value.triangles)?;
    Ok(d)
}
fn read_points(value: Bound<'_, PyAny>) -> PyResult<Vec<Vec2>> {
    Ok(value
        .extract::<Vec<(f32, f32)>>()?
        .into_iter()
        .map(|(x, y)| Vec2::new(x, y))
        .collect())
}
fn read_operations(operations: &Bound<'_, PyAny>) -> PyResult<Vec<MeshOperation>> {
    operations
        .try_iter()?
        .map(|value| {
            let op = value?;
            match op.getattr("kind")?.extract::<String>()?.as_str() {
                "create" => {
                    let spec = op.getattr("spec")?;
                    let g = spec.getattr("geometry")?;
                    Ok(MeshOperation::Create {
                        spec: MeshCreateSpec {
                            id: spec.getattr("id")?.extract()?,
                            name: spec.getattr("name")?.extract()?,
                            texture_asset_id: spec.getattr("texture_asset_id")?.extract()?,
                            geometry: MeshGeometry {
                                vertex_ids: g.getattr("vertex_ids")?.extract()?,
                                positions: read_points(g.getattr("positions")?)?,
                                uvs: read_points(g.getattr("uvs")?)?,
                                triangles: g.getattr("triangles")?.extract()?,
                            },
                        },
                    })
                }
                "rename" => Ok(MeshOperation::Rename {
                    id: op.getattr("id")?.extract()?,
                    name: op.getattr("name")?.extract()?,
                }),
                "set_positions" => Ok(MeshOperation::SetPositions {
                    id: op.getattr("id")?.extract()?,
                    vertex_ids: op.getattr("vertex_ids")?.extract()?,
                    positions: read_points(op.getattr("positions")?)?,
                }),
                _ => Err(pyo3::exceptions::PyTypeError::new_err(
                    "Unknown mesh operation",
                )),
            }
        })
        .collect()
}
fn receipt<'py>(py: Python<'py>, r: &zipzi_sdk::EditReceipt) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("kind", "published")?;
    d.set_item("before", version(py, r.before)?)?;
    d.set_item("after", version(py, r.after)?)?;
    d.set_item("changed", r.changed)?;
    d.set_item("label", &r.label)?;
    d.set_item("object_ids", &r.object_ids)?;
    Ok(d)
}
#[pymethods]
impl NativeRuntime {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let (host, client) = py
            .detach(|| -> HostResult<_> {
                let host = Arc::new(LocalHost::new(Default::default(), 16).map_err(|e| {
                    HostError::Runtime(RuntimeError::ExecutorFailure(e.to_string()))
                })?);
                let client = host.call(|rt| rt.register_client())?;
                Ok((host, client))
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { host, client })
    }
    #[getter]
    fn contract(&self) -> &'static str {
        "zipzi-python2-1"
    }
    fn close(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let result = py.detach(|| self.host.close());
        reply(py, result, "close", |()| Ok(py.None().into_bound(py)))
    }
    fn create_session(&self, py: Python<'_>, document_id: String) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let value = py.detach(|| {
            self.host.call(move |rt| {
                rt.create_session(client, &document_id, zipzi_core::ModelSetup::default())
            })
        });
        reply(py, value, "sessions.create", |id| {
            Ok(Py::new(py, SessionToken { id })?.into_bound(py).into_any())
        })
    }
    fn info(&self, py: Python<'_>, session: &SessionToken) -> PyResult<Py<PyDict>> {
        let id = session.id;
        let value = py.detach(|| self.host.call(move |rt| rt.session_info(id)));
        reply(py, value, "session.info", |s| {
            let d = PyDict::new(py);
            d.set_item("version", version(py, s.version)?)?;
            d.set_item("modified", s.modified)?;
            d.set_item("history_lengths", s.history_lengths)?;
            Ok(d.into_any())
        })
    }
    fn begin_edit(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        label: String,
    ) -> PyResult<Py<PyDict>> {
        let (client, session) = (self.client, session.id);
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.begin_edit(client, session, label, None))
        });
        reply(py, value, "edit.begin", |id| {
            Ok(Py::new(py, EditToken { id })?.into_bound(py).into_any())
        })
    }
    fn apply(
        &self,
        py: Python<'_>,
        edit: &EditToken,
        operations: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        // Own all arrays while attached; never borrow a mutable Python buffer
        // after releasing the GIL. Conversion failure is handled by Python Edit.
        let operations = read_operations(operations)?;
        let (client, edit) = (self.client, edit.id);
        let (value, state) = py
            .detach(|| {
                self.host.call(move |rt| {
                    let result = rt.edit_meshes(client, edit, operations);
                    let state = rt.edit_state(client, edit).ok();
                    Ok((result, state))
                })
            })
            .map(|(v, state)| (v.map_err(HostError::Runtime), state))
            .unwrap_or_else(|e| (Err(e), None));
        let result = reply(py, value, "edit.apply", |r| {
            let d = PyDict::new(py);
            d.set_item("source", source(py, r.source)?)?;
            d.set_item("ids", r.ids)?;
            Ok(d.into_any())
        })?;
        if let Some(e) = result.bind(py).get_item("error")? {
            e.set_item(
                "edit_state",
                if state == Some(EditState::Editing) {
                    "editing"
                } else {
                    "aborted"
                },
            )?;
        }
        Ok(result)
    }
    #[pyo3(signature = (session, id, edit=None))]
    fn mesh(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        id: String,
        edit: Option<&EditToken>,
    ) -> PyResult<Py<PyDict>> {
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.mesh_snapshot(client, session, edit, &id))
        });
        reply(py, value, "meshes.get", |r| {
            let d = PyDict::new(py);
            d.set_item("source", source(py, r.source)?)?;
            d.set_item("id", r.id)?;
            d.set_item("name", r.name)?;
            d.set_item("geometry", geometry(py, r.geometry)?)?;
            Ok(d.into_any())
        })
    }
    fn release_edit(&self, py: Python<'_>, edit: &EditToken) -> PyResult<Py<PyDict>> {
        let (client, edit) = (self.client, edit.id);
        let value = py.detach(|| self.host.call(move |rt| rt.release_edit(client, edit)));
        reply(py, value, "edit.cancel", |()| Ok(py.None().into_bound(py)))
    }
    fn commit(&self, py: Python<'_>, edit: &EditToken) -> PyResult<Py<PyDict>> {
        let (client, edit) = (self.client, edit.id);
        let value = py.detach(|| self.host.call(move |rt| rt.request_commit(client, edit)));
        reply(py, value, "edit.commit", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
    fn save(&self, py: Python<'_>, session: &SessionToken, path: String) -> PyResult<Py<PyDict>> {
        let (client, session) = (self.client, session.id);
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.request_save(client, session, Path::new(&path)))
        });
        reply(py, value, "project.save", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
    fn open(&self, py: Python<'_>, path: String) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.request_open(client, Path::new(&path)))
        });
        reply(py, value, "sessions.open", |open| {
            let d = PyDict::new(py);
            match open {
                OpenRequest::Existing(id) => {
                    d.set_item("session", Py::new(py, SessionToken { id })?)?
                }
                OpenRequest::Pending(id) => d.set_item("job", Py::new(py, JobToken { id })?)?,
            }
            Ok(d.into_any())
        })
    }
    fn history(&self, py: Python<'_>, session: &SessionToken, redo: bool) -> PyResult<Py<PyDict>> {
        let (client, session) = (self.client, session.id);
        let value = py.detach(|| {
            self.host.call(move |rt| {
                if redo {
                    rt.request_redo(client, session)
                } else {
                    rt.request_undo(client, session)
                }
            })
        });
        reply(py, value, "session.history", |id| {
            Ok(Py::new(py, JobToken { id })?.into_bound(py).into_any())
        })
    }
    fn wait(&self, py: Python<'_>, job: &JobToken) -> PyResult<Py<PyDict>> {
        let id = job.id;
        let info = py.detach(|| self.host.wait_job(self.client, id));
        let terminal = info.is_ok();
        let result = info.and_then(|info| {
            info.result
                .ok_or(HostError::Closed)?
                .map_err(HostError::Runtime)
        });
        let response = reply(py, result, "job.wait", |result| {
            let d = match result {
                JobValue::Published(r) => receipt(py, &r)?,
                JobValue::Saved(r) => {
                    let d = PyDict::new(py);
                    d.set_item("kind", "saved")?;
                    d.set_item("before", version(py, r.before)?)?;
                    d.set_item("after", version(py, r.after)?)?;
                    d.set_item("manifest", r.manifest.to_string_lossy().as_ref())?;
                    d.set_item("durable", r.durable)?;
                    d
                }
                JobValue::Opened(id) => {
                    let d = PyDict::new(py);
                    d.set_item("kind", "opened")?;
                    d.set_item("session", Py::new(py, SessionToken { id })?)?;
                    d
                }
                JobValue::Imported { .. } => {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Import has no Python handler yet",
                    ));
                }
            };
            Ok(d.into_any())
        })?;
        // Conversion can fail (e.g. Python allocation). Keep the Runtime slot
        // until Python owns the complete result, so waiting can be retried.
        if terminal {
            let client = self.client;
            if let Err(failure) = py.detach(|| self.host.call(move |rt| rt.release_job(client, id)))
            {
                return reply::<()>(py, Err(failure), "job.release", |()| {
                    Ok(py.None().into_bound(py))
                });
            }
        }
        Ok(response)
    }
}
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeRuntime>()?;
    Ok(())
}
