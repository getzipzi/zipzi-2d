"""Reviewed external Rust leaves and Python projection policy."""

# Opaque values with no complete exported authoring shape: Runtime identities,
# borrowed/counted host values or handwritten client records. These are not
# silently treated as named public records by the type graph.
EXTERNAL_TYPES = frozenset({
    '&str',
    'u64',
    'alloc::sync::Arc<zipzi_contract::authoring::EditReceipt>',
    'core::option::Option<zipzi_contract::runtime::ClientId>',
    'core::option::Option<zipzi_contract::runtime::JobId>',
    'core::option::Option<zipzi_contract::runtime::SessionId>',
    'zipzi_contract::authoring::SaveReceipt',
    'zipzi_contract::authoring::Version',
    'zipzi_contract::runtime::ClientId',
    'zipzi_contract::runtime::EditId',
    'zipzi_contract::runtime::ImageId',
    'zipzi_contract::runtime::JobId',
    'zipzi_contract::runtime::OperationId',
    'zipzi_contract::runtime::PlanId',
    'zipzi_contract::runtime::RuntimeInstanceId',
    'zipzi_contract::runtime::SessionId',
    'zipzi_contract::runtime::SessionInfo',
    'zipzi_contract::runtime::SourceRef',
})

# Public value readers are private implementation details. New projected
# readers require an explicit entry so source generation never scans old output.
READ_CLASSES = frozenset('''
Axis Keyform MeshBindingSpec Create SetKeyform Sample AxisSnapshot Appearance
KeyformSnapshot MeshBinding ObjectSummary RuntimeCapabilities RuntimeLimits
Cursor Origin Published Saved StateChanged Created Closed Constraint Meshing
Geometry MeshSpec Mesh ParameterSpec ParameterPatch Patch Parameter PlanCost
PlanReport Capability SceneCapabilities Usage ImagePixels Image Pose Checks
Drawable Offscreen Deformer Frame Diagnostic Result Summary SessionId
ConnectionLimits ConnectionInfo Version Source Receipt SaveReceipt SessionInfo
WriteResult
'''.split())

# Unit enums with the same wire values and construction semantics share one
# class even when a convenience API exports a shorter name.
CANONICAL_ENUM_ALIASES = frozenset({
    'zipzi_contract::model::MeshConstraintKind',
    'zipzi_contract::model::ParameterKind',
    'zipzi_contract::model::BlendMode',
    'zipzi_contract::authoring::GeometryDiagnosticKind',
})

# Native bridge entrypoints intentionally visible outside the generated module.
NATIVE_ENTRYPOINTS = frozenset('''
measure_addresspatch measure_addressselector measure_capabilitydomain
measure_collectionquery measure_deletionsection measure_meshbindingoperation
measure_modelingcommand measure_modelingintent measure_modelingquery
measure_parameteroperation measure_previewcommand measure_projectrequest measure_spatialquery
measure_observationview
read_addresspatch read_addressselector read_capabilitydomain read_collectionquery
read_deletionsection read_geometrychecks read_meshbindingoperation read_modelingcommand
read_modelingintent read_modelingquery read_objectkind_name read_observationview
read_parameteroperation read_previewcommand read_projectrequest read_spatialquery
write_capabilities write_deletionpage write_deletionsummary write_editorcontext
write_manualmeshtransferreport write_meshbinding write_modelingsamples write_observationbatch
write_observationestimate
write_ownedmodelingrecords write_parameter write_previewbatch write_previewstate
write_projectreport write_spatialsamples
'''.split())

# The public convenience projections retain positional constructors, explicit
# ownership conversion, or source-bearing result wrappers. They are distinct
# from the keyword-only canonical values produced from the same Rust shape.
SPECIAL_PROJECTIONS = {
    'meshes': 'Mesh convenience values use positional construction and own sequence inputs.',
    'mesh_ops': 'Mesh operations expose positional convenience constructors and a public operation discriminator.',
    'resource_values': 'Resource values adapt native handles and source-bearing snapshots to client ownership.',
    'resource_ops': 'Resource operations flatten creation specs and accept owned resource handles.',
    'plan_values': 'Plan reports add the captured source and use positional client result construction.',
    'scene': 'Scene operations use positional convenience constructors and explicit domain discriminators.',
    'parameter_values': 'Parameter convenience values own sequence inputs and expose positional constructors.',
    'binding_values': 'Binding convenience values use numeric coordinates and own sequence inputs.',
    'sampling_values': 'Sampling results adapt source-bearing frames and parameter poses for client use.',
    'reference_values': 'Reference capabilities project native state and use client-facing positional constructors.',
    'event_values': 'Events project native tagged payloads into source-bearing client event values.',
    'discovery': 'Runtime discovery metadata uses positional client result construction.',
}

