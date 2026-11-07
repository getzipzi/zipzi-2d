"""Run independent v4 SDK agents against installed wheels and bundled help.

macOS Seatbelt limits the sole MCP shell to its packet, installed interpreter and
system runtime. The agent never receives repository context or verifier criteria.
Token usage is report-only.
"""

from __future__ import annotations

import argparse
from collections.abc import Mapping
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import inspect
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time

import psutil

from python_agent_output import OutputPages
from python_agent_shell import policy

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = Path(__file__).resolve()
LIMITS = {"wall_seconds": 900, "tool_calls": 160, "seconds_per_call": 60}


class SampleReadbacks:
    """Reassemble only observed sample reads, without performing extra SDK calls."""

    def __init__(self):
        self.jobs = {}
        self.blobs = {}

    def observe(self, event, raw):
        path, owner, result = event["path"], event["owner"], event.get("result")
        if path == "Session.samples":
            self.jobs[result["id"]] = owner
        elif path == "Job.blob" and owner in self.jobs:
            self.blobs[result["id"]] = {
                "length": result["length"],
                "parts": {},
                "session": self.jobs[owner],
            }
        elif path == "Job.close":
            self.jobs.pop(owner, None)
        elif path == "Blob.close":
            self.blobs.pop(owner, None)
        elif path == "Blob.read" and owner in self.blobs:
            state = self.blobs[owner]
            offset = event["arguments"]["offset"]
            if len(raw) > len(state["parts"].get(offset, b"")):
                state["parts"][offset] = raw
            covered = 0
            for start, chunk in sorted(state["parts"].items()):
                if start > covered:
                    return
                covered = max(covered, start + len(chunk))
            if covered != state["length"]:
                return
            payload = bytearray(covered)
            for start, chunk in state["parts"].items():
                payload[start : start + len(chunk)] = chunk
            complete = {
                "length": covered,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "session": state["session"],
            }
            try:
                complete["json"] = json.loads(payload)
            except (ValueError, UnicodeDecodeError):
                pass
            event["readback"] = complete
            del self.blobs[owner]


# Trusted code is embedded in the read-only workflow runner. It observes public
# calls and actual returned bytes; no verifier criteria are exposed to participants.
BOOTSTRAP = (
    inspect.getsource(SampleReadbacks)
    + r"""
import functools, hashlib, inspect, json, runpy, socket, sys
from collections.abc import Mapping
from pathlib import Path
import zipzi
from zipzi._value_codec import Value
channel = socket.socket(fileno=int(sys.argv[1]))
script = Path(sys.argv[2]).resolve()
readbacks = SampleReadbacks()

def value(v):
    if isinstance(v, bytes):
        result = {'length':len(v), 'sha256':hashlib.sha256(v).hexdigest()}
        try: result['json'] = json.loads(v)
        except (ValueError, UnicodeDecodeError): pass
        return result
    if isinstance(v, Value): return value(v.to_wire())
    if isinstance(v, zipzi.Snapshot):
        return {'source':value(v.source), 'content':v.content, 'value':value(v.value)}
    if isinstance(v, zipzi.Blob):
        return {'type':'Blob', 'id':v.id, 'length':v.length}
    if isinstance(v, Mapping): return {str(k):value(x) for k,x in v.items()}
    if isinstance(v, (tuple,list)): return [value(x) for x in v]
    if isinstance(v, Path): return str(v)
    if v is None or isinstance(v,(str,int,float,bool)): return v
    return {'type':type(v).__name__, 'id':getattr(v,'id',None)}

def wrap(cls, name, original):
    signature = inspect.signature(original)
    @functools.wraps(original)
    def observed(self, *args, **kwargs):
        bound = signature.bind(self,*args,**kwargs)
        arguments = {k:value(v) for k,v in bound.arguments.items() if k != 'self'}
        event = {'path':cls.__name__+'.'+name, 'arguments':arguments,
                 'owner':getattr(self,'id',None)}
        try:
            result = original(self,*args,**kwargs)
        except BaseException as error:
            event['error'] = {'code':getattr(error,'code',type(error).__name__), 'message':str(error)}
            raise
        else:
            event['result'] = value(result)
            readbacks.observe(event, result)
            return result
        finally:
            if isinstance(self,zipzi.Draft):
                event['state'] = self.state
                event['source'] = value(self.source)
            channel.sendall((json.dumps(event)+'\n').encode())
    return observed

for cls in (zipzi.Runtime,zipzi.Sessions,zipzi.Session,zipzi.Draft,zipzi.Job,zipzi.Blob):
    for name,method in list(vars(cls).items()):
        if not name.startswith('_') and inspect.isfunction(method):
            setattr(cls,name,wrap(cls,name,method))
try:
    sys.argv = [str(script),*sys.argv[3:]]
    runpy.run_path(str(script),run_name='__main__')
finally:
    channel.close()
"""
)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def interpreter(packet: Path) -> Path:
    return packet / "environment/bin/python"


