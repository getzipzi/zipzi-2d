"""Process identity and graceful development-tree shutdown on Windows and POSIX.

Only a launcher's persisted identity establishes ownership. An executable match
alone is useful for detecting untracked instances, never for terminating them.
"""
from dataclasses import asdict, dataclass
import os
from pathlib import Path
import subprocess

import psutil


@dataclass(frozen=True)
class Identity:
    pid: int
    created: float
    executable: str

    @classmethod
    def capture(cls, pid):
        process = psutil.Process(pid)
        return cls(pid, process.create_time(), os.path.normcase(os.path.realpath(process.exe())))

    def process(self):
        try:
            process = psutil.Process(self.pid)
            if process.status() == psutil.STATUS_ZOMBIE:
                return None
            if Identity.capture(self.pid) != self:
                return None
            return process
        except psutil.NoSuchProcess:
            return None

    def json(self):
        return asdict(self)


def matching(executable):
    """Find untracked processes too; access errors are not proof of ownership."""
    expected = os.path.normcase(os.path.realpath(executable))
    result = []
    for process in psutil.process_iter(['pid', 'exe']):
        try:
            if process.info['exe'] and os.path.normcase(os.path.realpath(process.info['exe'])) == expected:
                result.append(Identity.capture(process.pid))
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
    return result


def launch(command, *, cwd, env, log):
    env = dict(env, PYTHONUTF8='1')
    options = {'creationflags': subprocess.CREATE_NO_WINDOW | subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else {'start_new_session': True}
    with Path(log).open('ab') as output:
        return subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                stdout=output, stderr=output, **options)


def stop(identity, *, timeout=5):
    """Stop only the verified owner and its observed descendants; never escalate.

Windows terminate is an OS process termination, not a UI close request. The P5
Editor integration must supply its explicit save/exit handshake before using
this operation for real windows. Failure leaves ownership records recoverable.
"""
    owner = identity.process()
    if owner is None:
        return
    children = owner.children(recursive=True)
    # psutil checks PID reuse before terminate; revalidate the root after traversal.
    if identity.process() is None:
        return
    for process in [*reversed(children), owner]:
        try:
            process.terminate()
        except psutil.NoSuchProcess:
            pass
    _, alive = psutil.wait_procs([*children, owner], timeout=timeout)
    alive = [process for process in alive if process.status() != psutil.STATUS_ZOMBIE]
    if alive:
        raise RuntimeError(f'Development processes did not exit: {[p.pid for p in alive]}')
