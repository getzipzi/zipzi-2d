"""Optional native backend imports. Connected authoring needs only Python."""
from importlib import import_module


class _NativeModule:
    def __getattr__(self, name):
        try:
            return getattr(import_module("zipzi._native"), name)
        except ModuleNotFoundError as error:
            if error.name != "zipzi._native":
                raise
            raise RuntimeError("NativeBackend requires a zipzi wheel with the optional native extension") from error


native = _NativeModule()


class _NativeType:
    def __init__(self, name):
        self.name = name

    def __call__(self, *args, **kwargs):
        return getattr(native, self.name)(*args, **kwargs)

    def __getattr__(self, name):
        return getattr(getattr(native, self.name), name)


# Preserve the installed NativeBackend's exception identity. A missing extension
# is supported; a broken extension/dependency is not disguised as missing.
try:
    _native_impl = import_module("zipzi._native")
    SdkFailure = _native_impl.SdkFailure
except ModuleNotFoundError as error:
    if error.name != "zipzi._native":
        raise

    _native_impl = None

    class SdkFailure(Exception):
        """SDK failure with structured domain diagnostics."""


def capabilities():
    return native.capabilities()


def __getattr__(name):
    if name.startswith("Native") or name == "ObjectHandle":
        return getattr(_native_impl, name) if _native_impl is not None else _NativeType(name)
    raise AttributeError(name)
