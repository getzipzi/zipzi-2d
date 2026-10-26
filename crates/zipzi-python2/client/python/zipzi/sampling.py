"""Explicit CPU sampling with generated owned values and exact source checks."""
from ._generated.sampling_values import (
    Checks as Checks, BlendMode as BlendMode, DiagnosticKind as DiagnosticKind,
    RenderKind as RenderKind, Diagnostic as Diagnostic, Parameter as Parameter,
    Pose as Pose, Drawable as Drawable, Deformer as Deformer, Offscreen as Offscreen,
    RenderCommand as RenderCommand, Frame as Frame, Result as Result, inputs as inputs,
)
from ._results import Snapshot
from ._values import Source, record

Inputs = tuple[dict[str, float], ...]

def read(raw: object) -> Snapshot[Result]:
    source = Source._read(record(raw)['source'])
    value = Result._read(raw)
    if source.version != value.version or len(value.parameters) != len(value.frames):
        raise ValueError('Inconsistent sampled source or batch')
    return Snapshot(source, value)

__all__ = ['Checks', 'BlendMode', 'DiagnosticKind', 'RenderKind', 'Diagnostic', 'Parameter', 'Pose', 'Drawable', 'Deformer', 'Offscreen', 'RenderCommand', 'Frame', 'Result']
