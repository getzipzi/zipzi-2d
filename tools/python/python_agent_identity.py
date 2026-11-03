"""Optional model/service metadata for reports; never an acceptance gate."""
from __future__ import annotations

LIFECYCLE_EVENTS=frozenset({'session.started','thread.started','turn.started','turn.completed','service.identity'})
IDENTITY_POLICY={
    'version':'p3a-service-metadata-2','mode':'report_only',
    'required':False,'affects_acceptance':False,
    'missing_evidence':'unavailable; does not affect acceptance',
    'configured_model_is_service_metadata':False,
}


def evaluate(events:list[dict],returncode:int)->dict:
    """Record observed lifecycle fields without treating model text as metadata."""
    values={'resolved_model':set(),'service_revision':set()}
    for event in events:
        if event.get('type') not in LIFECYCLE_EVENTS:continue
        for key,observed in values.items():
            value=event.get(key)
            if isinstance(value,str) and value.strip():observed.add(value)
    return {'mode':'report_only',
        'service_completed':returncode==0 and any(e.get('type')=='turn.completed' for e in events),
        'resolved_models':sorted(values['resolved_model']),
        'service_revisions':sorted(values['service_revision'])}
