# Authoring with Python

Use `zipzi.local()` with the matching native adapter, or `zipzi.connect()` to
attach to a private v4 Editor endpoint. `runtime.sessions.list()` returns
metadata records. Pass a record's `info["session"]` to `runtime.sessions.attach`
to edit a resident document. `session.detach()` releases the attachment.
`session.close()` closes the document and checks for unsaved changes and other
attached clients.

## Query and publish

Call `session.query()` with a generated query from `zipzi.values`. The result
carries the exact source and content ID. To continue a paginated collection
query, pass its `next_offset` and original source. A new publication makes the
old source stale. Start again from the first page if the model changes.

Open a Draft from the current Version. Use `draft.apply(*commands)` for one
validated batch, then publish. A successful Draft context manager publishes
automatically. An ordinary command failure ends the candidate. Preview
rejection preserves the last valid candidate. Draft commands and candidate
queries share the current sequence. Undo and Redo advance the publication
revision, even when they restore earlier content.

## Work and recovery

Project open, save, export and sampling return Jobs. A timeout leaves the Job
available for a later wait. Release finished Jobs and bulk Blobs with context
managers. Image uploads use sealed Blobs with chunks of up to 1 MiB. Save
receipts distinguish publication from durability warnings.

If a transport failure raises `OutcomeUnknown`, pass the exception to
`zipzi.recover_call()`. The client recovers the original request and result
using its request ID. Continue editing after that call resolves.

See the [recipe](recipes.py) for a complete edit, query and save.
