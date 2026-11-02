"""Structural diagnostics and explicit limits of automated semantic coverage."""
from collections import defaultdict
import re
from .reference_routing import page_group

PLACEHOLDERS = ('using the arguments shown in its signature',
                'Immutable typed value. See field semantics', 'Typed public API entry.',
                'Construction owns declared collections;', 'reading fields, repr and summary performs no IO.')


def quality_report(entries, domains, modeling_domains):
    grouped = defaultdict(list)
    for entry in entries:
        grouped[entry.identity or entry.path].append(entry)
    diagnostics=[]
    review=[]
    classifications=defaultdict(int)
    for identity, definitions in sorted(grouped.items()):
        paths=sorted(entry.path for entry in definitions)
        base={'identity':identity,'paths':paths,
              'pages':sorted({page_group(path,domains,modeling_domains) for path in paths}),
              'source':next((entry.documentation_source for entry in definitions if entry.documentation_source),identity)}
        checks=set(check for entry in definitions for check in entry.diagnostics)
        for category,subject,status in sorted(checks):
            if category=='type-reference' and status!='unresolved-sdk':
                classifications[status]+=1
                continue
            row=dict(base,category=category,subject=subject,status=status)
            (diagnostics if status in {'missing','unresolved-sdk'} else review).append(row)
        for entry in definitions:
            if any(pattern in entry.summary or any(pattern in text for _,text in entry.fields or ()) for pattern in PLACEHOLDERS):
                diagnostics.append(dict(base,category='placeholder',subject=entry.path,status='missing'))
        if any(entry.kind in {'type','enum','alias','callable'} for entry in definitions) and not any(entry.summary.strip() for entry in definitions):
            diagnostics.append(dict(base,category='empty-summary',subject='',status='missing'))
    return {'_generated_by':'zipzi-python2/tools/generate.py',
            'scope_note':'Type summaries, enum/union branches and callable parameters/returns are separate checks. Nonempty text requires human semantic review; it is not proof of correctness.',
            'pages':sorted({page_group(entry.path,domains,modeling_domains) for entry in entries}),
            'reference_classifications':dict(sorted(classifications.items())),
            'diagnostics':diagnostics,'semantic_review':review}


def debt_keys(coverage, quality):
    """Only stable identities/categories, never prose, counts or AST addresses."""
    keys={('type-summary',item['identity'],'') for item in coverage['missing']}
    keys.update((item['category'],item['identity'],item['subject']) for item in quality['diagnostics'])
    return keys


def validate_debt(current, baseline):
    added=current-baseline
    if added:raise ValueError('New documentation debt: '+repr(sorted(added)))
    resolved=baseline-current
    if resolved:raise ValueError('Remove resolved documentation debt from the baseline: '+repr(sorted(resolved)))


def validate_reference_declarations(documents):
    for path,text in documents.items():
        if not path.endswith('.md'):continue
        labels={}
        for label,target in re.findall(r'^\[([^\]]+)\]: (\S+)$',text,re.MULTILINE):
            key=' '.join(label.casefold().split())
            if key in labels:raise ValueError('Duplicate/ambiguous Markdown reference: '+path+' ['+label+']')
            labels[key]=target


def scoped_missing(coverage, quality, pages=(), paths=()):
    """Exact routed pages, plus explicit cross-page pilot paths."""
    wanted=set(paths)
    missing=[('type-summary',item) for item in coverage['missing']
             if wanted.intersection(item['paths']) or set(pages)&set(item['pages'])]
    missing.extend((item['category'],item) for item in quality['diagnostics']
                   if wanted.intersection(item['paths']) or set(pages)&set(item['pages']))
    return missing
