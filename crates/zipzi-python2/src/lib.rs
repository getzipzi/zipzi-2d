//! Thin Runtime bridge for zipzi-python2. Public values and errors live in
//! the pure Python client; native calls own their inputs before releasing GIL.
use pyo3::{
    prelude::*,
    types::{PyAny, PyBytes, PyDict, PyList, PyModule},
};
use std::{path::Path, sync::Arc};
mod generated;
mod parameter_bridge;
mod streams;
use generated::mesh::read_mesh_operation;
use zipzi_runtime::authoring::{
    EditOperation, ImagePixels, MAX_IMAGE_BYTES, PlanIntent, PlanSnapshot,
};
use zipzi_runtime::{
    host::{HostError, HostResult, LocalHost},
    *,
};
use zipzi_sdk::{ImageCreateSpec, MeshOperation, Version};

#[pyclass(frozen, name = "_SessionToken")]
struct SessionToken {
    id: SessionId,
}
#[pymethods]
impl SessionToken {
    #[getter]
    fn runtime(&self) -> String {
        self.id.runtime_instance().to_string()
    }
    #[getter]
    fn serial(&self) -> u64 {
        self.id.serial()
    }
}
#[pyclass(frozen, name = "_EditToken")]
struct EditToken {
    id: EditId,
}
#[pyclass(frozen, name = "_JobToken")]
struct JobToken {
    id: JobId,
}

