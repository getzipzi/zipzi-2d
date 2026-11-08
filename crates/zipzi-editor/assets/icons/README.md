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

Cursor artwork and application branding have their own visual requirements and
live in separate directories.
