# Ownership, publication and resource lifetimes

A script first owns a Runtime connection, attaches or creates a Session, and
uses a Draft to prepare edits. Publishing changes the resident document;
saving writes a file. A successful edit does not imply a successful save.

## What each object owns

| Object | Obtain it from | End of lifetime |
| --- | --- | --- |
| Runtime | `local()` or `connect()` | `close()` or context exit releases the client and its owned resources; Local also closes its in-process host |
| Session | `runtime.sessions.create()` / `attach(id)` | `detach()` releases the attachment; `close()` requests removal of the resident document |
| Draft | `session.draft(...)` | `publish()` commits the candidate; `cancel()` discards it; ordinary accepted command failure ends it |
| Snapshot | `session.query(...)` / `draft.query(...)` | Owned immutable Python data; no `close()` and no host resource to release |
| Job | Async methods such as `save()`, `samples()` or `apply_async()` | `close()` resolves/cancels unfinished work and releases the handle; `wait()` alone does not release it |
| Blob | `runtime.blob(...)`, `session.image_blob(...)` or `job.blob()` | `close()` releases the bulk handle/storage lease; bytes already read remain ordinary Python bytes |

Runtime, Draft, Job and Blob support `with`. Session and Snapshot do not.
Use Job and Blob contexts inside the Runtime context so that their owner remains
alive during cleanup. Snapshot values can be inspected after the Runtime closes;
their old source cannot be used to call a closed host.

## A Session is an attachment to a resident document

`session.id` is a resident session ID. `session.info()['document_id']` identifies
the document; these IDs have different purposes. Use `runtime.sessions.list()`
to find and explicitly attach the intended session.

In Connected mode, publication updates the document shared with the Editor and
other attached clients. `session.detach()` or closing your Runtime leaves the
resident document and its unsaved changes on the host. It also leaves that
document's project path owned. Do not use the detached attachment for more calls;
attach again when needed.

`session.close()` requests that the host remove the resident document. Unsaved
changes and other attachments can prevent closing. Use
`session.close(discard_modified=True)` only when you intend to discard unsaved
work; it does not override other attachments. In Local mode, closing the Runtime
closes its host, so unsaved documents disappear. Save before leaving that context
when you need a file.

## Draft: prepare, inspect, then publish

A Draft starts from an exact published Version. Its commands modify an isolated
candidate. Session queries still read the published document; Draft queries read
the candidate. The candidate's `draft.source.sequence` advances after accepted
changes.

| Operation or event | Resulting state |
| --- | --- |
| `session.draft(...)` | `ready` |
| Successful `apply(...)` or `preview(...)` | `ready`, with an updated candidate source |
| Rejected gesture `preview(...)` | Last valid candidate retained, normally `ready` |
| Ordinary accepted `apply(...)` failure | `ended`; candidate changes are not published |
| Accepted `apply_async(...)` | `working`; wait for that Job before using the Draft again |
| Successful async `job.wait()` | `ready`, with the worker's returned candidate source |
| Failed/cancelled async Job resolved by `wait()` | `ended` |
| Successful `publish()` or `cancel()` | `ended` |

Admission refusals such as a busy resource are distinct from accepted command
failures. Inspect the error and `draft.state`; do not assume every exception
means the same transition. `OutcomeUnknown` needs recovery before that decision.

Normal Draft context exit publishes the ready candidate as one history step
(a no-op need not add a step). An exception cancels an unended candidate.
Leaving a `working` Draft normally raises `DRAFT_BUSY`; finish its Job inside
the Draft context. Calling `publish()` explicitly inside the context is allowed:
the later context exit sees an ended Draft and does not publish twice.

Another publication can make the base stale. Use the Version from the original
read as `expected=` when the edit depends on that read. On `STALE_SOURCE`, read
the current document and rebuild the intended edit; an old candidate is not
automatically rebased. The [edit recipe](recipes.py) shows source checks and
verification before and after publication.

