"""Shared complete-call boundary. No domain algorithms or transport selection."""
from collections.abc import Callable, Sequence, Mapping
from typing import Generic, TypeVar, Protocol, overload, Literal, TYPE_CHECKING
from ._calls import known
from ._results import Batch, Snapshot, Page, PageCursor
from ._values import Source, WriteResult, record, sequence, string
from . import _queries as queries, sampling, modeling, resources, ops
from ._generated import codecs as _modeling_codec
if TYPE_CHECKING:
    from ._lifecycle import Session, Edit, Job
class _DomainOwner(Protocol):
    _session: 'Session'
    _edit: 'Edit | None'

T=TypeVar('T')
S=TypeVar('S')
class Collection(Generic[T]):
    def __init__(self,session:'Session',edit:'Edit | None',domain:str,read:Callable[[object],T],key:str):
        self._session,self._edit,self._domain,self._decode,self._key=session,edit,domain,read,key
    def _read(self,ids:tuple[str,...],expected:Source|None)->Batch[T]:
        if any(not isinstance(id,str) for id in ids):raise TypeError('Expected object ID text')
        if expected is not None and not isinstance(expected,Source):raise TypeError('Expected Source')
        s=self._session;token=s._token;edit=None if self._edit is None else self._edit._token
        def decode(raw:object)->Batch[T]:return queries.batch(raw,self._key,self._decode)
        return known(lambda:decode(s._backend.read_selection(token,edit,self._domain,ids,expected,self)),lambda:decode(s._backend.recover(self)))
    @staticmethod
    def _single(batch:Batch[T])->Snapshot[T]:return queries.single(batch)
    def _write(self,construct:Callable[[],ops.Operation])->WriteResult:
        if self._edit is None:raise TypeError('Writing requires an Edit')
        self._session._check_binding()
        return self._edit._construct_and_apply(construct)
    def _page(self,name:str|None,limit:int,cursor:PageCursor|None,read:Callable[[object],S])->Page[S]:
        if name is not None and not isinstance(name,str):raise TypeError('Expected exact name or None')
        if type(limit) is not int or not 1<=limit<=256:raise ValueError('Page limit must be 1..256')
        if cursor is not None and not isinstance(cursor,PageCursor):raise TypeError('Expected PageCursor')
        s=self._session;token=s._token;edit=None if self._edit is None else self._edit._token
        def decode(raw:object)->Page[S]:
            r=record(raw);next=r['next_cursor']
            return Page(Source._read(r['source']),tuple(read(item) for item in sequence(r['items'])),None if next is None else PageCursor(string(next).encode()))
        return known(lambda:decode(s._backend.collection_page(token,edit,self._domain,name,limit,None if cursor is None else cursor._token.decode(),self)),lambda:decode(s._backend.recover(self)))
    def _image_lease(self,id:str)->resources.ImageLease:
        s=self._session
        return resources.ImageLease._capture(s._backend,s._token,None if self._edit is None else self._edit._token,id)

def records(owner:_DomainOwner,query:modeling.ModelingQuery)->Snapshot[modeling.OwnedModelingRecords]:
    # Generated owner has the same two lifecycle references as Collection.
    s=owner._session;edit=owner._edit
    _modeling_codec.validate_ModelingQuery(query)
    token=s._token;candidate=None if edit is None else edit._token
    return known(lambda:queries.records(s._backend.modeling_records(token,candidate,query,owner)),lambda:queries.records(s._backend.recover(owner)))

@overload
def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],checks:sampling.Checks,expected:Source|None,as_job:Literal[False])->Snapshot[sampling.Result]: ...
@overload
def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],checks:sampling.Checks,expected:Source|None,as_job:Literal[True])->'Job[Snapshot[sampling.Result]]': ...
def sample(owner:_DomainOwner,poses:Sequence[Mapping[str,float]],checks:sampling.Checks,expected:Source|None,as_job:bool)->'Snapshot[sampling.Result] | Job[Snapshot[sampling.Result]]':
    from ._lifecycle import Job
    owned=sampling.inputs(poses)
    if not isinstance(checks,sampling.Checks) or expected is not None and not isinstance(expected,Source):raise TypeError('Expected Checks and optional Source')
    checks._wire();s=owner._session;edit=owner._edit;token=s._token;candidate=None if edit is None else edit._token
    if as_job:return Job[Snapshot[sampling.Result]]._start(s._backend,lambda job:s._backend.sample_job(token,candidate,owned,checks,expected,job),'sampled',sampling.read)
    return known(lambda:sampling.read(s._backend.sample(token,candidate,owned,checks,expected,owner)),lambda:sampling.read(s._backend.recover(owner)))