def clean_environment() -> dict[str, str]:
    return {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("ZIPZI_") and key not in {"PYTHONPATH", "PYTHONHOME"}
    }


def stop_participant(process, private: Path) -> None:
    """Stop dispatch first, then reap the separately owned workflow group."""
    if process is not None:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)
    active = private / "active-shell-pgid"
    if not active.exists():
        return
    pgid = int(active.read_text())
    try:
        leader = psutil.Process(pgid)
        children = leader.children(recursive=True)
        os.killpg(pgid, signal.SIGKILL)
        _, alive = psutil.wait_procs([leader, *children], timeout=5)
        if any(p.is_running() and p.status() != psutil.STATUS_ZOMBIE for p in alive):
            raise RuntimeError("SDK workflow did not terminate")
    except (ProcessLookupError, psutil.NoSuchProcess):
        pass
    active.unlink(missing_ok=True)


def install_workflow_runner(packet: Path) -> None:
    """Provide a real command so normal multiline shell scripts stay observed."""
    runner = packet / "environment/bin/sdk-python"
    source = (
        "#!" + str(interpreter(packet)) + " -I\n"
        "import os,sys\nfrom pathlib import Path\n"
        "script=Path(sys.argv[1]).resolve()\n"
        'work=Path(__file__).resolve().parents[2]/"work"\n'
        'if not script.is_relative_to(work): raise ValueError("Workflow scripts must be under work/")\n'
        'descriptor=os.environ["ZIPZI_AGENT_OBSERVER_FD"]\n'
        'os.execv(sys.executable,[sys.executable,"-I","-c",'
        + repr(BOOTSTRAP)
        + ",descriptor,str(script),*sys.argv[2:]])\n"
    )
    runner.write_text(source)
    runner.chmod(0o755)


def execute(command: str, packet: Path, private: Path, index: int) -> dict:
    temporary = packet / "work/tmp"
    temporary.mkdir(exist_ok=True)
    base = Path(
        subprocess.check_output(
            [str(interpreter(packet)), "-I", "-c", "import sys;print(sys.base_prefix)"],
            text=True,
            env=clean_environment(),
        ).strip()
    ).resolve()
    profile = private / "shell.sb"
    profile.write_text(policy(packet, base, temporary))
    env = {
        "PATH": str(packet / "environment/bin") + ":/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": str(temporary) + "/",
        "TMPPREFIX": str(temporary / "shell"),
        "PYTHONDONTWRITEBYTECODE": "1",
        "PYTHONNOUSERSITE": "1",
        "LC_ALL": "en_US.UTF-8",
    }
    executable = ["/bin/zsh", "-f", "-c", command]
    channel, child = socket.socketpair()
    pass_fds = (child.fileno(),)
    env["ZIPZI_AGENT_OBSERVER_FD"] = str(child.fileno())

    def receive():
        with (
            channel,
            channel.makefile("r") as stream,
            (private / "observations.ndjson").open("a") as log,
        ):
            for line in stream:
                event = json.loads(line)
                event["shell_call"] = index
                log.write(json.dumps(event) + "\n")
                log.flush()

    receiver = threading.Thread(target=receive, daemon=True)
    receiver.start()
    started = time.monotonic()
    try:
        with subprocess.Popen(
            ["/usr/bin/sandbox-exec", "-f", str(profile), *executable],
            cwd=packet,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            start_new_session=True,
            pass_fds=pass_fds,
        ) as process:
            child.close()
            (private / "active-shell-pgid").write_text(str(process.pid))
            try:
                output, _ = process.communicate(timeout=LIMITS["seconds_per_call"])
                code = process.returncode
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                output, _ = process.communicate()
                code = 124
            finally:
                (private / "active-shell-pgid").unlink(missing_ok=True)
    finally:
        receiver.join(timeout=5)
        if receiver.is_alive():
            (private / "observer-error").write_text("Observer channel did not close")
    return {
        "command": command,
        "returncode": code,
        "elapsed_seconds": time.monotonic() - started,
        "output": output.decode(errors="replace"),
    }


