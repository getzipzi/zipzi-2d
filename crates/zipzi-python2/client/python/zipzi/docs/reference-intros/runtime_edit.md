An Edit owns a candidate based on a document version. Read through edit.meshes to see its writes. A normal with-block exit commits and waits; an exceptional exit cancels. Explicit commit() returns a Job that must be waited for.

Ordinary write failures abort the candidate even when caught. Repair the input and create a new Edit. OutcomeUnknown requires recovery of the original call. See the [recovery guide](../guide.md).
