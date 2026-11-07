//! Mechanical v4 bridge. Python inputs become owned contract values before the
//! GIL is released; only the owner-thread Runtime executes authoring commands.
use pyo3::{
    prelude::*,
    types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple},
};
use serde_json::Value;
use std::sync::Arc;
use zipzi_runtime::{host::LocalHost, protocol::Protocol, ClientId};

fn failure(code: &str, message: &str) -> Value {
    zipzi_runtime::protocol::failure(code, message)
}
// Bound actual owned input before copying Python containers or strings. This
// is independent of model/history size and shares the Runtime transfer budget.
const MAX_INPUT_BYTES: usize = 128 << 20;
fn input_limit() -> PyErr {
    pyo3::exceptions::PyValueError::new_err("Owned input exceeds its allocation limit")
}
fn measure(value: &Bound<'_, PyAny>, depth: usize, bytes: &mut usize) -> PyResult<()> {
    if depth > 96 {
        return Err(input_limit());
    }
    let charge = |bytes: &mut usize, amount: usize| -> PyResult<()> {
        *bytes = bytes
            .checked_add(amount)
            .filter(|v| *v <= MAX_INPUT_BYTES)
            .ok_or_else(input_limit)?;
        Ok(())
    };
    charge(bytes, std::mem::size_of::<Value>())?;
    if let Ok(value) = value.cast::<PyString>() {
        return charge(bytes, value.to_str()?.len());
    }
    if value.is_none()
        || value.is_instance_of::<PyBool>()
        || value.is_instance_of::<PyInt>()
        || value.is_instance_of::<PyFloat>()
    {
        return Ok(());
    }
    if let Ok(value) = value.cast::<PyDict>() {
        charge(bytes, value.len().checked_mul(128).ok_or_else(input_limit)?)?;
        for (key, value) in value.iter() {
            let key = key.cast::<PyString>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Contract map keys must be strings")
            })?;
            charge(bytes, key.to_str()?.len())?;
            measure(&value, depth + 1, bytes)?;
        }
    } else if let Ok(value) = value.cast::<PyList>() {
        for item in value.iter() {
            measure(&item, depth + 1, bytes)?;
        }
    } else if let Ok(value) = value.cast::<PyTuple>() {
        for item in value.iter() {
            measure(&item, depth + 1, bytes)?;
        }
    } else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "Expected owned contract values",
        ));
    }
    Ok(())
}
fn read(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }
    if value.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(Value::String(value.extract()?));
    }
    if value.is_instance_of::<PyInt>() {
        return if let Ok(v) = value.extract::<i64>() {
            Ok(v.into())
        } else {
            Ok(value.extract::<u64>()?.into())
        };
    }
    if value.is_instance_of::<PyFloat>() {
        let v: f64 = value.extract()?;
        return serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Non-finite number"));
    }
    if let Ok(v) = value.cast::<PyDict>() {
        let mut out = serde_json::Map::new();
        for (k, v) in v.iter() {
            out.insert(k.extract()?, read(&v)?);
        }
        return Ok(Value::Object(out));
    }
    if let Ok(v) = value.cast::<PyList>() {
        let mut out = Vec::with_capacity(v.len());
        for item in v.iter() {
            out.push(read(&item)?);
        }
        return Ok(Value::Array(out));
    }
    if let Ok(v) = value.cast::<PyTuple>() {
        let mut out = Vec::with_capacity(v.len());
        for item in v.iter() {
            out.push(read(&item)?);
        }
        return Ok(Value::Array(out));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "Expected owned contract values",
    ))
}
fn write(py: Python<'_>, value: Value) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Value::Null => py.None(),
        Value::Bool(v) => v.into_pyobject(py)?.to_owned().into_any().unbind(),
        Value::String(v) => v.into_pyobject(py)?.into_any().unbind(),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else if let Some(v) = v.as_u64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else {
                v.as_f64().unwrap().into_pyobject(py)?.into_any().unbind()
            }
        }
        Value::Array(v) => {
            let out = PyList::empty(py);
            for item in v {
                out.append(write(py, item)?)?;
            }
            out.into_any().unbind()
        }
        Value::Object(v) => {
            let out = PyDict::new(py);
            for (key, value) in v {
                out.set_item(key, write(py, value)?)?;
            }
            out.into_any().unbind()
        }
    })
}
#[pyclass(frozen, name = "_Runtime")]
struct NativeRuntime {
    host: Arc<LocalHost>,
    client: ClientId,
}
#[pymethods]
impl NativeRuntime {
    #[new]
    fn new() -> PyResult<Self> {
        let host = Arc::new(
            LocalHost::new(Default::default(), 128)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?,
        );
        let client = host
            .call(|rt| rt.register_client())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { host, client })
    }
    #[getter]
    fn contract(&self) -> &'static str {
        zipzi_contract::protocol::CONTRACT
    }
    fn request(&self, py: Python<'_>, input: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let mut bytes = 0;
        measure(input, 0, &mut bytes)?;
        let lease = py
            .detach(|| self.host.call(move |rt| rt.reserve_transfer(bytes)))
            .map_err(|e| pyo3::exceptions::PyMemoryError::new_err(e.to_string()))?;
        let value = read(input)?;
        let operation = match serde_json::from_value(value) {
            Ok(v) => v,
            Err(e) => return write(py, failure("INVALID_REQUEST", &e.to_string())),
        };
        let host = self.host.clone();
        let client = self.client;
        let outcome = py.detach(move || {
            host.call(move |rt| {
                let _lease = lease;
                Ok(Protocol::default().execute_response(rt, client, operation))
            })
        });
        let response = match outcome {
            Ok(value) => value,
            Err(zipzi_runtime::host::HostError::Runtime(e)) => failure(&e.code, &e.message),
            Err(e) => failure("HOST_FAILURE", &e.to_string()),
        };
        write(py, response)
    }
    fn binary_request(
        &self,
        py: Python<'_>,
        input: &Bound<'_, PyAny>,
        payload: &Bound<'_, PyBytes>,
    ) -> PyResult<(Py<PyAny>, Py<PyBytes>)> {
        let mut bytes = payload.as_bytes().len();
        if bytes > zipzi_runtime::protocol::MAX_BINARY_CHUNK {
            return Err(input_limit());
        }
        measure(input, 0, &mut bytes)?;
        let lease = py
            .detach(|| self.host.call(move |rt| rt.reserve_transfer(bytes)))
            .map_err(|e| pyo3::exceptions::PyMemoryError::new_err(e.to_string()))?;
        let operation = read(input)?;
        let bytes = payload.as_bytes().to_vec();
        let client = self.client;
        let result = py
            .detach(|| {
                self.host.call(move |rt| {
                    let _lease = lease;
                    Ok(Protocol::default().execute_binary(rt, client, operation, bytes))
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let bytes = result.payload.as_ref().map_or(&[][..], |p| p.bytes());
        let length = bytes.len();
        let _copy = py
            .detach(|| self.host.call(move |rt| rt.reserve_transfer(length)))
            .map_err(|e| pyo3::exceptions::PyMemoryError::new_err(e.to_string()))?;
        Ok((write(py, result.value)?, PyBytes::new(py, bytes).unbind()))
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.host.close())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}
impl Drop for NativeRuntime {
    fn drop(&mut self) {
        let _ = Python::try_attach(|py| py.detach(|| self.host.close()));
    }
}
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeRuntime>()?;
    Ok(())
}
