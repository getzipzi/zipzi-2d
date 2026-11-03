//! SDK-owned identities, without exposing forgeable SDK incarnation fields.
use crate::*;
fn kind(text: &str) -> PyResult<zipzi_sdk::ObjectKind> {
    generated::modeling::read_objectkind_name(text)
}

fn capability(py: Python<'_>, value: zipzi_sdk::CapabilityState) -> PyResult<Bound<'_, PyAny>> {
    match value {
        zipzi_sdk::CapabilityState::Available => Ok("Available".into_pyobject(py)?.into_any()),
        zipzi_sdk::CapabilityState::Unavailable { code }
        | zipzi_sdk::CapabilityState::Unsupported { code } => {
            let result = PyDict::new(py);
            let payload = PyDict::new(py);
            payload.set_item("code", code)?;
            result.set_item(
                if matches!(value, zipzi_sdk::CapabilityState::Unavailable { .. }) {
                    "Unavailable"
                } else {
                    "Unsupported"
                },
                payload,
            )?;
            Ok(result.into_any())
        }
    }
}
#[pymethods]
impl NativeRuntime {
    fn object_ref(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        object_kind: &str,
        id: &str,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            id.len()
                .saturating_add(object_kind.len())
                .saturating_mul(4)
                .saturating_add(128),
            "references.capture",
            None
        );
        let id = id.to_owned();
        let client = self.client;
        let session = session.id;
        let kind = kind(object_kind)?;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.capture_object_ref(client, session, kind, &id))
        });
        reply(py, result, "references.capture", |info| {
            let result = PyDict::new(py);
            result.set_item(
                "reference",
                Py::new(py, ObjectRefToken { id: info.reference })?,
            )?;
            result.set_item("id", info.id)?;
            result.set_item("object_kind", object_kind)?;
            Ok(result.into_any())
        })
    }
    fn resolve_ref(&self, py: Python<'_>, reference: &ObjectRefToken) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let reference = reference.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.resolve_object_ref(client, reference))
        });
        reply(py, result, "references.resolve", |value| {
            Ok(source(py, value)?.into_any())
        })
    }
    fn ref_capabilities(&self, py: Python<'_>, reference: &ObjectRefToken) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let reference = reference.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.object_scene_capabilities(client, reference))
        });
        reply(py, result, "references.capabilities", |value| {
            let result = PyDict::new(py);
            macro_rules! fields { ($($field:ident),*) => { $(result.set_item(stringify!($field), capability(py, value.$field)?)?;)* }; }
            fields!(
                inspect,
                patch,
                move_object,
                reparent,
                reorder,
                duplicate,
                delete,
                dissolve
            );
            Ok(result.into_any())
        })
    }
    fn label_address(&self, py: Python<'_>, reference: &ObjectRefToken) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let reference = reference.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.capture_label_address(client, reference))
        });
        reply(py, result, "references.label", |info| {
            let result = PyDict::new(py);
            result.set_item(
                "address",
                Py::new(py, FieldAddressToken { id: info.address })?,
            )?;
            result.set_item("id", info.id)?;
            result.set_item("expected", info.expected)?;
            Ok(result.into_any())
        })
    }
    fn label_plan(
        &self,
        py: Python<'_>,
        address: &FieldAddressToken,
        name: &str,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let address = address.id;
        let _input = input_lease!(
            self,
            py,
            name.len().saturating_mul(4).saturating_add(128),
            "references.label_plan",
            None
        );
        let name = name.to_owned();
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.plan_label_patch(client, address, name))
        });
        reply(py, result, "references.label_plan", |value| {
            Ok(generated::resources::write_plansnapshot(py, value)?.into_any())
        })
    }
}

#[pymethods]
impl NativeRuntime {
    fn capture_address(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        selector: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_addressselector(selector)?,
            "addresses.capture",
            None
        );
        let selector = generated::modeling::read_addressselector(selector)?;
        let client = self.client;
        let session = session.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.capture_address(client, session, selector))
        });
        reply(py, result, "addresses.capture", |address| {
            let out = PyDict::new(py);
            out.set_item("address", Py::new(py, FieldAddressToken { id: address })?)?;
            Ok(out.into_any())
        })
    }
    fn address_plan(
        &self,
        py: Python<'_>,
        address: &FieldAddressToken,
        patch: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_addresspatch(patch)?,
            "addresses.plan",
            None
        );
        let patch = generated::modeling::read_addresspatch(patch)?;
        let client = self.client;
        let address = address.id;
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.plan_address_patch(client, address, patch))
        });
        reply(py, result, "addresses.plan", |value| {
            Ok(generated::resources::write_plansnapshot(py, value)?.into_any())
        })
    }
    fn modeling_capabilities(
        &self,
        py: Python<'_>,
        reference: &ObjectRefToken,
        domain: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_capabilitydomain(domain)?,
            "references.capabilities",
            None
        );
        let domain = generated::modeling::read_capabilitydomain(domain)?;
        let client = self.client;
        let reference = reference.id;
        let result = py.detach(|| {
            self.host.call(move |rt| {
                Ok((
                    rt.resolve_object_ref(client, reference)?,
                    rt.modeling_capabilities(client, reference, domain)?,
                ))
            })
        });
        reply(py, result, "references.capabilities", |value| {
            let out = PyDict::new(py);
            out.set_item("source", source(py, value.0)?)?;
            out.set_item(
                "capabilities",
                generated::modeling::write_capabilities(py, &value.1)?,
            )?;
            Ok(out.into_any())
        })
    }
    fn collection_capabilities(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            generated::modeling::measure_collectionquery(query)?,
            "collections.capabilities",
            None
        );
        let query = generated::modeling::read_collectionquery(query)?;
        let client = self.client;
        let session = session.id;
        let result = py.detach(|| {
            self.host.call(move |rt| {
                Ok((
                    SourceRef::Formal {
                        session,
                        version: rt.session_info(session)?.version,
                    },
                    rt.collection_capabilities(client, session, query)?,
                ))
            })
        });
        reply(py, result, "collections.capabilities", |value| {
            let out = PyDict::new(py);
            out.set_item("source", source(py, value.0)?)?;
            out.set_item(
                "capabilities",
                generated::modeling::write_capabilities(py, &value.1)?,
            )?;
            Ok(out.into_any())
        })
    }
}

#[pymethods]
impl NativeRuntime {
    fn editor_context(&self, py: Python<'_>, session: &SessionToken) -> PyResult<Py<PyDict>> {
        let session = session.id;
        let result = py.detach(|| {
            self.host.call(move |rt| {
                Ok((
                    rt.session_info(session)?.version,
                    rt.editor_context(session)?.cloned(),
                ))
            })
        });
        reply(py, result, "session.editor_context", |(version, value)| {
            let d = PyDict::new(py);
            d.set_item(
                "source",
                source(py, SourceRef::Formal { session, version })?,
            )?;
            d.set_item(
                "context",
                value
                    .as_ref()
                    .map(|v| generated::modeling::write_editorcontext(py, v))
                    .transpose()?
                    .unwrap_or_else(|| py.None()),
            )?;
            Ok(d.into_any())
        })
    }
}
