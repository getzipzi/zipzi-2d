"""Explicit lifetime for a source-bound SDK modeling plan."""
import json


class ModelingPlan:
    def __init__(self, native):
        self._native = native
        self._report = json.loads(native.report_json())
        self._closed = False

    @property
    def report(self):
        """Detached cost, loss, identity and version report."""
        return json.loads(json.dumps(self._report))

    def close(self):
        if not self._closed:
            self._native.close()
            self._closed = True

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        try:
            self.close()
        except Exception:
            if exc[0] is None:
                raise
