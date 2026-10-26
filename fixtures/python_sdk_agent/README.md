# Python SDK agent task inputs

`tasks.py` defines the frozen G01–G08 seeds, finite task data and hidden independent
expectations. It does not include agent answers. Prompts and budgets remain in
`tools/python/python2_acceptance.py` from P3A-0.

`tools/python/python2_agent_tasks.py --help` prepares an isolated installed-wheel
packet and an adjacent private controller manifest. Agent execution must deny
access to the private directory, repository, inherited instructions and network
from its shell; the controller and independent verifier run outside that sandbox.
The fixture is not an isolation mechanism by itself. Preparing a packet does not
run an agent or prove any task passed.

`python2_agent_runner.py` executes installed packets with a single external
MCP shell. On macOS, Seatbelt denies repository/controller reads, installed-package
writes and Internet access. Connected uses a transparent IPC relay: it drops real
host replies after processing, preserving original request recovery. It records
request identity so cached replay does not consume another write's fault.

The `p3a-agent-fixture-3-observed` revision supplies parameter range, axis
keys, keyform positions and explicit output goals. TASK.md gives installed package
entry points and bounded discovery guidance; it does not supply task solutions.
Original task prompts, model, seeds and functional thresholds remain unchanged.
On 2026-10-26 the user removed all token caps: input/output/total usage is reported
after testing and never gates a run; unavailable metadata stays unknown. Time,
tool call/byte and log budgets remain enforced.
All earlier failed attempts remain in the P3A-6 report; changing the fixture or
bootstrap guidance cannot turn those failures into passes.

SDK workflow scripts run with the standalone shell command
`sdk-python work/script.py`. The external shell supplies a trusted bootstrap to
the normal installed Python, without changing SDK distribution files. A private
inherited socket sends actual public-call results and pre-close snapshots to the
external observer. Local fixture decisions run public SDK publications in the
owning Runtime process. Connected mutations use a separate host client and are
armed only during participant calls, never the observer's own reads.

`verify.py` independently reopens saved files and checks intermediate samples,
consistent page scans, stale/failing Edit recovery, Undo/Redo pixels, lifecycle
registry restoration, history and durable receipts. G06 verifies a group of at
least 40 explicitly closed Sessions in one Runtime and compares its registry at
the start of creations with the final registry; earlier failed script attempts
remain recorded. Unknown original calls remaining at interpreter exit cannot be
reported as confirmed success. Instrumentation is for cooperative SDK evaluation;
it is not isolation against hostile Python introspection.

Long shell output is paginated: each delivered result stays within 16 KiB, while
complete raw stdout and its digest remain in external logs. When a page is not
complete, its `read_output_command` retrieves the next page through the same
shell tool. Each page consumes a tool call; time/call/log limits still apply.

`python2_agent_matrix.py` freezes a cohort's CLI, harness, prompts, packages and
host and retains each fixed task/seed once. The user confirmed the configured
model/effort as `gpt-6.1-sol/high` and removed the model/service identity gate.
Optional lifecycle metadata is report-only and cannot block acceptance or dispatch. Offline invented-API
classification remains unavailable for unobserved help commands; workflow
exploration and recovery actions come from external observation.

The current `p3a-independent-verifier-4-public-runtime` shares `criteria.py` with
the supplemental reviewer: native Runtime identity survives Python client changes,
and `recover_call` preserves the logical create/close operation. Unified
`Edit.apply` and domain methods share the same ordinary aborted-write criterion.
The completed fixed cohort retains its original grader/source snapshot and score
38/42 (at least 2/3 everywhere); a source-preserving supplemental review gives
41/42. The original functional threshold already passed without that correction.
P3A acceptance is complete under the user's updated rules.

## Model/service metadata

On 2026-10-26 the user removed this acceptance gate entirely. Configured model
and reasoning effort remain recorded. Optional model/service fields already
present in CLI lifecycle events are collected for context; missing, conflicting
or changed metadata does not fail a task, stop dispatch or change a matrix score.
The private identity sidecar, pinned metadata launcher and `--identity-launcher`
option were removed. No identity probe or another 42-run cohort is required to
close P3A. Earlier raw identity results remain historical records.

Service availability preflight still requires a completed CLI turn; actual task
execution, independent verification, functional thresholds and hard stops decide
acceptance. Package/document fingerprints, isolation, recovery, finite budgets
and token usage reporting continue under their documented rules.
