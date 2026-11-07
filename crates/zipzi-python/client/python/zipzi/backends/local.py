"""Owned native values cross PyO3 without a JSON encode/decode roundtrip."""
from importlib import import_module
from typing import Any
from ..values import CONTRACT
from .._errors import SdkFailure, unwrap

class LocalBackend:
    connection_info = None
    def __init__(self) -> None:
        try: self._native = import_module('zipzi_native._native')._Runtime()
        except ImportError as e: raise SdkFailure('LOCAL_BACKEND_MISSING','Install matching zipzi-native') from e
        if self._native.contract != CONTRACT:
            self._native.close()
            raise SdkFailure('PROTOCOL_MISMATCH','Native adapter uses another authoring contract')
    def request(self, operation: dict[str, Any]) -> Any: return unwrap(self._native.request(operation))
    def binary_request(self, operation: dict[str, Any], payload: bytes = b'') -> tuple[Any, bytes]:
        value, data = self._native.binary_request(operation, payload)
        return unwrap(value), data
    def recover(self) -> Any: raise SdkFailure('NO_PENDING_REQUEST','Local calls have no transport replay window')
    def close(self) -> None: self._native.close()
