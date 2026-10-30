"""Organize completed GPU observations as local PNG files and a provenance manifest.

No device or Runtime is consulted; this module does not compute image or geometry algorithms.
"""
from dataclasses import dataclass, asdict
import hashlib
import json
from pathlib import Path
from ._presentation import CompactValue
from ._results import Snapshot
from ._generated import values as m
from ._generated.codecs import observation as _modeling_codec

@dataclass(frozen=True,repr=False)
class Report(CompactValue):
    """Paths to an exported local inspection, readable without a live SDK host.

    Fields:
    directory: Client-side output directory created by save; not a remote host path.
    manifest: JSON provenance document containing Source, view, texture hashes and frame identities.
    images: PNG paths in input frame order; the original Rust PNG bytes are preserved.
    """
    directory:Path
    manifest:Path
    images:tuple[Path,...]

def save(result:Snapshot[m.ObservationBatch]|Snapshot[m.PreviewBatch],directory:Path)->Report:
    """Write completed observation/Preview images and JSON metadata into a fresh client-side directory.

    This performs local file IO only and preserves Rust PNG bytes. Preview batches
    require an image on every frame. Existing directories raise FileExistsError;
    IO errors can leave partial output. No host write, device probing, image
    recomputation or implicit request recovery occurs. Retain the original result
    if file IO needs to be retried into a new directory.
    """
    batch=result.value
    if isinstance(batch,m.ObservationBatch):
        view=batch.view;images=batch.frames
        playback:tuple[dict[str,object]|None,...]=tuple(None for _ in images)
    else:
        if batch.view is None or any(frame.observation is None for frame in batch.frames):
            raise ValueError("Preview batch must contain GPU images")
        view=batch.view;images=tuple(frame.observation for frame in batch.frames if frame.observation is not None)
        playback=tuple(dict(preview_id=frame.preview_id,sequence=frame.sequence,index=frame.index,time=frame.motion.time) for frame in batch.frames)
    manifest_frames:list[dict[str,object]]=[]
    for index,(image,motion) in enumerate(zip(images,playback)):
        manifest_frames.append(dict(file=f"frame-{index:03}.png",png_sha256=hashlib.sha256(image.png).hexdigest(),
            width=image.width,height=image.height,capture_id=image.capture_id,scene_digest=image.scene_digest,render_digest=image.render_digest,
            input_sha256=image.input_sha256,document_id=image.document_id,evaluation_revision=image.evaluation_revision,
            adapter_name=image.adapter_name,adapter_backend=image.adapter_backend,parameters=dict(image.parameters),
            visible_roi=image.visible_roi,scale=image.scale,offset=image.offset,
            textures=[dict(asset_id=t.asset_id,sha256=t.sha256) for t in image.textures],
            quality=[asdict(q) for q in image.quality],playback=motion))
    metadata=json.dumps(dict(schema=1,source=result.source._wire(),view=_modeling_codec.encode_ObservationView(view),frames=manifest_frames),
        ensure_ascii=False,allow_nan=False,sort_keys=True,indent=2)+"\n"
    directory=directory.absolute();directory.mkdir(parents=True,exist_ok=False)
    paths=tuple(directory/str(frame['file']) for frame in manifest_frames)
    for path,image in zip(paths,images):path.write_bytes(image.png)
    manifest=directory/'inspection.json';manifest.write_text(metadata,encoding='utf-8')
    return Report(directory,manifest,paths)

__all__=['Report','save']
