"""Installed pure wheel + production desktop: real animated-model authoring/IO.

Requires an explicit model with a motion containing a resolved parameter track.
Run with the isolated installed interpreter and -I; no local model is bundled.
"""
from pathlib import Path
from dataclasses import replace
import argparse, importlib.util, json, os, signal, tempfile
import zipzi
from zipzi import animation, modeling as m

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    host = args.host.resolve()
    model = args.model.resolve()
    output = args.output.resolve()
    assert host.is_file() and model.is_file()
    output.mkdir(exist_ok=False)
    assert '/site-packages/zipzi/' in zipzi.__file__ and importlib.util.find_spec('zipzi_native') is None
    pid = None
    try:
        with tempfile.TemporaryDirectory(prefix='zzr7mao-', dir='/tmp') as temporary:
            endpoint = Path(temporary) / 'mao.sock'
            os.environ['ZIPZI_CONFIG_DIR'] = str(Path(temporary) / 'config')
            with zipzi.connect(endpoint=endpoint, start=True, host=host) as runtime:
                assert runtime.connection is not None and 'show_window' in runtime.connection.capabilities
                pid = runtime.connection.host_pid
                session = runtime.sessions.import_(model, m.project.ImportFormat.MODEL3).wait()
                mesh = session.meshes.list(limit=1).items[0].id
                ids = session.modeling.records(m.ModelingQuery()).value.ids
                clip = session.modeling.records(m.ModelingQuery(motions=(ids['motions'][0],))).value.motions[0]
                index = next((i for i, t in enumerate(clip.tracks) if isinstance(t.target, m.motion.MotionTrackTargetParameter)))
                track = clip.tracks[index]
                parameter = session.parameters.get(track.target.parameter_id).value
                value = parameter.minimum * 0.75 + parameter.maximum * 0.25
                if abs(value - track.initial.value) < 0.0001:
                    value = parameter.minimum * 0.25 + parameter.maximum * 0.75
                assert abs(value - track.initial.value) > 0.0001
                tracks = list(clip.tracks)
                tracks[index] = replace(track, initial=replace(track.initial, value=value))
                with session.edit('Installed desktop Mao authoring') as edit:
                    edit.meshes.rename(mesh, 'R7 installed Mao mesh')
                    edit.apply(m.commands.ModelingCommandReplaceMotion(clip=replace(clip, name='R7 installed Mao motion', tracks=tuple(tracks))))
                assert session.info().history == (1, 0)
                with session.preview() as preview:
                    preview.apply(animation.ScheduleMotion(id=clip.id, time=0.0))
                    sampled = preview.sample((0.0, 0.125)).wait()
                    assert sampled.source == preview.source and len(sampled.value.frames) == 2
                before = session.modeling.records(m.ModelingQuery(motions=(clip.id,))).value.motions[0]
                saved = session.project.save(output / 'project').wait()
                assert saved.durable
                reopened = session.project.open(saved.manifest).wait()
                assert reopened.after.generation > reopened.before.generation
                assert session.meshes.get(mesh).value.name == 'R7 installed Mao mesh'
                assert session.modeling.records(m.ModelingQuery(motions=(clip.id,))).value.motions[0] == before
                assert session.info().history == (0, 0) and (not session.info().modified)
                motion = session.project.export(output / 'clip.motion3.json', m.project.ExportFormat.MOTION3, object_id=clip.id).wait()
                package = session.project.export(output / 'runtime', m.project.ExportFormat.PACKAGE, allow_loss=True).wait()
                assert motion.published and package.published
                assert (output / 'runtime/model.model3.json').is_file()
                curves = json.loads((output / 'clip.motion3.json').read_text())['Curves']
                curve = next((c for c in curves if c['Target'] == 'Parameter' and c['Id'] == parameter.runtime_id))
                assert abs(curve['Segments'][1] - before.tracks[index].initial.value) < 0.0001
                session.show_window()
                with zipzi.connect(endpoint=endpoint) as reconnected:
                    current = reconnected.sessions.attach(session.id)
                    assert current.meshes.get(mesh).value.name == 'R7 installed Mao mesh'
                    actual = current.modeling.records(m.ModelingQuery(motions=(clip.id,))).value.motions[0]
                    assert actual == before
                report = dict(
                    status='passed',
                    host_pid=pid,
                    meshes=len(ids['meshes']),
                    motions=len(ids['motions']),
                    edited_clip=clip.id,
                    old_key=track.initial.value,
                    new_key=before.tracks[index].initial.value,
                    preview_frames=2,
                    durable=saved.durable,
                    reopened_generation=reopened.after.generation,
                    motion_exported=motion.published,
                    package_exported=package.published,
                    package_allow_loss=True,
                    package_warnings=list(package.warnings),
                    package_diagnostics=list(map(str, package.diagnostics)),
                )
                (output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
                print(json.dumps(report), flush=True)
    finally:
        if pid is not None:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
if __name__ == '__main__':
    main()
