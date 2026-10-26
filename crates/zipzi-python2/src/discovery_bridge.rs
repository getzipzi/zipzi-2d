//! Native ownership boundary. Field conversion is emitted from Runtime declarations.
use crate::*;
fn domain(value: &str) -> PyResult<CollectionDomain> {
    Ok(match value {
        "meshes" => CollectionDomain::Meshes,
        "parameters" => CollectionDomain::Parameters,
        "bindings" => CollectionDomain::Bindings,
        "resources" => CollectionDomain::Resources,
        "scene" => CollectionDomain::Scene,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Unknown collection domain",
            ));
        }
    })
}
#[pymethods]
impl NativeRuntime {
    fn runtime_limits(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let value = py.detach(|| self.host.call(|rt| Ok(rt.limits().clone())));
        reply(py, value, "runtime.limits", |v| {
            Ok(generated::discovery::write_runtimelimits(py, &v)?.into_any())
        })
    }
    fn runtime_capabilities(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let value = py.detach(|| self.host.call(|rt| Ok(rt.runtime_capabilities())));
        reply(py, value, "runtime.capabilities", |v| {
            Ok(generated::discovery::write_runtimecapabilities(py, &v)?.into_any())
        })
    }
    fn detach_session(&self, py: Python<'_>, session: &SessionToken) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let id = session.id;
        let value = py.detach(|| self.host.call(move |rt| rt.detach_session(client, id)));
        reply(py, value, "session.detach", |()| {
            Ok(py.None().into_bound(py))
        })
    }
    fn close_session(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        discard_modified: bool,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let id = session.id;
        let value = py.detach(|| {
            self.host
                .call(move |rt| rt.close_session(client, id, discard_modified))
        });
        reply(py, value, "session.close", |v| {
            Ok(session_info(py, v)?.into_any())
        })
    }
    #[pyo3(signature=(session,edit,collection,name,limit,cursor))]
    fn collection_page(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        collection: &str,
        name: Option<String>,
        limit: usize,
        cursor: Option<String>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(self, py, 16384, "collection.list", None);
        let client = self.client;
        let input = CollectionQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            domain: domain(collection)?,
            name,
            limit,
            cursor,
        };
        let value = py.detach(|| self.host.call(move |rt| rt.collection_page(client, input)));
        reply(py, value, "collection.list", |v| {
            let result = PyDict::new(py);
            result.set_item("source", source(py, v.source)?)?;
            let items = PyList::empty(py);
            for v in v.items {
                items.append(generated::discovery::write_objectsummary(py, &v)?)?;
            }
            result.set_item("items", items)?;
            result.set_item("next_cursor", v.next_cursor)?;
            Ok(result.into_any())
        })
    }
    #[pyo3(signature=(session,edit,collection,ids,expected))]
    fn read_selection(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        collection: &str,
        ids: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let _input = input_lease!(
            self,
            py,
            input_budget::strings(Some(ids))?,
            "collection.get_many",
            None
        );
        let client = self.client;
        let id = session.id;
        let edit = edit.map(|e| e.id);
        let collection = domain(collection)?;
        let ids = ids.extract()?;
        let expected = expected.map(modeling_bridge::read_source).transpose()?;
        let value = py.detach(|| {
            self.host.call(move |rt| {
                // Conservatively reserve the complete Python expansion before conversion.
                let selection = rt.read_selection(client, id, edit, collection, ids, expected)?;
                let bytes = match &selection {
                    Selection::Meshes(v) => v.items.iter().fold(4096usize, |n, m| {
                        n.saturating_add(m.geometry.positions.len().saturating_mul(512))
                            .saturating_add(m.geometry.triangles.len().saturating_mul(256))
                    }),
                    Selection::Resources(_, v) => v
                        .iter()
                        .fold(4096usize, |n, m| n.saturating_add(m.pixels.rgba.len())),
                    _ => 1 << 20,
                };
                Ok((selection, rt.reserve_transfer(bytes)?))
            })
        });
        reply(py, value, "collection.get_many", |(v, _lease)| match v {
            Selection::Parameters(v) => Ok(parameter_bridge::batch(py, v)?.into_any()),
            Selection::Bindings(v) => Ok(modeling_bridge::binding_batch(py, v)?.into_any()),
            Selection::Meshes(v) => {
                let d = PyDict::new(py);
                d.set_item("source", source(py, v.source)?)?;
                let items = PyList::empty(py);
                for m in v.items {
                    items.append(generated::mesh::write_mesh(py, m)?)?;
                }
                d.set_item("items", items)?;
                Ok(d.into_any())
            }
            Selection::Resources(s, v) => {
                let d = PyDict::new(py);
                d.set_item("source", source(py, s)?)?;
                let items = PyList::empty(py);
                for m in v {
                    items.append(generated::resources::write_imagesnapshot(py, m)?)?;
                }
                d.set_item("items", items)?;
                Ok(d.into_any())
            }
            Selection::Scene(v) => {
                let d = PyDict::new(py);
                d.set_item("source", source(py, v.source)?)?;
                let items = PyList::empty(py);
                for m in v.items {
                    items.append(generated::scene::write_summary(py, m)?)?;
                }
                d.set_item("items", items)?;
                Ok(d.into_any())
            }
        })
    }
}
