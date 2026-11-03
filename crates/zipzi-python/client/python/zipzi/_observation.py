"""Owned rendering requests. Rust owns image, coordinate and texture algorithms."""
from collections.abc import Mapping,Sequence
from ._facade_support import _DomainOwner
from ._generated import values as m
from ._generated.codecs import observation as _modeling_codec
from ._jobs import Job
from ._results import Snapshot
from ._values import Source,record
from . import sampling
from ._calls import known

def view_input(view:m.ObservationView)->m.ObservationView:
    _modeling_codec.validate_ObservationView(view)
    if isinstance(view.texture_policy,m.ObservationTexturePolicyFitBudget) and view.texture_policy.max_rgba_bytes<=0:
        raise ValueError("Texture budget must be positive")
    return view

def estimate(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],view:m.ObservationView)->m.ObservationEstimate:
    parameters=sampling.inputs(poses);selected=view_input(view)
    if not parameters:raise ValueError("Expected nonempty poses")
    session=owner._session;edit=owner._edit
    token=session._token;candidate=None if edit is None else edit._token
    if edit is not None:edit._ensure_editing()
    def decode(raw:object)->m.ObservationEstimate:
        return raw if isinstance(raw,m.ObservationEstimate) else _modeling_codec.read_ObservationEstimate(raw)
    return known(lambda:decode(session._backend.estimate_observation(token,candidate,parameters,selected,owner)),lambda:decode(session._backend.recover(owner)))

def read(raw:object)->Snapshot[m.ObservationBatch]:
    value=record(raw)
    return Snapshot(Source._read(value['source']),_modeling_codec.read_ObservationBatch(value['samples']))

def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],view:m.ObservationView,expected:Source|None)->Job[Snapshot[m.ObservationBatch]]:
    parameters=sampling.inputs(poses);selected=view_input(view)
    if not parameters:raise ValueError("Expected nonempty poses")
    if expected is not None and not isinstance(expected,Source):raise TypeError("Expected Source")
    session=owner._session;edit=owner._edit
    token=session._token;candidate=None if edit is None else edit._token
    if edit is not None:edit._ensure_editing()
    return Job[Snapshot[m.ObservationBatch]]._start(session._backend,
        lambda job:session._backend.observation_job(token,candidate,parameters,selected,expected,job),'observation',read)
