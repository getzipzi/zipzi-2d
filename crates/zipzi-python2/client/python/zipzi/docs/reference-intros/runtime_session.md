A Session binds this client to a resident document. Session queries read committed content, including while an Edit is open. Use session.edit(label) to author candidate changes. Commit publishes into shared history; saving is a separate Job.

Use runtime.sessions.create() or attach(id) to obtain a Session. detach() releases this client binding; close() closes the host document and protects unsaved changes by default.