#[pyclass(frozen, name = "_ImageToken")]
struct ImageToken {
    id: ImageId,
}
#[pyclass(frozen, name = "_PlanToken")]
struct PlanToken {
    id: PlanId,
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
fn read_ids(value: Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    let ids: Vec<u32> = value.extract()?;
    zipzi_runtime::metrics::materialized_input(ids.len() * std::mem::size_of::<u32>());
    Ok(ids)
}
fn read_triangles(value: Bound<'_, PyAny>) -> PyResult<Vec<[u32; 3]>> {
    let triangles: Vec<[u32; 3]> = value.extract()?;
    zipzi_runtime::metrics::materialized_input(triangles.len() * std::mem::size_of::<[u32; 3]>());
    Ok(triangles)
}
fn read_operations(operations: &Bound<'_, PyAny>) -> PyResult<Vec<EditOperation>> {
    operations
        .try_iter()?
        .map(|value| {
            let op = value?;
            let kind = op.getattr("kind")?.extract::<String>()?;
            let mesh = match kind.as_str() {
                "create_parameter" | "patch_parameter" => {
                    return Ok(EditOperation::Parameter {
                        operation: parameter_bridge::read(&kind, &op)?,
                    });
                }
                "create_image" => {
                    let image = op.getattr("image")?.getattr("_token")?;
                    return Ok(EditOperation::CreateImage {
                        spec: ImageCreateSpec {
                            id: op.getattr("id")?.extract()?,
                            name: op.getattr("name")?.extract()?,
                        },
                        image: image.extract::<PyRef<'_, ImageToken>>()?.id,
                    });
                }
                "replace_image" => {
                    let image = op.getattr("image")?.getattr("_token")?;
                    return Ok(EditOperation::ReplaceImage {
                        id: op.getattr("id")?.extract()?,
                        image: image.extract::<PyRef<'_, ImageToken>>()?.id,
                    });
                }
                "apply_plan" => {
                    let plan = op.getattr("plan")?.getattr("_token")?;
                    return Ok(EditOperation::ApplyPlan {
                        plan: plan.extract::<PyRef<'_, PlanToken>>()?.id,
                    });
                }
                _ => read_mesh_operation(&kind, &op),
            }?;
            Ok(EditOperation::Mesh { operation: mesh })
        })
        .collect()
}
fn pixels(py: Python<'_>, r: ImagePixels) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("width", r.width)?;
    d.set_item("height", r.height)?;
    zipzi_runtime::metrics::copy_payload(r.rgba.len());
    d.set_item("rgba", PyBytes::new(py, &r.rgba))?;
    Ok(d)
}
fn plan_snapshot(py: Python<'_>, p: PlanSnapshot) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("plan", Py::new(py, PlanToken { id: p.plan })?)?;
    d.set_item("source", source(py, p.source)?)?;
    let r = PyDict::new(py);
    r.set_item("version", version(py, p.report.version)?)?;
    r.set_item("affected_ids", p.report.affected_ids)?;
    r.set_item("copied_ids", p.report.copied_ids)?;
    r.set_item("losses", p.report.losses)?;
    r.set_item("no_op", p.report.no_op)?;
    if let Some(cost) = p.report.cost {
        let c = PyDict::new(py);
        c.set_item("sample_combinations", cost.sample_combinations)?;
        c.set_item("payload_bytes", cost.payload_bytes)?;
        c.set_item("temporary_bytes", cost.temporary_bytes)?;
        c.set_item("history_bytes", cost.history_bytes)?;
        r.set_item("cost", c)?;
    } else {
        r.set_item("cost", py.None())?;
    }
    d.set_item("report", r)?;
    Ok(d)
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
fn session_info(py: Python<'_>, value: SessionInfo) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item(
        "id",
        identity(py, value.id.runtime_instance(), value.id.serial())?,
    )?;
    d.set_item("version", version(py, value.version)?)?;
    d.set_item("modified", value.modified)?;
    d.set_item("history_lengths", value.history_lengths)?;
    d.set_item(
        "state",
        match value.state {
            SessionState::Ready => "ready",
            SessionState::Busy(_) => "busy",
            SessionState::Faulted(_) => "faulted",
            SessionState::Closing => "closing",
            SessionState::Closed => "closed",
        },
    )?;
    d.set_item("path", value.path.map(|p| p.to_string_lossy().into_owned()))?;
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
        "zipzi-python2-5"
    }
    #[cfg(feature = "bench-instrumentation")]
    fn _measure(&self, py: Python<'_>, reset: bool) -> PyResult<Py<PyDict>> {
        let s = if reset {
            zipzi_runtime::metrics::reset()
        } else {
            zipzi_runtime::metrics::snapshot()
        };
        let d = PyDict::new(py);
        d.set_item("enabled", s.enabled)?;
        d.set_item("rust_live_bytes", s.rust_live_bytes)?;
        d.set_item("rust_start_bytes", s.rust_start_bytes)?;
        d.set_item("rust_peak_bytes", s.rust_peak_bytes)?;
        d.set_item("rust_allocations", s.rust_allocations)?;
        d.set_item("rust_allocated_bytes", s.rust_allocated_bytes)?;
        d.set_item("json_encodes", s.json_encodes)?;
        d.set_item("json_decodes", s.json_decodes)?;
        d.set_item("json_value_builds", s.json_value_builds)?;
        d.set_item("json_admission_encodes", s.json_admission_encodes)?;
        d.set_item("materialized_input_bytes", s.materialized_input_bytes)?;
        d.set_item("copied_payload_bytes", s.copied_payload_bytes)?;
        Ok(d.unbind())
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
            Ok(session_info(py, s)?.into_any())
        })
    }
    fn list_sessions(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let value = py.detach(|| self.host.call(|rt| Ok(rt.list_sessions())));
        reply(py, value, "sessions.list", |sessions| {
            let result = PyList::empty(py);
            for s in sessions {
                result.append(session_info(py, s)?)?;
            }
            Ok(result.into_any())
        })
    }
    fn attach(&self, py: Python<'_>, runtime: String, serial: u64) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let value = py.detach(|| {
            self.host.call(move |rt| {
                let id = rt
                    .list_sessions()
                    .into_iter()
                    .map(|s| s.id)
                    .find(|id| {
                        id.runtime_instance().to_string() == runtime && id.serial() == serial
                    })
                    .ok_or(RuntimeError::InvalidReference)?;
                rt.attach_session(client, id)?;
                Ok(id)
            })
        });
        reply(py, value, "sessions.attach", |id| {
            let result = PyDict::new(py);
            result.set_item("id", Py::new(py, SessionToken { id })?)?;
            Ok(result.into_any())
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
                    let result = rt.apply_authoring(client, edit, operations);
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
    fn prepare_image(
        &self,
        py: Python<'_>,
        width: u32,
        height: u32,
        rgba: &Bound<'_, PyBytes>,
    ) -> PyResult<Py<PyDict>> {
        if rgba.as_bytes().len() > MAX_IMAGE_BYTES {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Initial pixel transport is limited to 1 MiB",
            ));
        }
        zipzi_runtime::metrics::copy_payload(rgba.as_bytes().len());
        let rgba = rgba.as_bytes().to_vec();
        let client = self.client;
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.prepare_image(client, width, height, rgba))
        });
        reply(py, value, "resources.prepare_image", |r| {
            let d = PyDict::new(py);
            d.set_item("image", Py::new(py, ImageToken { id: r.image })?)?;
            d.set_item("width", r.width)?;
            d.set_item("height", r.height)?;
            Ok(d.into_any())
        })
    }
    fn prepared_pixels(&self, py: Python<'_>, image: &ImageToken) -> PyResult<Py<PyDict>> {
        let (client, image) = (self.client, image.id);
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.prepared_image_pixels(client, image))
        });
        reply(py, value, "resources.pixels", |r| {
            Ok(pixels(py, r)?.into_any())
        })
    }
    #[pyo3(signature = (session, id, edit=None))]
    fn image(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        id: String,
        edit: Option<&EditToken>,
    ) -> PyResult<Py<PyDict>> {
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.image_snapshot(client, session, edit, &id))
        });
        reply(py, value, "resources.get", |r| {
            let d = PyDict::new(py);
            d.set_item("source", source(py, r.source)?)?;
            d.set_item("id", r.id)?;
            d.set_item("name", r.name)?;
            d.set_item("pixels", pixels(py, r.pixels)?)?;
            Ok(d.into_any())
        })
    }
    fn plan(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        intent: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let id = intent.getattr("id")?.extract()?;
        let expected_name = intent.getattr("expected_name")?.extract()?;
        let name = intent.getattr("name")?.extract()?;
        let intent = match intent.getattr("kind")?.extract::<String>()?.as_str() {
            "mesh_rename" => PlanIntent::MeshRename {
                id,
                expected_name,
                name,
            },
            "image_rename" => PlanIntent::ImageRename {
                id,
                expected_name,
                name,
            },
            _ => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Unknown Plan intent",
                ));
            }
        };
        let (client, session, edit) = (self.client, session.id, edit.map(|e| e.id));
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.plan_authoring(client, session, edit, intent))
        });
        reply(py, value, "plan.create", |p| {
            Ok(plan_snapshot(py, p)?.into_any())
        })
    }
    fn release_handle(
        &self,
        py: Python<'_>,
        kind: &str,
        token: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let value = match kind {
            "image" => {
                let image = token.extract::<PyRef<'_, ImageToken>>()?.id;
                py.detach(|| {
                    self.host
                        .call(move |rt| rt.release_prepared_image(client, image))
                })
            }
            "plan" => {
                let plan = token.extract::<PyRef<'_, PlanToken>>()?.id;
                py.detach(|| {
                    self.host
                        .call(move |rt| rt.release_modeling_plan(client, plan))
                })
            }
            _ => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Unknown handle kind",
                ));
            }
        };
        reply(py, value, &format!("{kind}.close"), |()| {
            Ok(py.None().into_bound(py))
        })
    }
    fn statistics(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let value = py.detach(|| {
            self.host.call(|rt| {
                Ok((
                    rt.prepared_image_count(),
                    rt.modeling_plan_count(),
                    rt.image_memory_usage(),
                ))
            })
        });
        reply(py, value, "resources.usage", |(images, plans, memory)| {
            let d = PyDict::new(py);
            let m = PyDict::new(py);
            m.set_item("rgba_bytes", memory.rgba_bytes)?;
            m.set_item("png_bytes", memory.png_bytes)?;
            m.set_item("blob_bytes", memory.blob_bytes)?;
            m.set_item("preparing_bytes", memory.preparing_bytes)?;
            d.set_item("image_memory", m)?;
            d.set_item("prepared_images", images)?;
            d.set_item("plans", plans)?;
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
    #[cfg(feature = "test-support")]
    fn _test_control(&self, py: Python<'_>, action: String) -> PyResult<Py<PyDict>> {
        reply(
            py,
            self.host.test_control(&action).map_err(HostError::Runtime),
            "test.control",
            |v| Ok(v.into_pyobject(py)?.to_owned().into_any()),
        )
    }
    fn cancel_job(&self, py: Python<'_>, job: &JobToken) -> PyResult<Py<PyDict>> {
        let (client, job) = (self.client, job.id);
        let result = py.detach(|| self.host.call(move |rt| rt.cancel_job(client, job)));
        reply(py, result, "job.cancel", |()| Ok(py.None().into_bound(py)))
    }
    fn job_state(&self, py: Python<'_>, job: &JobToken) -> PyResult<Py<PyDict>> {
        let (client, job) = (self.client, job.id);
        let result = py.detach(|| {
            self.host
                .call(move |rt| Ok(rt.job_info(client, job)?.state))
        });
        reply(py, result, "job.state", |state| {
            let name = match state {
                JobState::Queued => "queued",
                JobState::Running => "running",
                JobState::Cancelling => "cancelling",
                JobState::Succeeded => "succeeded",
                JobState::Failed => "failed",
                JobState::Cancelled => "cancelled",
            };
            Ok(name.into_pyobject(py)?.into_any())
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
