"""Connected production Editor acceptance; run with tools/dev-python and --host."""
import argparse
from dataclasses import replace
import os
from pathlib import Path
import signal
import tempfile
import zipzi
from zipzi.mesh_generation import MeshSettings, generate

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host', type=Path, required=True)
args = parser.parse_args()
alpha = bytes(255 if 8 <= x < 56 and 8 <= y < 56 else 0 for y in range(64) for x in range(64))
settings = MeshSettings(edge_spacing=12, interior_spacing=12)
with zipzi.local() as local:
    try:
        generate(local, 64, 64, alpha)
    except zipzi.SdkFailure as e:
        assert e.code == 'UNSUPPORTED_OPERATION', e
    else:
        raise AssertionError('Local must reject Editor-only generation')
pid = None
try:
    with tempfile.TemporaryDirectory(prefix='zz-mesh-', dir='/tmp') as folder:
        os.environ['ZIPZI_CONFIG_DIR'] = str(Path(folder) / 'config')
        with zipzi.connect(endpoint=Path(folder)/'mesh.sock', start=True, host=args.host.resolve(),
                           timeout=60, startup_timeout=120) as runtime:
            pid = runtime.connection.host_pid
            assert 'editor_mesh_generate' in runtime.connection.capabilities
            session = runtime.sessions.create()
            assert session.info().history == (0, 0)
            for edge_mode in ('single', 'double', 'triple'):
                assert generate(runtime, 64, 64, alpha, settings=replace(settings, edge_mode=edge_mode)).triangles
            edges = generate(runtime, 64, 64, alpha, settings=settings, stage='edges')
            for algorithm in ('graded_poisson', 'simple_triangles', 'adaptive_quadtree', 'triangle_fractal', 'contour_paving'):
                s = replace(settings, fill_algorithm=algorithm)
                mesh = generate(runtime, 64, 64, alpha, settings=s)
                assert mesh.triangles and mesh == generate(runtime, 64, 64, alpha, settings=s)
                interior = generate(runtime, 64, 64, settings=s, stage='interior', geometry=edges)
                assert set(edges.vertex_ids).issubset(interior.vertex_ids)
            try:
                generate(runtime, 64, 64, alpha, settings=replace(settings, edge_spacing=0))
            except zipzi.SdkFailure as e:
                assert e.code == 'INVALID_ARGUMENT', e
            else:
                raise AssertionError('Invalid settings accepted')
            assert generate(runtime, 64, 64, alpha, settings=settings).triangles
            assert session.info().history == (0, 0)
            with session.edit('Generated mesh') as edit:
                mesh_id = edit.meshes.create(zipzi.meshes.MeshSpec('Generated', mesh))
            assert session.meshes.get(mesh_id).value.geometry == mesh
            assert session.info().history == (1, 0)
            print('PASS: Connected Editor, all five algorithms, stages, Edit commit, validation and Local refusal')
finally:
    if pid is not None:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
