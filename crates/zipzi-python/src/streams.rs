//! Direct cursor/metadata/event conversion; subscription never captures a model.
use crate::*;

fn cursor(py: Python<'_>, value: EventCursor) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("runtime", value.runtime.to_string())?;
    d.set_item(
        "session",
        value
            .session
            .map(|s| identity(py, s.runtime_instance(), s.serial()))
            .transpose()?
            .map(Bound::into_any),
    )?;
    d.set_item("sequence", value.sequence)?;
    Ok(d)
}
fn event<'py>(py: Python<'py>, value: &RuntimeEvent) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("cursor", cursor(py, value.cursor)?)?;
    d.set_item(
        "client",
        value
            .client
            .map(|c| identity(py, c.runtime_instance(), c.serial()))
            .transpose()?
            .map(Bound::into_any),
    )?;
    d.set_item(
        "job",
        value
            .job
            .map(|j| identity(py, j.runtime_instance(), j.serial()))
            .transpose()?
            .map(Bound::into_any),
    )?;
    d.set_item(
        "operation",
        identity(
            py,
            value.operation.runtime_instance(),
            value.operation.serial(),
        )?,
    )?;
    let (name, data) = match &value.kind {
        EventKind::Published(r) => ("published", receipt(py, r)?),
        EventKind::Saved(r) => {
            let data = PyDict::new(py);
            data.set_item("before", version(py, r.before)?)?;
            data.set_item("after", version(py, r.after)?)?;
            data.set_item("manifest", r.manifest.to_string_lossy().as_ref())?;
            data.set_item("durable", r.durable)?;
            ("saved", data)
        }
        EventKind::StateChanged(s) => ("state_changed", session_info(py, s.clone())?),
        EventKind::Created(s) => ("created", session_info(py, s.clone())?),
        EventKind::Closed(s) => ("closed", session_info(py, s.clone())?),
    };
    d.set_item("kind", name)?;
    d.set_item("value", data)?;
    Ok(d)
}
fn runtime_id(value: String) -> PyResult<RuntimeInstanceId> {
    value
        .parse::<RuntimeInstanceId>()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pymethods]
impl NativeRuntime {
    #[pyo3(signature = (session))]
    fn subscribe(&self, py: Python<'_>, session: Option<&SessionToken>) -> PyResult<Py<PyDict>> {
        let session = session.map(|s| s.id);
        let result = py.detach(|| {
            self.host.call(move |rt| match session {
                Some(session) => {
                    let sub = rt.subscribe_info(session)?;
                    Ok((Some(sub.info), vec![], sub.cursor))
                }
                None => {
                    let sub = rt.subscribe_registry();
                    Ok((None, sub.sessions, sub.cursor))
                }
            })
        });
        reply(py, result, "events.subscribe", |(info, sessions, at)| {
            let d = PyDict::new(py);
            d.set_item("cursor", cursor(py, at)?)?;
            if let Some(info) = info {
                d.set_item("info", session_info(py, info)?)?;
            } else {
                let values = PyList::empty(py);
                for info in sessions {
                    values.append(session_info(py, info)?)?;
                }
                d.set_item("sessions", values)?;
            }
            Ok(d.into_any())
        })
    }
    fn events(&self, py: Python<'_>, at: &Bound<'_, PyAny>, limit: usize) -> PyResult<Py<PyDict>> {
        let runtime = runtime_id(at.getattr("runtime")?.extract()?)?;
        let session = at.getattr("session")?;
        let session = if session.is_none() {
            None
        } else {
            Some(SessionId::from_parts(
                runtime_id(session.getattr("runtime")?.extract()?)?,
                session.getattr("serial")?.extract()?,
            ))
        };
        let at = EventCursor {
            runtime,
            session,
            sequence: at.getattr("sequence")?.extract()?,
        };
        let result = py.detach(|| self.host.call(move |rt| rt.events_page(at, limit)));
        reply(py, result, "events.poll", |events| {
            let values = PyList::empty(py);
            for value in events {
                values.append(event(py, &value)?)?;
            }
            Ok(values.into_any())
        })
    }
}
