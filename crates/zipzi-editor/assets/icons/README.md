# Editor UI icons

Functional icons share the Kit/Lucide 24 × 24 grid, `currentColor`, a 2-unit
stroke, and round caps/joins. Keep the main stroke consistent when drawing new
icons; simplify crowded geometry instead of making an individual tool thinner.

The compact toolbar renders these at 12 logical pixels with the default scale.
Review at 12, 16 and 24 pixels on light and dark backgrounds. Enlarged previews
alone can hide crowded intersections or disappearing details.

Mesh vertex markers use filled circles with radius 1.25. Their 2.5-unit diameter
provides a modest emphasis over the 2-unit edges. The source-image sun is a
separate detail with radius 1. Bezier endpoints are filled 3 × 3 squares and
control handles are filled circles with radius 1.5. These small markers have no
stroke: adding a 2-unit outline would grow them to 5 units and close the holes
at compact sizes. Bezier handle guides retain reduced opacity.

Glue tools share the square junction used on the canvas. Add point combines
that junction with edges and a plus; Bind converges two vertices into one
junction; Unbind releases a junction into two outward branches; Edit Glue
weights pairs the junction with a brush. Keep their square markers hollow
and their ordinary vertex markers filled so the two remain distinct at 12px.

Weight brushes use mirrored arrows toward A/B and one-sided release branches.
Inactive A/B icons use their red/green mesh hues; selected icons inherit the
toolbar's contrasting foreground. Reattach and Release both reuse the junction
merge/split symbols. Mode names and brush directions belong in tooltips.

Cursor artwork and application branding have their own visual requirements and
live in separate directories.
