"""Owned rendering requests. Rust owns image, coordinate and texture algorithms."""
from collections.abc import Mapping,Sequence
from ._facade_support import _DomainOwner
from ._generated import modeling as m
from ._jobs import Job
from ._results import Snapshot
from ._values import Source,record
from . import sampling

def view_input(view:m.ObservationView)->m.ObservationView:
    m.validate_ObservationView(view)
    return view

def read(raw:object)->Snapshot[m.ObservationBatch]:
    value=record(raw)
    return Snapshot(Source._read(value['source']),m.read_ObservationBatch(value['samples']))

def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],view:m.ObservationView,expected:Source|None)->Job[Snapshot[m.ObservationBatch]]:
    parameters=sampling.inputs(poses);selected=view_input(view)
    if not parameters:raise ValueError("Expected 1..64 poses")
    if expected is not None and not isinstance(expected,Source):raise TypeError("Expected Source")
    session=owner._session;edit=owner._edit
    token=session._token;candidate=None if edit is None else edit._token
    if edit is not None:edit._ensure_editing()
    return Job[Snapshot[m.ObservationBatch]]._start(session._backend,
        lambda job:session._backend.observation_job(token,candidate,parameters,selected,expected,job),'observation',read)
