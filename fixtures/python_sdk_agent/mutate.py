"""External Connected fixture publications using only the normal installed SDK."""
import json
from pathlib import Path
import sys
import zipzi
from zipzi import parameters

data=json.loads(Path(sys.argv[1]).read_text())
with zipzi.connect(endpoint=Path(data['controller_endpoint'])) as runtime:
    identity=data['fixture_sessions'][0]
    session=runtime.sessions.attach(zipzi.SessionId(identity['runtime'],identity['serial']))
    before=session.info()
    with session.edit('external fixture publication') as edit:
        edit.parameters.create(parameters.ParameterSpec('External fixture',0.,1.,0.))
    after=session.info()
    assert after.version!=before.version and after.history[0]==before.history[0]+1
    session.detach()
print(json.dumps({'published':True,'before':repr(before.version),'after':repr(after.version)}))
