//! Owned numeric inputs and direct CPU-frame conversion. No JSON intermediary.
use crate::*;
use zipzi_runtime::authoring::{
    MeshBindingBatch, MeshBindingQuery, ModelingSampleBatch, ModelingSampleQuery,
};
use zipzi_sdk::{
    MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation, MeshKeyformSpec,
};

macro_rules! fields {
    ($dict:ident, $value:ident, $($field:ident),* $(,)?) => { $( $dict.set_item(stringify!($field), $value.$field)?; )* };
}
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
        let b = PyDict::new(py);
        fields!(b, binding, id, mesh_id);
        let axes = PyList::empty(py);
        for axis in binding.axes {
            let a = PyDict::new(py);
            fields!(a, axis, id, parameter_id);
            let samples = PyList::empty(py);
            for sample in axis.samples {
                let s = PyDict::new(py);
                fields!(s, sample, id, value);
                samples.append(s)?;
            }
            a.set_item("samples", samples)?;
            axes.append(a)?;
        }
        b.set_item("axes", axes)?;
        let keyforms = PyList::empty(py);
        for k in binding.keyforms {
            let item = PyDict::new(py);
            fields!(item, k, samples, draw_order);
            item.set_item("positions", generated::mesh::points(py, k.positions)?)?;
            let a = PyDict::new(py);
            let appearance = k.appearance;
            fields!(a, appearance, opacity, multiply, screen);
            item.set_item("appearance", a)?;
            keyforms.append(item)?;
        }
        b.set_item("keyforms", keyforms)?;
        bindings.append(b)?;
    }
    d.set_item("bindings", bindings)?;
    Ok(d)
}
fn points<'py>(
    py: Python<'py>,
    values: impl IntoIterator<Item = zipzi_core::Vec2>,
) -> PyResult<Bound<'py, PyList>> {
    let result = PyList::empty(py);
    for p in values {
        let d = PyDict::new(py);
        fields!(d, p, x, y);
        result.append(d)?;
    }
    Ok(result)
}
fn frame(py: Python<'_>, value: zipzi_core::DrawableFrame) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source_revision", value.source_revision)?;
    let parameters = PyList::empty(py);
    for p in value.parameters {
        let item = PyDict::new(py);
        fields!(item, p, id, requested, value, clamped);
        parameters.append(item)?;
    }
    d.set_item("parameters", parameters)?;
    let drawables = PyList::empty(py);
    for drawable in value.drawables {
        let item = PyDict::new(py);
        fields!(
            item,
            drawable,
            id,
            runtime_id,
            part_id,
            raw_blend_mode,
            texture_asset_id,
            texture_slot,
            renderable,
            draw_order,
            render_order,
            opacity,
            multiply_color,
            screen_color,
            enabled,
            visible,
            double_sided,
            inverted_mask,
            masks
        );
        item.set_item("positions", points(py, drawable.positions)?)?;
        item.set_item("uvs", points(py, drawable.uvs.iter().copied())?)?;
        item.set_item("indices", PyList::new(py, drawable.indices.iter())?)?;
        item.set_item(
            "blend_mode",
            match drawable.blend_mode {
                zipzi_core::BlendMode::Normal => "normal",
                zipzi_core::BlendMode::Additive => "additive",
                zipzi_core::BlendMode::Multiplicative => "multiplicative",
            },
        )?;
        drawables.append(item)?;
    }
    d.set_item("drawables", drawables)?;
    let deformers = PyList::empty(py);
    for deformer in value.deformers {
        let item = PyDict::new(py);
        fields!(item, deformer, id, enabled);
        item.set_item("points", points(py, deformer.points)?)?;
        let axis = deformer
            .rotation_axis
            .map(|p| {
                let a = PyDict::new(py);
                fields!(a, p, x, y);
                Ok::<_, PyErr>(a)
            })
            .transpose()?;
        item.set_item("rotation_axis", axis)?;
        deformers.append(item)?;
    }
    d.set_item("deformers", deformers)?;
    let offscreens = PyList::empty(py);
    for offscreen in value.offscreens {
        let item = PyDict::new(py);
        fields!(
            item,
            offscreen,
            renderable,
            id,
            runtime_id,
            owner_part_id,
            parent_offscreen_id,
            render_order,
            opacity,
            enabled,
            blend_mode,
            flags,
            masks,
            multiply_color,
            screen_color
        );
        offscreens.append(item)?;
    }
    d.set_item("offscreens", offscreens)?;
    let render_plan = PyList::empty(py);
    for command in value.render_plan {
        let item = PyDict::new(py);
        let payload = PyDict::new(py);
        let (tag, field, id) = match command {
            zipzi_core::evaluation::RenderCommand::BeginOffscreen { offscreen_id } => {
                ("BeginOffscreen", "offscreen_id", offscreen_id)
            }
            zipzi_core::evaluation::RenderCommand::DrawMesh { mesh_id } => {
                ("DrawMesh", "mesh_id", mesh_id)
            }
            zipzi_core::evaluation::RenderCommand::EndOffscreen { offscreen_id } => {
                ("EndOffscreen", "offscreen_id", offscreen_id)
            }
        };
        payload.set_item(field, id)?;
        item.set_item(tag, payload)?;
        render_plan.append(item)?;
    }
    d.set_item("render_plan", render_plan)?;
    Ok(d)
}
fn sample_batch(py: Python<'_>, value: ModelingSampleBatch) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("source", source(py, value.source)?)?;
    d.set_item("state", value.state)?;
    let s = PyDict::new(py);
    s.set_item("version", version(py, value.samples.version)?)?;
    s.set_item("parameters", value.samples.parameters)?;
    let frames = PyList::empty(py);
    for f in value.samples.frames {
        frames.append(frame(py, f)?)?;
    }
    s.set_item("frames", frames)?;
    let diagnostics = PyList::empty(py);
    for diagnostic in value.samples.diagnostics {
        let item = PyDict::new(py);
        fields!(item, diagnostic, mesh_id, triangle_index);
        item.set_item(
            "kind",
            match diagnostic.kind {
                zipzi_sdk::GeometryDiagnosticKind::SmallTriangle => "SmallTriangle",
                zipzi_sdk::GeometryDiagnosticKind::InconsistentWinding => "InconsistentWinding",
                zipzi_sdk::GeometryDiagnosticKind::OutsideModelBounds => "OutsideModelBounds",
            },
        )?;
        diagnostics.append(item)?;
    }
    s.set_item("diagnostics", diagnostics)?;
    d.set_item("samples", s)?;
    Ok(d)
}
fn read_source(value: &Bound<'_, PyAny>) -> PyResult<SourceRef> {
    let runtime = value
        .getattr("runtime")?
        .extract::<String>()?
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
        ids: Option<Vec<String>>,
    ) -> PyResult<Py<PyDict>> {
        let client = self.client;
        let input = MeshBindingQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            ids,
        };
        let result = py.detach(|| {
            self.host
                .call(move |rt| rt.mesh_binding_snapshot(client, input))
        });
        reply(py, result, "bindings.get", |value| {
            Ok(binding_batch(py, value)?.into_any())
        })
    }
    #[pyo3(signature = (session, edit, poses, checks, expected))]
    fn sample(
        &self,
        py: Python<'_>,
        session: &SessionToken,
        edit: Option<&EditToken>,
        poses: Vec<zipzi_core::PreviewValues>,
        checks: &Bound<'_, PyAny>,
        expected: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyDict>> {
        let bounds = checks
            .getattr("model_bounds")?
            .extract::<Option<((f32, f32), (f32, f32))>>()?
            .map(|(min, max)| zipzi_sdk::GeometryBounds {
                min: zipzi_core::Vec2::new(min.0, min.1),
                max: zipzi_core::Vec2::new(max.0, max.1),
            });
        let input = ModelingSampleQuery {
            session: session.id,
            edit: edit.map(|e| e.id),
            parameters: poses,
            checks: zipzi_sdk::GeometryChecks {
                min_triangle_area: checks.getattr("min_triangle_area")?.extract()?,
                model_bounds: bounds,
            },
            expected: expected.map(read_source).transpose()?,
        };
        let client = self.client;
        let result = py.detach(|| self.host.call(move |rt| rt.sample_modeling(client, input)));
        reply(py, result, "modeling.sample", |value| {
            Ok(sample_batch(py, value)?.into_any())
        })
    }
}
