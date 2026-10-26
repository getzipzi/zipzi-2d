"""P3A-0 agent experiment configuration and validation, before any agent runs.

The runner and isolated packet/fixture implementation belong to P3A-5. This
module freezes their inputs and enforcement requirements; it never starts a
model or labels configuration validation as task success. On 2026-10-26 the user
removed token limits: usage is reported after a run and never gates success.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from python2_agent_identity import IDENTITY_POLICY

PROMPTS = {
    "G01": "从提供的多个驻留会话中，找到任务数据指定的文档和所属 Part。对象名称有重复，请按所属关系选择，读取并仅修改指定对象。先检查执行宿主能力；条件不足时准确报告。最后用正式读取验证目标和其他同名对象，按任务数据要求保存。",
    "G02": "用任务数据中的非连续顶点 ID 创建网格、参数和绑定。三角形引用规则和坐标约定请查随包文档。先读取候选并采样指定 CPU pose，核对任务要求后提交，再读取正式结果并保存。",
    "G03": "发现任务文档中的全部对象并按任务数据更新选定对象。集合超过一页，且外部 fixture 会在读取页间修改一次文档。处理来源变化，保证最终清单来自一致版本；验证完整对象集合和指定更新后保存。",
    "G04": "按任务数据规划指定修改，读取并解释成本和损失，然后应用。外部 fixture 会使一次规划来源失效，并提供一次可修正的普通写入失败。根据随包错误指引处理，完成目标并验证正式内容和历史。",
    "G05": "使用任务数据给出的 RGBA 内容准备、接纳和替换图片；验证 Undo/Redo、保存并关闭会话，重新打开后核对像素。执行宿主的路径由 fixture 提供。按文档正确等待 IO 和释放资源。",
    "G06": "按任务数据连续完成 40 次会话创建、编辑、保存或显式丢弃和关闭。使用宿主默认预算。处理未保存关闭拒绝，最后验证会话注册表和保存重开结果。对大数组使用摘要，保留可确认的结果。",
    "G07": "完成任务数据指定的创建和修改。Connected 宿主会丢弃一次创建响应和一次应用响应。按随包文档恢复原调用，核对返回类型、对象集合和历史；结果尚未确认时继续保留可恢复状态。",
    "G08": "完成任务数据指定的编辑、提交和保存。Connected 宿主会丢弃一次 commit 响应和一次 Job 查询终态响应。处理原调用的恢复、任务等待及上下文状态，验证实际正式结果、历史和保存回执。",
}
COMMON_PROMPT = """你在隔离的任务目录执行 zipzi Python SDK 验收任务。
仅使用正常安装包、随包文档、task.json 和 fixture 提供的可访问输入/宿主绝对路径。
先用离线帮助发现契约，再选择会话和对象，读取、编辑/必要时规划、等待、验证、保存并释放。
不要猜测 ID、方法、字段或能力；不确定时查询随包帮助。不要访问仓库源码或私有 SDK 接口。
按任务资源预算工作，不执行无界重试。记录错误和恢复动作；只对已确认的结果报告成功。
最后输出结果清单和可复核的产物路径，说明任何尚未确认的状态。
"""
BUDGET = {"wall_seconds": 900, "tool_calls": 160, "seconds_per_tool_call": 60,
    "tool_output_bytes_per_call": 16384, "log_bytes": 8388608,
    "enforcement": "runner records every call; kills over wall budget; tool call/time/byte or log excess fails run; token usage is report-only; never truncate evidence into success"}
TOKEN_USAGE_POLICY = {"mode": "report_only", "limits": None,
    "fields": ["input_tokens", "output_tokens", "total_tokens"],
    "missing_usage": "unavailable; never zero-filled or treated as a run failure"}
FINAL_RESPONSE_SCHEMA = {"type":"object","properties":{
    "status":{"type":"string","enum":["completed","unconfirmed","failed"]},
    "artifacts":{"type":"array","items":{"type":"string"}},
    "unconfirmed":{"type":"array","items":{"type":"string"}}},
    "required":["status","artifacts","unconfirmed"],"additionalProperties":False}


def token_usage(records: list[dict]) -> dict[str, int | None]:
    """Summarize actual runner usage; missing or malformed counts stay unknown."""
    def total(key: str) -> int | None:
        values = [record.get(key) for record in records]
        if not values or any(type(value) is not int or value < 0 for value in values):
            return None
        return sum(values)
    inputs, outputs = total("input_tokens"), total("output_tokens")
    return {"input_tokens": inputs, "output_tokens": outputs,
        "total_tokens": None if inputs is None or outputs is None else inputs + outputs}


def digest(value: object) -> str:
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def configuration() -> dict:
    executable = shutil.which("codex")
    if executable is None:
        raise ValueError("A concrete agent runner version must be identified before freezing")
    runner = Path(executable).resolve()
    version = subprocess.check_output([str(runner), "--version"], text=True).strip()
    tasks = []
    for index, (task, prompt) in enumerate(PROMPTS.items(), 1):
        tasks.append({"id": task, "environments": ["local", "connected"] if index <= 6 else ["connected"],
            "prompt": prompt, "prompt_sha256": digest(COMMON_PROMPT + prompt),
            "seeds": [314159 + index * 100 + repeat for repeat in range(3)],
            "fixture": {"version": "p3a-agent-fixture-3-observed", "normal_wheels_only": True,
                "runtime_budget": "shared Rust defaults; no increases", "inputs": "task.json + host-accessible paths",
                "controls": {"G03": "publish after first page", "G04": "invalidate plan then domain failure",
                    "G07": "drop create/apply response", "G08": "drop commit/terminal job response"}.get(task, "none")},
            "verifier": {"version": "p3a-independent-verifier-4-public-runtime", "input": "host model/pixels/version/history/files",
                "agent_claims_are_evidence": False}})
    result = {"schema": 2, "status": "configuration_frozen_tasks_not_run",
        "model": {"id": "gpt-6.1-sol", "reasoning_effort": "high", "fallback": False},
        "runner": {"kind": "codex exec", "version": version, "executable": str(runner),
            "executable_sha256": hashlib.sha256(runner.read_bytes()).hexdigest(),
            "arguments": ["exec", "--json", "--ignore-user-config", "--ignore-rules", "--ephemeral", "--skip-git-repo-check", "--model", "gpt-6.1-sol", "-c", 'model_reasoning_effort="high"'],
            "environment": "isolated packet; no repository, inherited conversation, user skills or answers"},
        "common_prompt": COMMON_PROMPT, "tasks": tasks,
        "tools": {"allowed": ["shell"], "shell_root": "isolated packet", "network": "disabled inside packet",
            "output_delivery": "paged responses within the byte budget; opaque read-output cursor; full raw output retained",
            "readable": ["installed distributions", "package documents/recipes", "task.json", "fixture inputs"],
            "writable": ["task output directory", "host fixture path through SDK"],
            "private_fault_controls": "external fixture only, unavailable to agent"},
        "budget_per_run": dict(BUDGET),
        "token_usage_policy": dict(TOKEN_USAGE_POLICY, fields=list(TOKEN_USAGE_POLICY["fields"])),
        "identity_policy": json.loads(json.dumps(IDENTITY_POLICY)),
        "final_response_schema": FINAL_RESPONSE_SCHEMA,
        "thresholds": {"runs_per_environment": 3, "environments": 14, "runs": 42, "minimum_successes": 38,
            "minimum_successes_per_environment": 2,
            "hard_stop": ["duplicate_write", "wrong_object_modified", "silent_partial_publication", "unknown_reported_as_success"],
            "deterministic_gates": "D01-D08 and C01-C08 must all pass independently"},
        "required_run_record": ["task", "environment", "seed", "model", "runner_version", "package_hashes", "document_hashes", "prompt_hash", "success", "failure_reason", "tool_calls", "exploration_calls_before_first_correct_call", "invented_api_calls", "recovery_actions", "input_tokens", "output_tokens", "total_tokens", "elapsed_seconds", "log_bytes", "independent_verifier_result"],
    }
    result["configuration_sha256"] = digest(result)
    return result


def validate(config: dict) -> None:
    content = dict(config)
    recorded = content.pop("configuration_sha256", None)
    if recorded != digest(content):
        raise ValueError("Frozen acceptance configuration changed")
    if config["common_prompt"] != COMMON_PROMPT or config["budget_per_run"] != BUDGET:
        raise ValueError("Frozen common prompt or per-run budgets changed")
    if config.get("schema") != 2 or config.get("token_usage_policy") != TOKEN_USAGE_POLICY:
        raise ValueError("Token usage must be report-only without token limits")
    if config.get('final_response_schema')!=FINAL_RESPONSE_SCHEMA:raise ValueError('Final confirmation schema changed')
    if config.get('identity_policy')!=IDENTITY_POLICY:raise ValueError('Service metadata must remain report-only')
    if not Path(config['runner'].get('executable','')).is_absolute():raise ValueError('Runner executable must be frozen by absolute path')
    expected = {(task, environment) for task in PROMPTS for environment in
                (["local", "connected"] if task <= "G06" else ["connected"])}
    actual = [(task["id"], environment) for task in config["tasks"] for environment in task["environments"]]
    if len(actual) != 14 or set(actual) != expected:
        raise ValueError("All fourteen task environments must be present once")
    for task in config["tasks"]:
        if task["prompt"] != PROMPTS[task["id"]] or task["prompt_sha256"] != digest(COMMON_PROMPT + task["prompt"]):
            raise ValueError("Task prompt drift")
        index = int(task["id"][1:])
        if task["seeds"] != [314159 + index * 100 + repeat for repeat in range(3)]:
            raise ValueError("Fixed fixture seeds changed")
    limits = config["thresholds"]
    if limits != {"runs_per_environment": 3, "environments": 14, "runs": 42, "minimum_successes": 38,
            "minimum_successes_per_environment": 2,
            "hard_stop": ["duplicate_write", "wrong_object_modified", "silent_partial_publication", "unknown_reported_as_success"],
            "deterministic_gates": "D01-D08 and C01-C08 must all pass independently"}:
        raise ValueError("Experiment exit thresholds changed")
    budget = config["budget_per_run"]
    if any(type(value) is not int or value <= 0 for key, value in budget.items() if key != "enforcement"):
        raise ValueError("Every measured budget must be a finite positive integer")
    if config["model"]["id"] != "gpt-6.1-sol" or config["model"]["fallback"] or not config["runner"]["version"]:
        raise ValueError("Model/runner must be explicitly pinned without fallback")