# Exact names keep domain decisions reviewable; fields and wire layouts always
# come from the Rust type graph. Generic identities are listed without erasure.
DOMAIN_MEMBERS = {
    'zipzi_contract::animation': {
        'motion': 'MotionEventFired MotionPlaybackPosition MotionSnapshot',
        'physics': 'PhysicsOutputSnapshot PhysicsRigSnapshot',
    },
    'zipzi_contract::authoring': {
        'addressing': 'AttachmentCapabilities BindingCapabilities CapabilityState CollectionCapabilities ExpressionCapabilities MotionCapabilities ObjectKind OrganizationCapabilities PhysicsCapabilities PoseCapabilities ReferenceCapabilities ResourceCapabilities SceneCapabilities SceneContentCapabilities',
        'bindings': 'BindingAuthoringEdit BindingFieldPatch MeshBindingAxisSpec MeshBindingCreateSpec MeshBindingOperation MeshKeyformSpec ScalarSample',
        'deletion': 'DeletionIntent DeletionPage DeletionPositionGuarantee DeletionReviewState DeletionRow DeletionSection DeletionSectionCount DeletionSummary',
        'mesh': 'ManualMeshOperation ManualMeshTransferReport ManualVertexTarget MeshCreateSpec MeshGeometry MeshOperation TopologyReplacement VertexPatchSpec VertexWeightSpec',
        'resources': 'ImageCreateSpec',
        'commands': 'ModelingIntent ModelingPlanReport',
        'queries': 'ModelingQuery OwnedModelingRecords',
        'sampling': 'GeometryBounds GeometryChecks GeometryDiagnostic GeometryDiagnosticKind GeometrySnapshot GeometrySpace MeshQuality ModelingSamples TriangleQuality',
        'parameters': 'ParameterCreateSpec ParameterGroupCreateSpec ParameterOperation ParameterPatch',
        'physics': 'PhysicsCollection',
        'scene': 'GluePairSpec OffscreenFormSpec PartCreateSpec SceneOperation ScenePlacement SceneSummary',
        'observation': 'TextureCoverage TopologyOverlay TopologyThresholds',
    },
    'zipzi_contract::document': {
        'scene': 'AttachmentEdit AttachmentRef DrawOrderGroup HierarchyOrder OffscreenDeletePolicy OffscreenMappingPatch OrganizationCollection OrganizationDeletePolicy OrganizationEdit ReparentContext ReparentEdit ReparentPlacement ReparentStrategy SceneCollection SceneContentEdit SceneContentTarget SceneContentValue SceneCopyScope SceneCreate SceneEdit SceneField SceneFieldValue ShapeTarget WarpPointPatch',
        'bindings': 'AxisReduction BindingGridEdit BindingGridTarget BlendAuthoringEdit BlendTableOperation GridOperation MeshGridEdit PartKeyformSlot',
        'project': 'CdiCombinedSet CdiNamespaceIds CdiParameterEntry CdiParameterGroup CdiParameterRef CdiPartEntry DisplayInfo DisplayInfoOrigin DocumentAuthoringEdit DocumentField DocumentValue EditorState Model3Settings ModelHitArea ModelParameterGroup ModelTargetRef ObjectEditorState PackageAttachment',
        'commands': 'ConversionCost ConversionDomain',
        'deletion': 'DeletionChannel DeletionChoice DeletionDecision DeletionDecisionReason DeletionEffect DeletionField DeletionKind DeletionLock DeletionObject DeletionOffscreenPlacement DeletionReason DeletionRelation DeletionRemoval DeletionRequest DeletionStructuralBlocker DeletionTarget DeletionTolerance',
        'expression': 'ExpressionAsset ExpressionBlend ExpressionEdit ExpressionEntry ExpressionEntryFields ExpressionTarget',
        'motion': 'MotionClip MotionContainer MotionEdit MotionEvent MotionGroup MotionKey MotionPoint MotionRegistration MotionSegment MotionTrack MotionTrackTarget',
        'mesh': 'NewMeshVertex VertexContentPatch VertexDeletePolicy VertexSeed VertexWeight',
        'parameters': 'ParameterRangeEdit ParameterRangePolicy',
        'pose': 'PoseAsset PoseCollection PoseEdit PoseEntry PoseFields PoseGroup PoseLink PosePartRef',
        'addressing': 'ReferenceCollection ReferenceEdit ReferenceEntry<zipzi_contract::document::CdiParameterRef> ReferenceEntry<zipzi_contract::document::ModelTargetRef> ReferenceFields ReferenceSelectionField',
        'resources': 'ResourceEdit',
    },
    'zipzi_contract::evaluation': {
        'sampling': 'Drawable DrawableFrame EvaluatedDeformer EvaluatedParameter OffscreenFrame RenderCommand',
    },
    'zipzi_contract::model': {
        'scene': 'Appearance BlendMode Canvas Glue GlueVertexPair ModelSetup Offscreen Part PartId RotationPose RotationTransform Transform TransformData TransformId WarpTransform',
        'bindings': 'BindingAxis BindingSample BlendShapeBinding BlendShapeConstraint BlendShapeKeyTable BlendShapeTargetKind ConstraintSample DeltaGlueKeyform DeltaKeyforms DeltaMeshKeyform DeltaOffscreenKeyform DeltaPartKeyform DeltaRotationKeyform DeltaWarpKeyform GlueBinding GlueKeyform MeshBinding MeshKeyform OffscreenKeyform PartKeyform RotationKeyform SceneBinding SceneKeyform SceneTrack WarpKeyform',
        'project': 'ExportSettings',
        'resources': 'ImageAsset ImageContent',
        'mesh': 'Mesh MeshConstraint MeshConstraintKind MeshMeshing',
        'parameters': 'Parameter ParameterKind',
        'common': 'PreciseVec2 Vec2',
    },
    'zipzi_contract::physics': {
        'physics': 'PhysicsAxis PhysicsDictionaryEntry PhysicsForces PhysicsMeta PhysicsNormalization PhysicsRange PhysicsVector',
    },
    'zipzi_contract::physics_document': {
        'physics': 'PhysicsAsset PhysicsAssetProperty PhysicsDefinition PhysicsEdit PhysicsInput PhysicsNode PhysicsOutput PhysicsPatch PhysicsProperty PhysicsRigPatch PhysicsSetting PhysicsTarget',
    },
    'zipzi_contract::project': {
        'project': 'ExportFormat ImportFormat OpaqueExportIssue OpaqueExportProblem ProjectDiagnostic ProjectReport ProjectRequest TextureSlot',
    },
    'zipzi_contract::runtime': {
        'addressing': 'AddressPatch AddressSelector Capabilities CapabilityDomain',
        'queries': 'CollectionQuery MeshQuery PlanQuery',
        'runtime': 'ClientId EditorContext EventCursor EventKind ObjectSummary RuntimeCapabilities RuntimeEvent RuntimeLimits',
        'resources': 'ImagePixels ImageQuery ImageSnapshot PreparedImageInfo PreparedPixelsQuery ResourceOperation ResourceUsage',
        'mesh': 'MeshSnapshot',
        'commands': 'ModelingCommand PlanIntent PlanSnapshot',
        'sampling': 'ModelingSampleData',
        'observation': 'ObservationBatch ObservationImage ObservationView ObservedBounds ObservationTexturePolicy ObservationTextureSize ObservationContentIdentity ObservationTextureSource ObservationTextureTarget ObservationTexturePlan ObservationTexturePlanning ObservationTextureReport ObservationTextureReportEntry ObservationEstimate ObservationEstimateIssue',
        'preview': 'PreviewBatch PreviewCommand PreviewFrame PreviewState',
    },
    'zipzi_contract::scene_reference': {'scene': 'SceneReference'},
    'zipzi_contract::spatial': {'spatial': 'GeometryHit GeometryQuery ObjectBounds ObjectTarget SpatialFrame SpatialQuery SpatialResult SpatialSamples TriangleHit'},
    'zipzi_contract::warp_bezier': {'scene': 'WarpBezier WarpBezierNode'},
}


def _build_owners():
    result = {}
    for namespace, domains in DOMAIN_MEMBERS.items():
        for domain, names in domains.items():
            for name in names.split():
                identity = namespace+'::'+name
                if identity in result:
                    raise ValueError('Conflicting domain projection: '+identity)
                result[identity] = domain
    return result


OWNERS = _build_owners()
