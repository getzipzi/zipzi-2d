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

P3A-6 connects the normal installed packet to the fixed runner, external page/plan
mutation and lost-response controller, and independent host/model/file verifier.
All required controller events and verifier evidence must exist before a run can
be marked passed. The recorder enforces the frozen wall/call/token/output/log
budgets and saves one concise Markdown result alongside temporary raw events.
