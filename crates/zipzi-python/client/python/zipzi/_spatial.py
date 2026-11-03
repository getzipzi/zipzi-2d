"""Typed spatial request/result projection; geometry is evaluated only by Rust."""
from collections.abc import Sequence,Mapping
from ._generated import values as m
from ._generated.codecs import spatial as _modeling_codec
from ._values import Source,record
from ._results import Snapshot
from ._jobs import Job
from ._facade_support import _DomainOwner
from . import sampling

def inputs(queries:Sequence[m.SpatialQuery])->tuple[m.SpatialQuery,...]:
    owned=tuple(queries)
    if len(owned)>64:raise ValueError("At most 64 spatial queries")
    total=0
    for query in owned:
        if isinstance(query,m.SpatialQueryBounds):total+=len(query.targets)
        if isinstance(query,m.SpatialQueryHits) and query.max_candidates>4096:raise ValueError("At most 4096 candidates")
        if total>4096:raise ValueError("At most 4096 spatial target selectors")
        _modeling_codec.validate_SpatialQuery(query)
    return owned

def read(raw:object)->Snapshot[m.SpatialSamples]:
    value=record(raw)
    return Snapshot(Source._read(value['source']),_modeling_codec.read_SpatialSamples(value['samples']))

def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],queries:Sequence[m.SpatialQuery],expected:Source|None)->Job[Snapshot[m.SpatialSamples]]:
    parameters=sampling.inputs(poses);selected=inputs(queries)
    if expected is not None and not isinstance(expected,Source):raise TypeError("Expected Source")
    session=owner._session;edit=owner._edit
    token=session._token;candidate=None if edit is None else edit._token
    if edit is not None:edit._ensure_editing()
    return Job[Snapshot[m.SpatialSamples]]._start(session._backend,
        lambda job:session._backend.spatial_job(token,candidate,parameters,selected,expected,job),'spatial',read)
