"""Render the already validated offline API documentation model."""
import importlib.util
from pathlib import Path

_spec=importlib.util.spec_from_file_location('zipzi_catalog_output', Path(__file__).parent/'codegen/catalog_output.py')
_catalog_output=importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_catalog_output)

TARGETS=frozenset({'client/python/zipzi/_generated/metadata.py','client/python/zipzi/docs/reference.md'})


def render_catalog(contract,model,header,contract_hash):
    entries,domains,common,package_version,source_sha,prefix=model
    return _catalog_output.render(entries,contract,contract_hash,domains,common,package_version,source_sha,prefix,header)