## Snapshot: data stays readable; a source can become stale

A Snapshot contains `source`, `content` and an immutable `value`. Decode object
records with `snapshot.records(values.Parameter)` or another matching type.
There is no refresh operation: query again for new data.

Reuse the original `source` for all pages or related reads. A published source
identifies an exact Version; later publication, Undo or Redo invalidates it for
new queries. A Draft source also includes its candidate sequence, so another
candidate edit invalidates an earlier sequence. Already returned Python data
stays readable, but combining it with data from another source can give an
inconsistent view.

`content` identifies resident immutable content. It is not a persistent file
checksum: reopening the same file can give equal authored data with a different
content identity. Verify saved data by comparing stable IDs, fields and bytes.

## Job and Blob: wait, inspect, release

The following fragment assumes an attached `session` and an existing valid
`command`. Notice the nesting: the Job resolves before the Draft can publish.

```python
with session.draft('Async edit') as draft:
    with draft.apply_async(command) as job:
        try:
            candidate_source = job.wait(timeout=1)
        except zipzi.JobTimeout:
            candidate_source = job.wait()
    # The Draft is ready here. Normal context exit publishes it.
```

`job.status()` reads host status without applying the local completion transition.
Use `job.wait()` to receive the projected result and update the owning Draft.
`wait(timeout=...)` bounds waiting; it does not cancel the task. Catch
`JobTimeout` inside the Job context if you want to keep waiting. If it escapes,
context cleanup closes the Job and requests cancellation of unfinished work.
Cancellation does not undo an external publication that has already happened.

For readback, keep the Job alive while consuming its Blob:

```python
from zipzi.docs.recipes import read_all

with session.samples(request) as job:
    job.wait()
    with job.blob() as blob:
        payload = read_all(blob)
```

`request` above is a `values.SampleRequest`. A Blob's `length` is its total byte
length, and `info` carries metadata for readbacks. Each read or append transfers
at most 1 MiB; multiple smaller reads are valid. Upload blobs must be completely
written and sealed before use in an image edit. Releasing a Blob does not undo
an image already accepted into a Draft.

## Saving is a separate Job

Publish first, then save while the Runtime and Session remain available:

```python
with session.save(destination) as job:
    receipt = job.wait()
print(receipt['published'], receipt['durable'], receipt['warnings'])
```

`destination` is an absolute `pathlib.Path` on the host. Saving a new destination
without a fingerprint refuses an existing file. After saving/opening a tracked
project, `session.save()` updates that path using its known fingerprint. A save
captures content; later edits can leave the resident document dirty even though
that save succeeded. Check publication, durability and warnings separately.

## Lost responses: preserve ownership and recover the original call

`OutcomeUnknown` means the original call may have executed. It differs from a
confirmed command failure and from `JobTimeout`. Do not issue the same mutation
again under a new request ID. Recover using the caught exception:

```python
draft = session.draft('Edit')
try:
    result = draft.apply(command)
except zipzi.OutcomeUnknown as error:
    result = zipzi.recover_call(error)
# Only continue here after the original call is resolved successfully.
draft.publish()
```

This fragment shows one recovery attempt. Recovery can itself raise another
`OutcomeUnknown`; retain that new exception and the same owners, and recover
again before doing more work. If recovery resolves to a confirmed failure,
handle its code and inspect the owner state instead of publishing blindly.

When `OutcomeUnknown` escapes a Runtime, Draft, Job or Blob context, its context
manager deliberately preserves ownership instead of performing ordinary cleanup.
Keep references to those objects and explicitly release them after recovery.
If the lost reply was for context-exit publication, recovery resolves that
publication; it does not resume the departed Python `with` block. Do not publish
again. Likewise, recover a lost close response before deciding whether more
cleanup is needed.

For complete task examples, continue with the [workflow guide](guide.md) and
[recipes](recipes.py). Exact method signatures are in the
[generated API reference](reference/index.md).
