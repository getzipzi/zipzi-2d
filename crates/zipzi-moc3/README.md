# zipzi-moc3

MOC3 binary format support for encoding, decoding, inspection, and model import.

Raw decoders/importers preserve runtime texture slots and UVs. Project/SDK session
imports additionally unpack those pages into independent Mesh images; see
[project import artwork](../zipzi-project/README.md#import-artwork).
