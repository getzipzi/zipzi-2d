"""Create tiny Shell-only v1 fixtures; authoring JSON is intentionally not decoded."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile
import zlib


def fixtures(directory):
    directory.mkdir(parents=True, exist_ok=True)
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    pixels = b'\xff\x00\x00\xff\x00\xff\x00\xff'
    png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 1, 8, 6, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(b'\x00' + pixels)) + chunk(b'IEND', b''))
    for variant in ('image', 'absent', 'corrupt'):
        info = dict(format='zipzi-project-package', container_version=1, document='project.json',
                    document_sha256='intentionally not verified by Shell', required_features=[])
        if variant != 'absent':
            info['preview'] = dict(sha256=hashlib.sha256(png).hexdigest(), width=2, height=1)
        with zipfile.ZipFile(directory / (variant + '.zipzi'), 'w') as archive:
            archive.writestr('format.json', json.dumps(info), compress_type=zipfile.ZIP_DEFLATED)
            archive.writestr('project.json', 'Shell must not decode this authoring JSON')
            if variant != 'absent':
                archive.writestr('preview.png', png if variant == 'image' else b'corrupt')
    (directory / 'expected.png').write_bytes(png)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    fixtures(parser.parse_args().output)
