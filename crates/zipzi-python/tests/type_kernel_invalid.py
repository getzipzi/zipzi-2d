"""Every marked line must produce a typing diagnostic."""
import zipzi
from zipzi import values as v

def invalid(draft: zipzi.Draft, session: zipzi.Session) -> None:
    draft.apply({'command':'rename'})  # error
    draft.apply('plan-token')  # error
    v.ParameterCreateSpec(name=7,minimum=0,maximum=1,default_value=0)  # error
    v.Version(session='id',generation='zero',revision=0)  # error
    v.ObjectAddress(kind='parameter',id='id')  # error
    session.query(v.QuerySummary(),source=session.version)  # error
    draft.source.sequence=2  # error
    session.query(v.QuerySummary()).value['missing']  # error
    session.query(v.QuerySummary()).value['meshes'] = 2  # error
    session.save().wait()['published'] = False  # error
