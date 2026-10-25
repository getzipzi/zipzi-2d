"""Finite public request directory; generation takes over in P3A-3."""
from . import _lifecycle as lifecycle
from . import resources, references, events
from ._calls import register_request
from ._owned import Owned
from .buffers import Buffer


def register() -> None:
    # Only complete-call methods belong here; constructors, codecs and user
    # callables cannot enter through arbitrary getattr/name-based matching.
    groups = (
        (lifecycle.Runtime, "close events_since"),
        (lifecycle.Sessions, "create list attach open import_ subscribe"),
        (lifecycle.Session, "edit info editor_context capabilities address reference show_window subscribe undo redo"),
        (lifecycle.Project, "save run diagnose export replace open"),
        (lifecycle.Edit, "apply apply_many commit cancel"),
        (lifecycle.Job, "wait close state cancel"),
        (lifecycle.Meshes, "get get_many plan"),
        (lifecycle.EditableMeshes, "create rename set_positions"),
        (lifecycle.Images, "get plan snapshot"),
        (lifecycle.EditableImages, "create_image replace_image"),
        (lifecycle.Parameters, "get get_many list"),
        (lifecycle.EditableParameters, "create patch"),
        (lifecycle.Bindings, "get get_many list"),
        (lifecycle.EditableBindings, "create set_keyform"),
        (lifecycle.Scene, "get get_many plan"),
        (lifecycle.Modeling, "records plan plan_async sample sample_async"),
        (resources.Resources, "prepare_image prepare_buffer load_png prepare_png usage"),
        (resources.PreparedImage, "pixels"),
        (Buffer, "read"),
        (Owned, "close"),
        (references.ObjectRef, "resolve label capabilities scene_capabilities"),
        (references.LabelAddress, "plan"),
        (references.FieldAddress, "plan"),
        (events.SessionSubscription, "poll resync"),
        (events.RegistrySubscription, "poll resync"),
    )
    for cls, names in groups:
        for name in names.split():
            register_request(getattr(cls, name))
