"""Shared byte-stream contract; framing is independent of the OS transport."""
import os
import socket
from typing import Protocol


class Stream(Protocol):
    def connect(self, endpoint: str) -> None: ...
    def settimeout(self, timeout: float) -> None: ...
    def recv(self, size: int) -> bytes: ...
    def sendall(self, data: bytes) -> None: ...
    def close(self) -> None: ...


def stream() -> Stream:
    if os.name == 'nt':
        from ._windows_pipe import Pipe
        return Pipe()
    return socket.socket(socket.AF_UNIX)
