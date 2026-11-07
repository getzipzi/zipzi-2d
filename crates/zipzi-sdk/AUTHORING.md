# Authoring contracts

`Snapshot` pairs a frozen Core model with the immutable image versions it
references. Use `AuthoringSession::read_snapshot` to capture the published version,
or `Draft::read_snapshot` to read a candidate at its current sequence.

A typed `Command` uses an `ObjectAddress` for an authored object. Positions
selected by a query belong to that query's full source. A successful batch
validates the complete candidate and advances its sequence. A command failure
ends the Draft. Specialized previews prepare an isolated result and install it
only on success.

Prepare and publish the same candidate. Publication checks its base Version and
adds a single history step for its net writes. Undo and Redo restore previous
snapshots and advance the publication revision. When two Drafts share a base,
the first publication advances the Version and the second becomes stale.

Domain rules still govern identities, ownership and order. Copies allocate
identities for owned content and keep shared references. Topology changes
transfer dependent shapes using the domain policies. Owner/child locks and
dependency checks apply to commands and typed patches.

See the [SDK workflow](README.md) for project I/O and runnable examples, and
the [implementation plan](../../docs/archive/plans/authoring-architecture.md) for
model validation, conversion, publication and resource ownership details.
