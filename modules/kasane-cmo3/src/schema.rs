//! Cubism XML class identifiers and schema versions (wire-format data).
pub(crate) fn class_name(tag: &str) -> Option<&'static str> {
    Some(match tag {
    "ACAffecterForm" => "com.live2d.cubism.doc.model.affecter.ACAffecterForm",
    "ACAffecterSource" => "com.live2d.cubism.doc.model.affecter.ACAffecterSource",
    "ACDeformerForm" => "com.live2d.cubism.doc.model.deformer.ACDeformerForm",
    "ACDeformerSource" => "com.live2d.cubism.doc.model.deformer.ACDeformerSource",
    "ACDrawableForm" => "com.live2d.cubism.doc.model.drawable.ACDrawableForm",
    "ACDrawableSource" => "com.live2d.cubism.doc.model.drawable.ACDrawableSource",
    "ACExtension" => "com.live2d.cubism.doc.model.extension.ACExtension",
    "ACForm" => "com.live2d.cubism.doc.model.ACForm",
    "ACParameterControllableSource" => "com.live2d.cubism.doc.model.ACParameterControllableSource",
    "ACTextureInput" => "com.live2d.cubism.doc.model.extension.textureInput.ACTextureInput",
    "AlphaComposition" => "com.live2d.cubism.doc.model.drawable.AlphaComposition",
    "Anisotropy" => "com.live2d.graphics3d.texture.Anisotropy",
    "CAffecterGuid" => "com.live2d.type.CAffecterGuid",
    "CAffecterId" => "com.live2d.cubism.doc.model.id.CAffecterId",
    "CAffecterSourceSet" => "com.live2d.cubism.doc.model.affecter.CAffecterSourceSet",
    "CAffine" => "com.live2d.type.CAffine",
    "CArtMeshForm" => "com.live2d.cubism.doc.model.drawable.artMesh.CArtMeshForm",
    "CArtMeshSource" => "com.live2d.cubism.doc.model.drawable.artMesh.CArtMeshSource",
    "CArtPathBrushSetting" => "com.live2d.cubism.doc.model.drawable.artPath.Line.CArtPathBrushSetting",
    "CCachedImage" => "com.live2d.graphics.cachedImage.CCachedImage",
    "CCachedImageManager" => "com.live2d.graphics.cachedImage.CCachedImageManager",
    "CColor" => "com.live2d.type.CColor",
    "CDeformerGuid" => "com.live2d.type.CDeformerGuid",
    "CDeformerId" => "com.live2d.cubism.doc.model.id.CDeformerId",
    "CDeformerSourceSet" => "com.live2d.cubism.doc.model.deformer.CDeformerSourceSet",
    "CDrawableGuid" => "com.live2d.type.CDrawableGuid",
    "CDrawableId" => "com.live2d.cubism.doc.model.id.CDrawableId",
    "CDrawableSourceSet" => "com.live2d.cubism.doc.model.drawable.CDrawableSourceSet",
    "CEditableMeshExtension" => "com.live2d.cubism.doc.model.extension.editableMesh.CEditableMeshExtension",
    "EffectParameterType" => "com.live2d.cubism.doc.model.EffectParameterType",
    "CEffectParameterGroups" => "com.live2d.cubism.doc.model.CEffectParameterGroups",
    "CExtensionGuid" => "com.live2d.type.CExtensionGuid",
    "CFloatColor" => "com.live2d.type.CFloatColor",
    "CFormGuid" => "com.live2d.type.CFormGuid",
    "CGameMotionSet" => "com.live2d.cubism.doc.gameData.motions.CGameMotionSet",
    "CGlueForm" => "com.live2d.cubism.doc.model.affecter.glue.CGlueForm",
    "CGlueSource" => "com.live2d.cubism.doc.model.affecter.glue.CGlueSource",
    "CGuidesSetting" => "com.live2d.cubism.doc.modeling.ui.guide.CGuidesSetting",
    "CImageCanvas" => "com.live2d.graphics.CImageCanvas",
    "CImageIcon" => "com.live2d.type.CImageIcon",
    "CImageResource" => "com.live2d.graphics.CImageResource",
    "CLabelColor" => "com.live2d.cubism.doc.model.CLabelColor",
    "CLabelColorType" => "com.live2d.cubism.doc.model.CLabelColorType",
    "CMeshGeneratorExtension" => "com.live2d.cubism.doc.model.extension.meshGenerator.CMeshGeneratorExtension",
    "CModelGuid" => "com.live2d.type.CModelGuid",
    "CModelInfo" => "com.live2d.cubism.doc.model.CModelInfo",
    "CModelSource" => "com.live2d.cubism.doc.model.CModelSource",
    "CMotionSyncSettingSourceSet" => "com.live2d.cubism.doc.model.motionSync.CMotionSyncSettingSourceSet",
    "CParameterGroup" => "com.live2d.cubism.doc.model.param.group.CParameterGroup",
    "CParameterGroupGuid" => "com.live2d.type.CParameterGroupGuid",
    "CParameterGroupId" => "com.live2d.cubism.doc.model.id.CParameterGroupId",
    "CParameterGroupSet" => "com.live2d.cubism.doc.model.param.group.CParameterGroupSet",
    "CParameterGuid" => "com.live2d.type.CParameterGuid",
    "CParameterId" => "com.live2d.cubism.doc.model.id.CParameterId",
    "CParameterSource" => "com.live2d.cubism.doc.model.param.CParameterSource",
    "CParameterSourceSet" => "com.live2d.cubism.doc.model.param.CParameterSourceSet",
    "CPartForm" => "com.live2d.cubism.doc.model.parts.CPartForm",
    "CPartGuid" => "com.live2d.type.CPartGuid",
    "CPartId" => "com.live2d.cubism.doc.model.id.CPartId",
    "CPartSource" => "com.live2d.cubism.doc.model.parts.CPartSource",
    "CPartSourceSet" => "com.live2d.cubism.doc.model.parts.CPartSourceSet",
    "CPhysicsSettingsSourceSet" => "com.live2d.cubism.doc.gameData.physics.CPhysicsSettingsSourceSet",
    "CPoint" => "com.live2d.type.CPoint",
    "CRandomPoseSettingManager" => "com.live2d.cubism.doc.model.randomPose.CRandomPoseSettingManager",
    "CRotationDeformerForm" => "com.live2d.cubism.doc.model.deformer.rotation.CRotationDeformerForm",
    "CRotationDeformerSource" => "com.live2d.cubism.doc.model.deformer.rotation.CRotationDeformerSource",
    "CSize" => "com.live2d.type.CSize",
    "CTextureAtlas" => "com.live2d.cubism.doc.model.texture.textureAtlas.CTextureAtlas",
    "CTextureAtlasGuid" => "com.live2d.type.CTextureAtlasGuid",
    "CTextureInputExtension" => "com.live2d.cubism.doc.model.extension.textureInput.CTextureInputExtension",
    "CTextureInput_TextureAtlasRegion" => "com.live2d.cubism.doc.model.extension.textureInput.CTextureInput_TextureAtlasRegion",
    "CTextureManager" => "com.live2d.cubism.doc.model.texture.CTextureManager",
    "CWarpDeformerForm" => "com.live2d.cubism.doc.model.deformer.warp.CWarpDeformerForm",
    "CWarpDeformerSource" => "com.live2d.cubism.doc.model.deformer.warp.CWarpDeformerSource",
    "CWritableImage" => "com.live2d.graphics.CWritableImage",
    "CachedImageType" => "com.live2d.graphics.cachedImage.CachedImageType",
    "ColorComposition" => "com.live2d.cubism.doc.model.drawable.ColorComposition",
    "CoordType" => "com.live2d.doc.CoordType",
    "EditorEdition" => "com.live2d.cubism.doc.model.options.edition.EditorEdition",
    "ExtendedInterpolationType" => "com.live2d.cubism.doc.model.interpolator.extendedInterpolation.ExtendedInterpolationType",
    "FilterMode" => "com.live2d.graphics3d.texture.GTexture$FilterMode",
    "GEditableMesh2" => "com.live2d.graphics3d.editableMesh.GEditableMesh2",
    "GEditableMeshGuid" => "com.live2d.type.GEditableMeshGuid",
    "GTexture" => "com.live2d.graphics3d.texture.GTexture",
    "GTexture2D" => "com.live2d.graphics3d.texture.GTexture2D",
    "GTextureGuid" => "com.live2d.type.GTextureGuid",
    "GVector2" => "com.live2d.graphics3d.type.GVector2",
    "InterpolationType" => "com.live2d.cubism.doc.model.interpolator.InterpolationType",
    "KeyFormMorphTargetSet" => "com.live2d.cubism.doc.model.morphTarget.KeyFormMorphTargetSet",
    "KeyOnParameter" => "com.live2d.cubism.doc.model.interpolator.KeyOnParameter",
    "KeyformBindingSource" => "com.live2d.cubism.doc.model.interpolator.KeyformBindingSource",
    "KeyformGridAccessKey" => "com.live2d.cubism.doc.model.interpolator.KeyformGridAccessKey",
    "KeyformGridSource" => "com.live2d.cubism.doc.model.interpolator.KeyformGridSource",
    "KeyformOnGrid" => "com.live2d.cubism.doc.model.interpolator.KeyformOnGrid",
    "MagFilter" => "com.live2d.graphics3d.texture.MagFilter",
    "MeshGenerateSetting" => "com.live2d.cubism.doc.model.extension.meshGenerator.MeshGenerateSetting",
    "MinFilter" => "com.live2d.graphics3d.texture.MinFilter",
    "ModelViewerSetting" => "com.live2d.cubism.doc.modeling.ui.viewer.ModelViewerSetting",
    "MorphTargetBlendWeightConstraintSet" => "com.live2d.cubism.doc.model.morphTarget.MorphTargetBlendWeightConstraintSet",
    "TextureImageGroup" => "com.live2d.cubism.doc.model.texture.TextureImageGroup",
    "TextureState" => "com.live2d.cubism.doc.model.drawable.TextureState",
    "Type" => "com.live2d.cubism.doc.model.param.CParameterSource$Type",
    "WrapMode" => "com.live2d.graphics3d.texture.WrapMode",
    "CPhysicsDataGuid" => "com.live2d.type.CPhysicsDataGuid",
    "CPhysicsInput" => "com.live2d.cubism.doc.gameData.physics.CPhysicsInput",
    "CPhysicsOutput" => "com.live2d.cubism.doc.gameData.physics.CPhysicsOutput",
    "CPhysicsSettingId" => "com.live2d.cubism.doc.model.id.CPhysicsSettingId",
    "CPhysicsSettingsGuid" => "com.live2d.type.CPhysicsSettingsGuid",
    "CPhysicsSettingsSource" => "com.live2d.cubism.doc.gameData.physics.CPhysicsSettingsSource",
    "CPhysicsSourceType" => "com.live2d.cubism.doc.gameData.physics.CPhysicsController$CPhysicsSourceType",
    "CPhysicsVertex" => "com.live2d.cubism.doc.gameData.physics.CPhysicsVertex",
    "KeyFormMorphTarget" => "com.live2d.cubism.doc.model.morphTarget.KeyFormMorphTarget",
    "MorphTargetBlendWeightConstraint" => "com.live2d.cubism.doc.model.morphTarget.MorphTargetBlendWeightConstraint",
    "MorphTargetEditBaseParameterMap" => "com.live2d.cubism.doc.model.morphTarget.MorphTargetEditBaseParameterMap",
_=>return None,
})
}
pub(crate) fn version(tag: &str) -> Option<(&'static str, u32)> {
    Some(match tag {
        "CModelSource" => ("CModelSource", 15),
        "CArtMeshSource" => ("CArtMeshSource", 5),
        "CFloatColor" => ("CFloatColor", 1),
        "CLabelColor" => ("CLabelColor", 0),
        "CParameterGroup" => ("CParameterGroup", 4),
        "CPartForm" => ("CPartForm", 2),
        "CPartSource" => ("CPartSource", 2),
        "CRotationDeformerForm" => ("CRotationDeformerForm", 1),
        "KeyFormMorphTarget" => ("KeyFormMorphTarget", 2),
        "KeyformGridSource" => ("KeyformGridSource", 1),
        _ => return None,
    })
}
pub(crate) fn is_builtin(tag: &str) -> bool {
    matches!(
        tag,
        "root"
            | "shared"
            | "main"
            | "entry"
            | "null"
            | "file"
            | "i"
            | "f"
            | "d"
            | "l"
            | "b"
            | "s"
            | "array"
            | "array_list"
            | "carray_list"
            | "hash_map"
            | "linked_set"
            | "long-array"
            | "int-array"
            | "float-array"
            | "short-array"
            | "byte-array"
    )
}