def serve(packet: Path, private: Path) -> None:
    pages = OutputPages()
    calls = 0
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        try:
            method = request["method"]
            if method == "initialize":
                result = {
                    "protocolVersion": request["params"]["protocolVersion"],
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "sdk-v4-shell", "version": "1"},
                }
            elif method == "tools/list":
                result = {
                    "tools": [
                        {
                            "name": "shell",
                            "description": "Shell isolated to this installed SDK packet. Write scripts in work/, results in output/. "
                            "Run ALL SDK workflows as sdk-python work/script.py; this command is installed on PATH. "
                            "Read offline help using environment/bin/python -I. Output pages are <=16 KiB; "
                            "use read_output_command for continuation. Each command has 60 seconds.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {"command": {"type": "string"}},
                                "required": ["command"],
                                "additionalProperties": False,
                            },
                        }
                    ]
                }
            elif method == "tools/call":
                calls += 1
                if calls > LIMITS["tool_calls"]:
                    (private / "budget-failed").write_text("tool_calls")
                    raise RuntimeError("Tool call budget exceeded")
                command = request["params"]["arguments"]["command"]
                if command.startswith("read-output "):
                    page = pages.read(command.split(None, 1)[1])
                else:
                    event = execute(command, packet, private, calls)
                    with (private / "shell-events.ndjson").open("a") as log:
                        log.write(json.dumps(event) + "\n")
                    page = pages.first(event)
                result = {
                    "content": [
                        {"type": "text", "text": json.dumps(page, ensure_ascii=False)}
                    ],
                    "isError": page["returncode"] != 0,
                }
            elif method == "ping":
                result = {}
            else:
                raise ValueError("Unsupported MCP method " + method)
            response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        except Exception as error:
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32603, "message": str(error)},
            }
        print(json.dumps(response), flush=True)


def prepare(output: Path, wheels: list[Path]) -> Path:
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    for name in ("work", "output", "host"):
        (output / name).mkdir(mode=0o700)
    subprocess.run(
        ["uv", "venv", "--python", "3.14", str(output / "environment")], check=True
    )
    install_workflow_runner(output)
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            str(interpreter(output)),
            "--no-index",
            *[str(p.resolve()) for p in wheels],
        ],
        check=True,
    )
    # Host credentials live in the packet; the host executable and judge remain outside.
    return output


def installed_identity(python: Path) -> dict:
    code = """import hashlib,importlib.util,json,pathlib
result={}
for module in ('zipzi','zipzi_native'):
    spec=importlib.util.find_spec(module)
    if spec is None: continue
    root=pathlib.Path(spec.origin).parent
    for file in root.rglob('*'):
        if file.is_file() and '__pycache__' not in file.parts:
            result[module+'/'+file.relative_to(root).as_posix()]=hashlib.sha256(file.read_bytes()).hexdigest()
print(json.dumps(result,sort_keys=True))
"""
    return json.loads(
        subprocess.check_output(
            [str(python), "-I", "-c", code], env=clean_environment(), text=True
        )
    )


