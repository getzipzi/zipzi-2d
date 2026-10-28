"""Checkout-local launch ownership and readiness, independent of build/UI code."""
import json
import os
from pathlib import Path
import time
import threading
import uuid

import psutil
from dev_platform import file_lock
from dev_process import Identity, launch, stop


def save(path, value):
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex)
    try:
        with temporary.open('w', encoding='utf-8') as stream:
            json.dump(value, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


class Lifecycle:
    def __init__(self, directory):
        self.directory = Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)

    def records(self):
        for path in self.directory.glob('launch-*.json'):
            try:
                value = json.loads(path.read_text(encoding='utf-8'))
            except FileNotFoundError:
                continue  # A concurrent reload retired this launch record.
            yield path, Identity(**value['identity'])

    def owners(self):
        return [identity for _, identity in self.records() if identity.process() is not None]

    def active(self):
        """A reload owns the intentional gap between stopping and starting."""
        try:
            with file_lock(self.directory / 'operation.lock', blocking=False):
                return bool(self.owners())
        except BlockingIOError:
            return True

    def _stop(self):
        records = list(self.records())
        live = [identity for _, identity in records if identity.process() is not None]
        if len(live) > 1:
            raise RuntimeError('Multiple tracked development processes; close the extra instance')
        for path, identity in records:
            stop(identity)
            path.unlink()
            path.with_suffix('.pid').unlink(missing_ok=True)
            path.with_suffix('.ready').unlink(missing_ok=True)

    def stop(self):
        with file_lock(self.directory / 'operation.lock', blocking=False):
            self._stop()

    def reload(self, build, *, cwd, env, timeout=5, publish=None, guard=None):
        """Build before stopping; a failed build preserves the running instance.

        build returns the launch argv, after publishing a checkout-private binary.
        Timeout retains the identity so another reload can recover late startup.
        A reused resident PID is never stored as owned by this launcher.
        """
        with file_lock(self.directory / 'operation.lock', blocking=False):
            command = build()
            if guard is not None:
                guard(self.owners())
            self._stop()
            if publish is not None:
                command = publish(command)
            record = self.directory / ('launch-' + uuid.uuid4().hex + '.json')
            pid_file = record.with_suffix('.pid')
            ready_file = record.with_suffix('.ready')
            environment = dict(env, ZIPZI_DEV_PID_FILE=str(pid_file),
                               ZIPZI_DEV_READY_FILE=str(ready_file))
            child = launch(command, cwd=cwd, env=environment, log=self.directory / 'run.log')
            try:
                identity = Identity.capture(child.pid)
            except psutil.NoSuchProcess:
                child.wait()
                raise RuntimeError(f'Development process exited before ownership capture: {child.returncode}')
            try:
                save(record, {'identity': identity.json()})
            except BaseException:
                stop(identity)
                child.wait(timeout=5)
                raise
            # Reap while this launcher lives without tying host lifetime to it.
            threading.Thread(target=child.wait, daemon=True, name='dev-process-reaper').start()
            deadline = time.monotonic() + timeout
            while True:
                try:
                    published = pid_file.read_text(encoding='utf-8').strip()
                except FileNotFoundError:
                    published = ''
                if published.startswith('reused:'):
                    resident = int(published.split(':', 1)[1])
                    # Wait for the forwarding launcher to finish, never adopt
                    # or terminate the resident which handled its request.
                    child.wait(timeout=max(.01, deadline - time.monotonic()))
                    if child.returncode:
                        raise RuntimeError('Resident handoff process failed')
                    record.unlink()
                    pid_file.unlink(missing_ok=True)
                    ready_file.unlink(missing_ok=True)
                    return {'reused': resident}
                if published == str(child.pid) and ready_file.exists() and ready_file.stat().st_size and identity.process() is not None:
                    return {'owned': identity}
                if child.poll() is not None:
                    raise RuntimeError(f'Development process exited before readiness: {child.returncode}')
                if time.monotonic() >= deadline:
                    raise TimeoutError('Development readiness timed out; launch record retained for recovery')
                time.sleep(.025)
