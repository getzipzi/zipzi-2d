"""Static ownership and generated-value checks; not executed."""
from pathlib import Path
from typing import Any, assert_type
import zipzi
from zipzi import values as v

def workflow(runtime: zipzi.Runtime) -> None:
    session = runtime.sessions.create()
    assert_type(session, zipzi.Session)
    assert_type(session.version, v.Version)
    with session.draft() as draft:
        command = v.CommandCreateParameter(spec=v.ParameterCreateSpec(name='p',minimum=0,maximum=1,default_value=0))
        job = draft.apply_async(command)
        assert_type(job, zipzi.Job[v.DraftSource])
        assert_type(job.wait(), v.DraftSource)
        assert_type(draft.query(v.QuerySummary()), zipzi.Snapshot)
        job.close()
    assert_type(session.save(Path('/tmp/test.zipzi')).wait()['published'], Any)  # dynamic job report field
