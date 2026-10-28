#!/usr/bin/env python3
"""Render an offline side-by-side gallery from reference and Rust test reports."""
import argparse
import html
import json
from pathlib import Path


def svg(points, triangles):
    edges = sorted({tuple(sorted((t[i], t[(i + 1) % 3]))) for t in triangles for i in range(3)})
    path = " ".join(
        f"M{points[a][0]:.3f},{points[a][1]:.3f}L{points[b][0]:.3f},{points[b][1]:.3f}"
        for a, b in edges
    )
    return f'<svg viewBox="-8 -8 208 176"><path d="{path}" fill="none" stroke="#267788" stroke-width=".65"/></svg>'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--reference", type=Path, default=Path("fixtures/mesh-generation/psd2live.json"))
    parser.add_argument("--output", type=Path, default=Path("target/mesh-port/comparison.html"))
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    reference = json.loads(args.reference.read_text())
    key = lambda c: (c["name"], c["edge"], c["fill"])
    cases = {key(c): c for c in reference["cases"]}
    cards = []
    for c in report:
        ref = cases[key(c)]
        points = list(zip(ref["positions"][::2], ref["positions"][1::2]))
        triangles = [ref["indices"][i:i + 3] for i in range(0, len(ref["indices"]), 3)]
        title = html.escape(" · ".join(key(c)))
        cards.append(f'''<article data-shape="{html.escape(c['name'])}">
<h2>{title}</h2><div class="pair"><figure>{svg(points, triangles)}
<figcaption>PSD2Live · {len(points)} vertices</figcaption></figure>
<figure>{svg(c['points'], c['triangles'])}<figcaption>Rust · {c['vertices']} vertices</figcaption></figure></div>
<p>Foreground coverage: {c['covered']} / {c['painted']} · recovered components: {c['recovered_components']}</p></article>''')
    options = ''.join(f'<option>{html.escape(s)}</option>' for s in sorted({c['name'] for c in report}))
    document = f'''<!doctype html><html lang="en"><meta charset="utf-8">
<title>PSD2Live / Zipzi mesh comparison</title><style>
body{{font:14px system-ui;background:#edf1f2;color:#17343a;margin:24px}}
h1{{font-size:24px}}h2{{font-size:13px}}main{{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:16px}}
article{{background:white;border-radius:8px;padding:16px}}article[hidden]{{display:none}}
.pair{{display:flex}}figure{{width:50%;margin:0}}svg{{width:100%;background:#fafcfc}}figcaption{{text-align:center}}
article p{{font-size:12px;color:#52666b}}select{{margin:0 0 20px;padding:6px}}
</style><h1>PSD2Live / Zipzi mesh comparison</h1>
<p>Four masks × three edge modes × five fill strategies. Equal pixel coverage and topology do not imply identical triangles.</p>
<label>Shape <select id="shape"><option value="">All</option>{options}</select></label>
<main>{''.join(cards)}</main><script>
document.querySelector('#shape').onchange = e => document.querySelectorAll('article').forEach(a => a.hidden = !!e.target.value && a.dataset.shape !== e.target.value);
</script></html>'''
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(document)
    print(args.output.resolve())


if __name__ == "__main__":
    main()
