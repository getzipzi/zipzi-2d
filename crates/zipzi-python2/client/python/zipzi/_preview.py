"""Client-owned animation playback on an exact frozen source; Rust owns evaluation."""
from __future__ import annotations
from collections.abc import Sequence
from typing import TYPE_CHECKING
from ._owned import Owned
from ._jobs import Job
from ._results import Snapshot
from ._values import Source, record, integer
from ._generated import values as m
from ._generated.codecs import preview as _modeling_codec
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object

class Preview(Owned):
    """Independent frozen animation runtime. Create through Session.preview or Edit.preview.

    Close explicitly or use with. Edits never refresh this source. Playback and
    physics state belong to this Preview; authored content/history are unchanged.
    Each accepted sample request increments its operation ordinal, including a
    cancelled request. A busy Preview refuses controls and concurrent samples.
    """
    def __init__(self, backend: Backend):
        super().__init__(backend, "preview")
        self._source: Source | None = None
        self._serial = 0

    @classmethod
    def _create(cls, backend: Backend, session: object, edit: object | None, expected: Source | None) -> Preview:
        if expected is not None and not isinstance(expected, Source):
            raise TypeError("Expected Source or None")
        preview=cls(backend)
        def load(raw: object) -> Preview:
            value=record(raw)
            preview._token=value["preview"]
            preview._serial=integer(value["serial"])
            preview._source=Source._read(value["source"])
            return preview
        return preview._request("capture",lambda:backend.create_preview(session,edit,expected,preview),load)

    @property
    def id(self) -> int:
        """Read the Runtime-wide serial without IO; combine with source.runtime to compare returned frames across views."""
        return self._serial

    @property
    def source(self) -> Source:
        """Read the cached exact frozen Source without IO or refreshing the model."""
        assert self._source is not None
        return self._source

    def info(self) -> Snapshot[m.PreviewState]:
        """Read this Preview's accepted ordinal and actual SDK clock. Busy work refuses this read."""
        self._check(self._backend)
        return self._request("info",lambda:self._backend.preview_state(self._token,self),self._state)

    def _state(self, raw: object) -> Snapshot[m.PreviewState]:
        value=record(raw)
        state=value["value"]
        return Snapshot(Source._read(value["source"]),state if isinstance(state,m.PreviewState) else _modeling_codec.read_PreviewState(state))

    def apply(self, command: m.PreviewCommand) -> Snapshot[m.PreviewState]:
        """Apply one typed animation control in this Preview. SDK validation errors preserve its usable state; commands never publish model history."""
        self._check(self._backend)
        _modeling_codec.validate_PreviewCommand(command)
        return self._request("apply",lambda:self._backend.apply_preview(self._token,command,self),self._state)

    def _sample(self, times: Sequence[float], geometry: bool, incremental: bool, queries: Sequence[m.SpatialQuery], view: m.ObservationView | None) -> Job[Snapshot[m.PreviewBatch]]:
        self._check(self._backend)
        from ._spatial import inputs
        selected=inputs(queries)
        if view is not None:
            from ._observation import view_input
            view=view_input(view)
        targets=tuple(times)
        import math
        if not targets or view is None and len(targets)>64 or any(isinstance(t,bool) or not isinstance(t,(float,int)) or not math.isfinite(t) or not 0<=t<=1_000_000/60 for t in targets):
            raise ValueError("Expected finite nonnegative SDK sample times; CPU-only batches require 1..64 targets")
        if type(geometry) is not bool:raise TypeError("Expected geometry bool")
        def decode(raw: object) -> Snapshot[m.PreviewBatch]:
            value=record(raw)
            samples=value["samples"]
            batch=samples if isinstance(samples,m.PreviewBatch) else _modeling_codec.read_PreviewBatch(samples)
            return Snapshot(Source._read(value["source"]),batch)
        return Job[Snapshot[m.PreviewBatch]]._start(self._backend,
            lambda owner:self._backend.sample_preview(self._token,targets,geometry,incremental,selected,view,owner),"preview",decode)

    def sample(self, times: Sequence[float], *, geometry: bool = True, queries: Sequence[m.SpatialQuery] = (), view: m.ObservationView | None = None) -> Job[Snapshot[m.PreviewBatch]]:
        """Submit canonical seeks in target order; wait for a frozen-source batch.

        View batches use the host observation frame limit; CPU-only batches allow
        1..64 frames. Duplicates/backward times are retained. CPU geometry is optional.
        view requests GPU RGBA/PNG from each exact state using once-resolved batch
        textures and applies SDK Model opacity. Missing devices fail explicitly;
        image/capture failure may retain completed seeks. No view keeps CPU-only work.
        queries returns geometric results from the same SDK frame, including hidden
        objects when requested; textures, masks and occlusion are not pixel-tested. A cancelled
        batch discards output and retains completed targets; each individual SDK
        seek is transactional when interrupted; cancellation after the whole batch
        completes reports success. Close/wait the Job to release its
        result slot. Cancellation never changes authored content or another view.
        """
        return self._sample(times,geometry,False,queries,view)

    def estimate_observation(self, times: Sequence[float], *, view: m.ObservationView, geometry: bool = True, queries: Sequence[m.SpatialQuery] = ()) -> m.ObservationEstimate:
        """Estimate frozen-source observation using metadata and current controls.

        This synchronous read does not replay, advance the clock/sequence, decode
        images, initialize a device or reserve a Job. Busy Previews refuse it.
        The result reports static feasibility and current availability separately;
        it does not reserve capacity for a later sample/seek/advance_to.
        """
        self._check(self._backend)
        from ._observation import view_input
        from ._spatial import inputs
        from ._generated.codecs.observation import read_ObservationEstimate
        import math
        targets=tuple(times);selected=inputs(queries);view=view_input(view)
        if not targets or any(isinstance(t,bool) or not isinstance(t,(float,int)) or not math.isfinite(t) or not 0<=t<=1_000_000/60 for t in targets):
            raise ValueError("Expected nonempty finite nonnegative SDK sample times")
        if type(geometry) is not bool:raise TypeError("Expected geometry bool")
        def decode(raw:object)->m.ObservationEstimate:
            return raw if isinstance(raw,m.ObservationEstimate) else read_ObservationEstimate(raw)
        return self._request("estimate_observation",lambda:self._backend.estimate_preview_observation(self._token,targets,view,geometry,selected,self),decode)

    def seek(self, time: float, *, geometry: bool = True, queries: Sequence[m.SpatialQuery] = (), view: m.ObservationView | None = None) -> Job[Snapshot[m.PreviewBatch]]:
        """Submit one canonical seek and return a one-frame batch Job; an interrupted SDK seek preserves its prior state; late cancellation reports the completed outcome."""
        return self._sample((time,),geometry,False,queries,view)

    def advance_to(self, time: float, *, geometry: bool = True, queries: Sequence[m.SpatialQuery] = (), view: m.ObservationView | None = None) -> Job[Snapshot[m.PreviewBatch]]:
        """Submit SDK canonical incremental playback. Cancellation may retain completed simulation steps; discard cancelled output. Backward targets use canonical seek."""
        return self._sample((time,),geometry,True,queries,view)

    def __enter__(self) -> Preview:
        """Validate this owned Preview and return it; scope exit closes it and cancels pending playback work."""
        super().__enter__()
        return self
