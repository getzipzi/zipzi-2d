# SDK acceptance recipes

These scripts are invoked by `tools/validation/validate_sdk.py` and Python typing CI.
They were restored from the parent of `9970d891` after P5 detected that the
validator still referenced scripts deleted by that commit. The source PNG is
synthetic. Current API adaptations remove the old pixels-per-unit constructor
argument and use pixel-sized model coordinates.

`python_cpu_recipe.py` creates and reopens a bound model. The two-asset recipe
checks interpolation, persistence and export. The import/edit recipe explicitly
recognizes the fixture's two missing attachment references and discards only
those references through the public API before exporting; it records this action
in its report. The original fixture remains unchanged, and export with unresolved
attachments is still rejected by the SDK CPU tests.

Run recipes with an installed wheel from outside the checkout. CPU recipes and
typing were verified in P5; observation, agent-repair and reference capture require
an observe-enabled wheel and remain part of GPU acceptance.
