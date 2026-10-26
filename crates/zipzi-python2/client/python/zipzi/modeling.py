"""Canonical modeling values and source-bearing selected record snapshots.

Operations use the same SDK authoring/plan engines in both backends. Collection
and parameter sweeps are explicit on Session.modeling and Edit.modeling.
"""
from ._generated.modeling import *

from ._generated.modeling import __all__ as __all__

from typing import ClassVar as _ClassVar
from collections.abc import Mapping as _Mapping
from ._values import Version as _Version, Source as _Source
from ._extensions import ExtensionValue as ExtensionValue

# Resolve postponed annotations through the same private aliases as their declaring module.
_ANNOTATION_TYPES = (_ClassVar, _Mapping, _Version, _Source)

from builtins import bytes as _Bytes
_ANNOTATION_BYTES = _Bytes
