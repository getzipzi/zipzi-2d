"""Static ownership and generated-value checks; not executed."""
from pathlib import Path
from typing import assert_type
import zipzi
from zipzi import values as v
from zipzi.values.parameters import Parameter
from zipzi.values.meshes import MeshGeometry

def workflow(runtime: zipzi.Runtime) -> None:
    session = runtime.sessions.create()
    assert_type(session, zipzi.Session)
    assert_type(session.version, v.Version)
    with session.draft() as draft:
        command = v.CommandCreateParameter(spec=v.ParameterCreateSpec(name='p',minimum=0,maximum=1,default_value=0))
        job = draft.apply_async(command)
        assert_type(job, zipzi.Job[v.DraftSource])
        assert_type(job.wait(), v.DraftSource)
        assert_type(draft.query(v.QuerySummary()), zipzi.Snapshot[zipzi.results.Summary])
        assert_type(draft.query(v.QueryMetadata()), zipzi.Snapshot[zipzi.results.Metadata])
        page = draft.query(v.QueryCollection(kind=v.ObjectKind.PARAMETER, offset=0, limit=2))
        assert_type(page.value['next_offset'], int | None)
        assert_type(page.records(v.Parameter), tuple[v.Parameter, ...])
        assert_type(page.records(v.parameters.Parameter), tuple[Parameter, ...])
        assert_type(v.MeshGeometry, type[MeshGeometry])
        job.close()
    assert_type(session.save(Path('/tmp/test.zipzi')).wait()['published'], bool)
    assert_type(session.info()['dirty'], bool)
    assert_type(session.query(v.QuerySummary()).value['parameters'], int)
    assert_type(session.query(v.QueryMetadata()), zipzi.Snapshot[zipzi.results.Metadata])
