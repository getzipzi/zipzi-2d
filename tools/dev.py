#!/usr/bin/env python3
"""Cross-platform development entrypoint. No command starts a full test suite implicitly."""
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/workspace'))
import cargo_domain

HELP = '''Usage: python tools/dev.py COMMAND [arguments...]

  test sdk MODE       fast, runtime, mesh, regression, stress, python2,
                      python2-connected, full
  test editor MODE    fast, ui, regression, acceptance, tools, full
  test tools ARGS     unittest discovery arguments (-p, -k, -v)
  python ARGS         source SDK Python
  editor MODE         watch (default), reload, run, stop; Windows development prototype

Cargo filters and arguments after -- are passed through unchanged.
Windows UI logic tests are available; Metal native acceptance targets require macOS.
'''


def test_commands(domain, mode, args):
    """Return ordered commands; domain selection always uses the package inventory."""
    python = sys.executable
    groups = cargo_domain.domains()
    def cargo(operation, *flags):
        return cargo_domain.command(groups, domain, operation, list(flags))
    if domain == 'sdk':
        if mode in {'python2', 'python2-connected'}:
            name = 'test.py' if mode == 'python2' else 'connected.py'
            return [[python, str(ROOT / 'crates/zipzi-python2/tools' / name), *args]]
        selectors = {
            'fast': ['-p', 'zipzi-sdk', '--lib', '--test', 'integration'],
            'runtime': ['-p', 'zipzi-runtime'],
            'mesh': ['-p', 'zipzi-sdk', '--test', 'alpha_mesh'],
            'stress': ['-p', 'zipzi-sdk', '--test', 'alpha_mesh'],
        }
        if mode == 'full':
            return [cargo('fmt', '--', '--check'), cargo('test', '--locked', *args)]
        if mode == 'regression':
            command = cargo('test', '--locked', '--lib', '--tests', *args)
        elif mode in selectors:
            command = ['cargo', 'test', '--locked', *selectors[mode], *args]
        else:
            raise ValueError(f'Unknown SDK test mode: {mode}')
        if mode in {'mesh', 'regression'}:
            if '--' not in command:
                command.append('--')
            command += ['--skip', 'alpha_mesh_stress::']
        return [command]
    if domain != 'editor':
        raise ValueError(f'Unknown test domain: {domain}')
    if mode == 'tools':
        return [[python, str(ROOT / 'tools/test_tools.py'), *args]]
    if mode == 'fast':
        return [['cargo', 'test', '--locked', '-p', 'zipzi-editor-core', '--lib', '--test', 'integration', *args]]
    flags = ['--locked', '--features', 'zipzi-editor/test-support']
    if mode == 'ui':
        flags += ['--lib']
    elif mode == 'regression':
        flags += ['--lib', '--test', 'integration']
    elif mode == 'acceptance':
        for name in ('acceptance', 'native_render', 'headless_screenshot', 'animation_acceptance',
                     'timeline_acceptance', 'runtime_interaction_acceptance', 'runtime_ipc_acceptance',
                     'runtime_handoff_acceptance'):
            flags += ['--test', name]
    elif mode != 'full':
        raise ValueError(f'Unknown Editor test mode: {mode}')
    commands = [cargo('test', *flags, *args)]
    if mode == 'full':
        commands[:0] = [cargo('fmt', '--', '--check'), [python, str(ROOT / 'tools/test_tools.py')]]
    return commands


def run_commands(commands, *, env=None, runner=None, before=None):
    runner = subprocess.call if runner is None else runner
    for command in commands:
        if before is not None:
            before(command)
        result = runner(command, cwd=ROOT, env=env)
        if result:
            return result if result > 0 else 128 - result
    return 0


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    if not args or args[0] in {'-h', '--help', 'help'}:
        print(HELP)
        return 0
    command = args.pop(0)
    if command == 'editor':
        from dev_editor import main as editor_main
        return editor_main(args)
    if command == 'python':
        # Keep caller cwd, stdin, stdout and all Python arguments intact.
        return subprocess.call([sys.executable, str(ROOT / 'tools/python/dev_python.py'), *args])
    if command != 'test' or not args:
        raise ValueError(HELP)
    domain = args.pop(0)
    if domain == 'tools':
        return run_commands([[sys.executable, str(ROOT / 'tools/test_tools.py'), *args]])
    if not args or args[0] in {'-h', '--help', 'help'}:
        print(HELP)
        return 0
    mode = args.pop(0)
    commands = test_commands(domain, mode, args)
    # Check the complete inventory, even for focused tests. Never remove a
    # package from a domain to avoid platform-specific graphical dependencies.
    cargo_domain.validate(cargo_domain.domains())
    environment = dict(os.environ, PYTHONUTF8='1')
    # Cargo runs at the checkout root; preserve caller-relative target paths.
    if environment.get('CARGO_TARGET_DIR'):
        environment['CARGO_TARGET_DIR'] = str(Path(environment['CARGO_TARGET_DIR']).resolve())
    for invocation in commands:
        path_options = {'--target-dir'} if invocation[0] == 'cargo' else {'--output', '--editor-host'}
        end = invocation.index('--') if '--' in invocation else len(invocation)
        for index in range(end):
            if invocation[index] in path_options and index + 1 < end:
                invocation[index + 1] = str(Path(invocation[index + 1]).resolve())
            elif invocation[index].partition('=')[0] in path_options and '=' in invocation[index]:
                flag, value = invocation[index].split('=', 1)
                invocation[index] = flag + '=' + str(Path(value).resolve())
    def prepare(command):
        if domain != 'editor' or mode not in {'acceptance', 'full'} or command[:2] != ['cargo', 'test']:
            return
        if not environment.get('ZIPZI_CLIENT_PYTHON'):
            environment['ZIPZI_CLIENT_PYTHON'] = subprocess.check_output(
                [sys.executable, str(ROOT / 'crates/zipzi-python2/tools/connected.py'), '--install-only'],
                cwd=ROOT, env=environment, text=True, encoding='utf-8').strip()
    if domain == 'editor':
        environment.pop('ZIPZI_DEV_SESSION_DIR', None)
    return run_commands(commands, env=environment, before=prepare)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode)
    except (ValueError, OSError, RuntimeError) as error:
        print(f'dev: {error}', file=sys.stderr)
        raise SystemExit(2)