def run(
    output: Path, task: str, environment: str, wheels: list[Path], host: Path
) -> dict:
    packet = output.resolve()
    private = packet.parent / (packet.name + ".controller")
    private.mkdir(parents=True, exist_ok=False)
    prepare(
        packet,
        wheels
        if environment == "local"
        else [p for p in wheels if "zipzi_native" not in p.name],
    )
    judge_env = private / "environment"
    subprocess.run(["uv", "venv", "--python", "3.14", str(judge_env)], check=True)
    judge_python = judge_env / "bin/python"
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            str(judge_python),
            "--no-index",
            *[str(p.resolve()) for p in wheels],
        ],
        check=True,
    )
    package_before = {p.name: sha(p) for p in wheels}
    installed_before = installed_identity(interpreter(packet))
    (private / "installed-files.json").write_text(
        json.dumps(installed_before, indent=2) + "\n"
    )
    fixture = ROOT / "tools/python/python_agent_v4_fixture.py"
    source_before = {"harness": sha(SCRIPT), "fixture": sha(fixture), "host": sha(host)}
    agent = shutil.which("codex")
    if agent is None:
        raise RuntimeError("Codex CLI is required")
    host_process = None
    process = None
    try:
        endpoint = None
        if environment == "connected":
            env = clean_environment()
            env.update(
                ZIPZI_RUNTIME_DIR=str(packet / "host"), ZIPZI_RUNTIME_PROFILE="agent"
            )
            log = (private / "host.log").open("wb")
            host_process = subprocess.Popen(
                [str(host.resolve())], env=env, stdout=log, stderr=subprocess.STDOUT
            )
            log.close()
            endpoint = packet / "host/agent.sock"
            deadline = time.monotonic() + 30
            while not endpoint.exists():
                if host_process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Fixture host failed to start")
                time.sleep(0.05)
        # Setup/verifier must also use the installed distribution, never source imports.
        command = [
            str(judge_python),
            "-I",
            str(fixture),
            "setup",
            str(packet),
            str(private),
            task,
            environment,
        ]
        if endpoint is not None:
            command += ["--endpoint", str(endpoint)]
        subprocess.run(
            command,
            check=True,
            env=clean_environment(),
            stdout=(private / "setup.log").open("w"),
        )
        instructions = (
            "You are an independent SDK user. Complete TASK.md using only the sdk_shell shell tool, "
            "installed zipzi packages and bundled documentation. No repository, Internet, private "
            "interfaces or outside files. Write work scripts in work/ and artifacts in output/. "
            "Every workflow that opens a Runtime MUST run as sdk-python work/script.py. This command is on PATH and supports normal multiline shell commands. "
            "Offline help may use environment/bin/python -I. Read the installed guide first. "
            "Discover needed API details yourself. Do not edit installed packages. Verify actual "
            "results; report unresolved outcomes honestly. This is a fresh task, with no prior conversation."
        )
        (private / "instructions.md").write_text(instructions)
        task_data = json.loads((packet / "task.json").read_text())
        prompt = (
            task_data["goal"]
            + "\nRead task.json for concrete inputs. Limits: "
            + json.dumps(LIMITS)
            + ". No token limits; usage is report-only."
        )
        (packet / "TASK.md").write_text(prompt)
        arguments = [
            agent,
            "exec",
            "--ignore-user-config",
            "--ignore-rules",
            "--ephemeral",
            "--skip-git-repo-check",
            "--json",
            "--sandbox",
            "read-only",
            "--cd",
            str(packet),
            "-c",
            "features.shell_tool=false",
            "-c",
            "features.unified_exec=false",
            "-c",
            "features.multi_agent=false",
            "-c",
            "features.apps=false",
            "-c",
            "project_doc_max_bytes=0",
            "-c",
            "model_instructions_file=" + json.dumps(str(private / "instructions.md")),
            "-c",
            "mcp_servers.sdk_shell.command=" + json.dumps(sys.executable),
            "-c",
            "mcp_servers.sdk_shell.args="
            + json.dumps([str(SCRIPT), "shell", str(packet), str(private)]),
            "-c",
            "mcp_servers.sdk_shell.startup_timeout_sec=30",
            "-c",
            "mcp_servers.sdk_shell.tool_timeout_sec=65",
            "-c",
            'mcp_servers.sdk_shell.default_tools_approval_mode="approve"',
            prompt,
        ]
        (private / "command.json").write_text(json.dumps(arguments, indent=2) + "\n")
        # Demonstrate isolation rather than assuming the policy works.
        denial = execute(
            "cat " + shlex.quote(str(ROOT / "AGENTS.md")), packet, private, 0
        )
        if denial["returncode"] == 0:
            raise RuntimeError("Repository isolation probe unexpectedly succeeded")
        (private / "isolation.json").write_text(json.dumps(denial, indent=2))
        started = time.monotonic()
        with (
            (private / "agent.ndjson").open("wb") as log,
            (private / "agent.stderr").open("wb") as err,
        ):
            process = subprocess.Popen(
                arguments,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=err,
                cwd=packet,
                start_new_session=True,
            )
            while process.poll() is None:
                if (
                    time.monotonic() - started > LIMITS["wall_seconds"]
                    or (private / "budget-failed").exists()
                ):
                    (private / "budget-failed").write_text("run_limit")
                    stop_participant(process, private)
                    break
                time.sleep(0.5)
        stop_participant(process, private)
        agent_exit = process.returncode
        process = None
        verification = subprocess.run(
            [
                str(judge_python),
                "-I",
                str(fixture),
                "verify",
                str(packet),
                str(private),
            ],
            env=clean_environment(),
            capture_output=True,
            text=True,
            timeout=60,
        )
        (private / "verify.log").write_text(verification.stdout + verification.stderr)
        try:
            judgement = json.loads(verification.stdout)
        except ValueError:
            judgement = {
                "verified": False,
                "error": verification.stdout + verification.stderr,
            }
        events = [
            json.loads(line)
            for line in (private / "agent.ndjson").read_text().splitlines()
            if line.startswith("{")
        ]
        usage = [e["usage"] for e in events if e.get("type") == "turn.completed"]
        calls_path = private / "shell-events.ndjson"
        calls = (
            [json.loads(line) for line in calls_path.read_text().splitlines()]
            if calls_path.exists()
            else []
        )
        forbidden = [
            e
            for e in events
            if (item := e.get("item", {})).get("type")
            in {"command_execution", "web_search"}
            or (
                item.get("type") == "mcp_tool_call"
                and item.get("server") not in {None, "sdk_shell"}
            )
        ]
        unchanged = package_before == {p.name: sha(p) for p in wheels}
        installed_unchanged = installed_before == installed_identity(
            interpreter(packet)
        )
        source_unchanged = source_before == {
            "harness": sha(SCRIPT),
            "fixture": sha(fixture),
            "host": sha(host),
        }
        passed = bool(
            judgement.get("verified")
            and agent_exit == 0
            and unchanged
            and installed_unchanged
            and source_unchanged
            and not forbidden
            and not (private / "budget-failed").exists()
            and not (private / "observer-error").exists()
        )
        result = {
            "task": task,
            "environment": environment,
            "passed": passed,
            "verification": judgement,
            "agent_exit": agent_exit,
            "tool_calls": len(calls),
            "failed_shell_calls": sum(c["returncode"] != 0 for c in calls),
            "elapsed_seconds": time.monotonic() - started,
            "usage": usage,
            "token_limits": None,
            "wheels": package_before,
            "wheel_inputs_unchanged": unchanged,
            "installed_files_unchanged": installed_unchanged,
            "harness_and_host_unchanged": source_unchanged,
            "configured_model": "CLI default, no model or effort override",
            "forbidden_tools": forbidden,
            "cli_version": subprocess.check_output(
                [agent, "--version"], text=True
            ).strip(),
            "harness_sha256": sha(SCRIPT),
            "fixture_sha256": sha(fixture),
        }
        (private / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        return result
    finally:
        stop_participant(process, private)
        if host_process is not None:
            host_process.terminate()
            try:
                host_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                host_process.kill()
                host_process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    shell = commands.add_parser("shell")
    shell.add_argument("packet", type=Path)
    shell.add_argument("private", type=Path)
    task = commands.add_parser("run")
    task.add_argument("--output", type=Path, required=True)
    task.add_argument("--task", choices=["A01", "A02", "A03", "A04"], required=True)
    task.add_argument("--environment", choices=["local", "connected"], required=True)
    task.add_argument("--wheel", type=Path, action="append", required=True)
    task.add_argument(
        "--host", type=Path, default=ROOT / "target/python/local_host_fixture"
    )
    suite = commands.add_parser("suite")
    suite.add_argument("--output", type=Path, required=True)
    suite.add_argument("--wheel", type=Path, action="append", required=True)
    suite.add_argument(
        "--host", type=Path, default=ROOT / "target/python/local_host_fixture"
    )
    suite.add_argument("--parallel", type=int, choices=[1, 2, 3], default=3)
    suite.add_argument("--environment", choices=["local", "connected"], action="append")
    args = parser.parse_args()
    if args.command == "shell":
        serve(args.packet.resolve(), args.private.resolve())
    elif args.command == "run":
        result = run(args.output, args.task, args.environment, args.wheel, args.host)
        print(json.dumps(result, indent=2))
        if not result["passed"]:
            raise SystemExit(1)
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        summary_path = args.output / "summary.json"
        if summary_path.exists():
            parser.error("Evidence already exists; select a new output directory")
        results = []
        environments = args.environment or ["local", "connected"]
        with ThreadPoolExecutor(max_workers=args.parallel) as pool:
            pending = {
                pool.submit(
                    run,
                    args.output / (task + "-" + env),
                    task,
                    env,
                    args.wheel,
                    args.host,
                ): (task, env)
                for task in ("A01", "A02", "A03", "A04")
                for env in environments
            }
            for future in as_completed(pending):
                task, env = pending[future]
                try:
                    result = future.result()
                except Exception as error:
                    result = {
                        "task": task,
                        "environment": env,
                        "passed": False,
                        "infrastructure_error": repr(error),
                    }
                results.append(result)
                summary = {
                    "suite": "v4-installed-agent-1",
                    "passed": sum(r["passed"] for r in results),
                    "completed": len(results),
                    "planned": 4 * len(environments),
                    "results": sorted(
                        results, key=lambda r: (r["task"], r["environment"])
                    ),
                }
                summary_path.write_text(json.dumps(summary, indent=2) + "\n")
                print(task, env, "passed" if result["passed"] else "FAILED", flush=True)
        if not all(r["passed"] for r in results):
            raise SystemExit(1)


if __name__ == "__main__":
    main()
