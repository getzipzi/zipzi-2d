"""Route generated value families to readable domain modules.

Only source organization belongs here. Fields, defaults, variants and codecs
continue to come from the compiled Rust contract.
"""

# Exceptions precede family prefixes: a MeshBinding belongs with bindings,
# while a BlendMode is shared by scene and animation values.
SPECIAL = {
    "Appearance": "common",
    "BlendMode": "common",
    "QuerySource": "common",
    "ParameterActivation": "sampling",
    "ProjectFingerprint": "operations",
    "ImportFormat": "operations",
    "ExportFormat": "operations",
    "CollectionTarget": "operations",
    "SceneBinding": "bindings",
    "SceneKeyform": "bindings",
    "SceneTrack": "bindings",
    "SceneReference": "scene",
    "ShapeTarget": "bindings",
    "ConstraintSample": "blend_shapes",
    "ConversionDomain": "bindings",
    "AxisReduction": "bindings",
    "PartKeyform": "bindings",
    "PartKeyformSlot": "bindings",
    "RotationKeyform": "bindings",
    "WarpKeyform": "bindings",
    "OffscreenKeyform": "bindings",
    "OffscreenDeletePolicy": "deletion",
    "OrganizationDeletePolicy": "deletion",
    "VertexDeletePolicy": "deletion",
    "VertexContentPatch": "scene_content",
    "PropertyPatch": "scene_edit",
    "ModelSetup": "scene",
    "DocumentAuthoringEdit": "scene_edit",
    "DocumentValue": "scene_edit",
    "PackageAttachment": "resources",
    "SceneContentEdit": "scene_content",
    "SceneContentTarget": "scene_content",
    "SceneContentValue": "scene_content",
    "SceneCopyScope": "scene_content",
    "OffscreenMappingPatch": "scene_content",
}

FAMILIES = (
    ("Command", "commands"),
    ("Operation", "operations"),
    ("Query", "operations"),
    ("Parameter", "parameters"),
    ("MeshBinding", "bindings"),
    ("MeshKeyform", "bindings"),
    ("MeshConstraint", "bindings"),
    ("Binding", "bindings"),
    ("Blend", "blend_shapes"),
    ("Delta", "blend_shapes"),
    ("Grid", "bindings"),
    ("Mesh", "meshes"),
    ("Manual", "meshes"),
    ("NewMeshVertex", "meshes"),
    ("Vertex", "meshes"),
    ("GlueVertexPair", "meshes"),
    ("Motion", "motion"),
    ("Expression", "expressions"),
    ("Physics", "physics"),
    ("Pose", "pose"),
    ("Deletion", "deletion"),
    ("Organization", "organization"),
    ("Reparent", "organization"),
    ("Asset", "resources"),
    ("Attachment", "resources"),
    ("ImageInput", "resources"),
    ("Reference", "references"),
    ("Cdi", "references"),
    ("DisplayInfo", "references"),
    ("Model", "references"),
    ("Sample", "sampling"),
    ("SpatialQuery", "sampling"),
    ("Observation", "sampling"),
    ("Topology", "sampling"),
    ("Scene", "scene_edit"),
    ("Warp", "scene"),
    ("Rotation", "scene"),
    ("Part", "scene"),
    ("Transform", "scene"),
    ("Canvas", "scene"),
    ("ExportSettings", "scene"),
    ("EditorState", "scene"),
    ("ObjectEditorState", "scene"),
    ("DeformerOriginalAction", "scene"),
)


def module_for(name):
    if name in SPECIAL:
        return SPECIAL[name]
    for prefix, module in FAMILIES:
        if name.startswith(prefix):
            return module
    return "common"
