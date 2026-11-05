#!/usr/bin/env python3
"""Validate a native first-frame trace and join load, preview GPU and presentation.

Metal presentedTime is a compositor timestamp, not a screen/input-to-photon probe.
"""
import argparse
import collections
import json
from pathlib import Path


def analyze(records):
    def require(condition, message):
        if not condition:
            raise ValueError(message)

    metadata = [r for r in records if r.get('type') == 'metadata']
    require(len(metadata) == 1 and metadata[0].get('schema') == 1,
            'expected one schema-1 metadata record')
    metadata = metadata[0]
    require(metadata.get('first_frame') and metadata.get('automatic'),
            'expected automatic first-frame mode')
    ends = [r for r in records if r.get('type') == 'end']
    require(len(ends) == 1 and ends[0].get('lost_events') == 0,
            'trace is truncated or lost events')
    events = [r for r in records if r.get('type') == 'event']
    require(not any(e['stage'] in ('benchmark_failed', 'render_error') for e in events),
            'benchmark or renderer failed')
    groups = collections.defaultdict(list)
    for event in events:
        groups[event['id']].append(event)

    def one(stage, identity=None):
        candidates = events if identity is None else groups[identity]
        selected = [e for e in candidates if e['stage'] == stage]
        require(len(selected) == 1, f'expected one {stage} for {identity}')
        return selected[0]

    load = one('load_begin')
    selected = one('first_frame_presented', load['id'])
    one('benchmark_done', load['id'])
    preview = selected['fields'].get('preview', 0)
    composition = selected['fields'].get('composition', 0)
    require(preview > 0 and composition > 0, 'missing model frame identities')
    accepted_ids = {e['id'] for e in events if e['stage'] == 'accepted'}
    model_compositions = collections.defaultdict(set)
    for event in events:
        if event['stage'] == 'compose_surface':
            identity = event.get('fields', {}).get('preview', 0)
            if identity in accepted_ids:
                model_compositions[event['id']].add(identity)
    presentations = [e for e in events if e['stage'] == 'presented'
                     and e['id'] in model_compositions]
    require(presentations, 'no presented composition containing an accepted model')
    presented = one('presented', composition)
    require(preview in model_compositions[composition],
            'presented composition does not contain the selected model preview')
    require(presented['at_ns'] == selected['at_ns']
            == min(e['at_ns'] for e in presentations),
            'selected composition is not the first actual model presentation')
    redraws = [e for e in groups[load['id']] if e['stage'] == 'first_frame_redraw']
    require(all(e['fields'].get('preview') == preview for e in redraws),
            'retained redraw changed model preview identity')

    names = {
        'load_begin': load, 'presented': presented,
        **{stage: one(stage, load['id']) for stage in (
            'load_admitted', 'load_worker_begin', 'load_worker_end',
            'load_attach_begin', 'load_attached')},
        **{stage: one(stage, preview) for stage in (
            'scene_begin', 'scene_end', 'request', 'enqueued', 'worker_begin',
            'textures_ready', 'model_synced', 'encode_begin', 'preview_commit_begin',
            'preview_submitted', 'published', 'ui_received', 'accepted',
            'preview_gpu_start', 'preview_gpu_end', 'preview_complete')},
        **{stage: one(stage, composition) for stage in (
            'compose_begin', 'drawable_acquired', 'compose_commit_begin', 'compose_submitted',
            'compose_gpu_start', 'compose_gpu_end', 'compose_complete')},
    }
    cpu_frame = names['compose_begin']['fields'].get('cpu_frame', 0)
    require(cpu_frame > 0, 'composition has no correlated UI frame')
    for stage in ('ui_draw_begin', 'ui_draw_end'):
        names[stage] = one(stage, cpu_frame)
    require(names['scene_begin']['at_ns'] >= names['load_attach_begin']['at_ns'],
            'selected preview predates document attachment')
    for stage in ('preview_complete', 'compose_complete'):
        require(names[stage]['fields'].get('status') == 4, f'{stage} did not complete')
    require(not any(e['stage'] in ('rejected', 'coalesced', 'cancelled', 'delivery_coalesced')
                    for e in groups[preview]), 'selected preview was discarded')
    dimensions = names['request']['fields']
    require(dimensions.get('width', 0) > 0 and dimensions.get('height', 0) > 0,
            'invalid viewport extent')
    require(names['published']['fields'].get('draw_calls', 0) > 0,
            'first preview contains no model draw calls')

    pairs = {
        'load_to_viewport_gpu_end_ms': ('load_begin', 'preview_gpu_end'),
        'load_to_present_ms': ('load_begin', 'presented'),
        'load_worker_ms': ('load_worker_begin', 'load_worker_end'),
        'load_to_scene_ready_ms': ('load_begin', 'scene_end'),
        'attach_refresh_ms': ('load_attach_begin', 'load_attached'),
        'scene_ms': ('scene_begin', 'scene_end'),
        'mailbox_ms': ('enqueued', 'worker_begin'),
        'texture_prepare_upload_ms': ('worker_begin', 'textures_ready'),
        'model_sync_ms': ('textures_ready', 'model_synced'),
        'encode_ms': ('encode_begin', 'preview_commit_begin'),
        'preview_submit_ms': ('worker_begin', 'preview_submitted'),
        'preview_queue_ms': ('preview_commit_begin', 'preview_gpu_start'),
        'preview_gpu_ms': ('preview_gpu_start', 'preview_gpu_end'),
        'scene_ready_to_gpu_end_ms': ('scene_end', 'preview_gpu_end'),
        'accepted_to_present_ms': ('accepted', 'presented'),
        'ui_wait_ms': ('accepted', 'ui_draw_begin'),
        'ui_draw_ms': ('ui_draw_begin', 'ui_draw_end'),
        'drawable_wait_ms': ('compose_begin', 'drawable_acquired'),
        'compose_cpu_ms': ('compose_begin', 'compose_submitted'),
        'compose_gpu_ms': ('compose_gpu_start', 'compose_gpu_end'),
        'compose_gpu_end_to_present_ms': ('compose_gpu_end', 'presented'),
        'preview_gpu_end_to_present_ms': ('preview_gpu_end', 'presented'),
    }
    metrics = {}
    for metric, (start, end) in pairs.items():
        delta = names[end]['at_ns'] - names[start]['at_ns']
        require(delta >= 0, f'negative interval: {metric}')
        metrics[metric] = delta / 1e6
    return {
        'metadata': metadata, 'load': load['id'], 'preview': preview,
        'composition': composition, 'cpu_frame': cpu_frame, 'presented_ns': presented['at_ns'],
        'retained_redraw_requests': len(redraws),
        'dropped_model_compositions': sum(e['stage'] == 'present_dropped'
                                          and e['id'] in model_compositions for e in events),
        'viewport': {key: dimensions[key] for key in ('width', 'height')},
        'draw_calls': names['published']['fields']['draw_calls'], 'metrics': metrics,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = analyze([json.loads(line) for line in args.trace.read_text().splitlines()])
    text = json.dumps(result, ensure_ascii=False, indent=2) + '\n'
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end='')


if __name__ == '__main__':
    main()
