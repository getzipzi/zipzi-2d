//! Bounded source-preserving reads of stable object identities.
use crate::types::error;
use serde::Serialize;
use serde_json::{json, Value};
use zipzi_contract::kernel::AssetRecord;
use zipzi_contract::{
    authoring::ObjectKind, document::*, kernel_wire::Query, model::*, physics_document::*,
};
use zipzi_core::state::DocumentState;
use zipzi_sdk::{SourcedSnapshot, Status};
trait Record {
    fn id(&self) -> &str;
    fn value(&self, remaining: &mut usize) -> Result<Value, Status>;
}
pub(crate) fn bounded(value: &impl Serialize, remaining: &mut usize) -> Result<Value, Status> {
    struct Count(usize, usize);
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(b.len())
                .filter(|n| *n <= self.1)
                .ok_or_else(|| std::io::Error::other("query output limit"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0, *remaining);
    serde_json::to_writer(&mut count, value)
        .map_err(|_| error("RESOURCE_LIMIT", "Response exceeds its result bound"))?;
    *remaining -= count.0;
    serde_json::to_value(value).map_err(|e| Status::error("QUERY_ENCODING", e.to_string()))
}
macro_rules! record {
    ($($t:ty),*) => { $(impl Record for $t {
        fn id(&self) -> &str { &self.id }
        fn value(&self, remaining: &mut usize) -> Result<Value, Status> { bounded(self, remaining) }
    })* };
}
record!(
    AssetRecord,
    Parameter,
    Part,
    Transform,
    Mesh,
    MeshBinding,
    SceneBinding,
    BlendShapeKeyTable,
    BlendShapeConstraint,
    BlendShapeBinding,
    Glue,
    Offscreen,
    ExpressionAsset,
    MotionClip,
    BindingAxis,
    BindingSample,
    ConstraintSample,
    GlueVertexPair,
    CdiParameterGroup,
    CdiCombinedSet,
    ModelParameterGroup,
    ModelHitArea,
    ExpressionEntry,
    MotionGroup,
    MotionRegistration,
    MotionTrack,
    MotionKey,
    MotionEvent,
    PackageAttachment,
    PoseAsset,
    PoseGroup,
    PoseEntry,
    PoseLink,
    PhysicsAsset,
    PhysicsSetting,
    PhysicsInput,
    PhysicsOutput,
    PhysicsNode
);
impl<T: Serialize> Record for ReferenceEntry<T> {
    fn id(&self) -> &str {
        &self.id
    }
    fn value(&self, remaining: &mut usize) -> Result<Value, Status> {
        bounded(self, remaining)
    }
}
impl Record for DrawOrderGroup {
    fn id(&self) -> &str {
        &self.owner
    }
    fn value(&self, remaining: &mut usize) -> Result<Value, Status> {
        bounded(self, remaining)
    }
}
fn records(s: &DocumentState, kind: ObjectKind) -> Box<dyn Iterator<Item = &dyn Record> + '_> {
    macro_rules! iter {
        ($e:expr) => {
            Box::new($e.map(|v| v as &dyn Record))
        };
    }
    match kind {
        ObjectKind::Asset => iter!(s.asset_order().iter().filter_map(|id| s.asset(id))),
        ObjectKind::Parameter => iter!(s.parameter_order().iter().filter_map(|id| s.parameter(id))),
        ObjectKind::Part => iter!(s.part_order().iter().filter_map(|id| s.part(id))),
        ObjectKind::Transform => iter!(s.transform_order().iter().filter_map(|id| s.transform(id))),
        ObjectKind::Mesh => iter!(s.mesh_order().iter().filter_map(|id| s.mesh(id))),
        ObjectKind::MeshBinding => iter!(s.binding_order().iter().filter_map(|id| s.binding(id))),
        ObjectKind::SceneBinding => iter!(s
            .scene_binding_order()
            .iter()
            .filter_map(|id| s.scene_binding(id))),
        ObjectKind::BlendKeyTable => iter!(s
            .blend_key_table_order()
            .iter()
            .filter_map(|id| s.blend_key_table(id))),
        ObjectKind::BlendConstraint => iter!(s
            .blend_constraint_order()
            .iter()
            .filter_map(|id| s.blend_constraint(id))),
        ObjectKind::BlendBinding => iter!(s
            .blend_binding_order()
            .iter()
            .filter_map(|id| s.blend_binding(id))),
        ObjectKind::Glue => iter!(s.glue_order().iter().filter_map(|id| s.glue(id))),
        ObjectKind::Offscreen => iter!(s.offscreen_order().iter().filter_map(|id| s.offscreen(id))),
        ObjectKind::Expression => iter!(s
            .expression_order()
            .iter()
            .filter_map(|id| s.expression(id))),
        ObjectKind::Motion => iter!(s.motion_order().iter().filter_map(|id| s.motion(id))),
        ObjectKind::BindingAxis => iter!(s
            .binding_order()
            .iter()
            .filter_map(|id| s.binding(id))
            .flat_map(|b| b.axes.iter())
            .chain(
                s.scene_binding_order()
                    .iter()
                    .filter_map(|id| s.scene_binding(id))
                    .flat_map(|b| b.axes.iter())
            )
            .chain(
                s.glue_order()
                    .iter()
                    .filter_map(|id| s.glue(id))
                    .filter_map(|g| g.binding.as_ref())
                    .flat_map(|b| b.axes.iter())
            )),
        ObjectKind::BindingSample => Box::new(
            s.binding_order()
                .iter()
                .filter_map(|id| s.binding(id))
                .flat_map(|b| b.axes.iter())
                .flat_map(|a| a.samples.iter())
                .map(|v| v as &dyn Record)
                .chain(
                    s.scene_binding_order()
                        .iter()
                        .filter_map(|id| s.scene_binding(id))
                        .flat_map(|b| b.axes.iter())
                        .flat_map(|a| a.samples.iter())
                        .map(|v| v as &dyn Record),
                )
                .chain(
                    s.glue_order()
                        .iter()
                        .filter_map(|id| s.glue(id))
                        .filter_map(|g| g.binding.as_ref())
                        .flat_map(|b| b.axes.iter())
                        .flat_map(|a| a.samples.iter())
                        .map(|v| v as &dyn Record),
                )
                .chain(
                    s.blend_key_table_order()
                        .iter()
                        .filter_map(|id| s.blend_key_table(id))
                        .flat_map(|t| t.samples.iter())
                        .map(|v| v as &dyn Record),
                )
                .chain(
                    s.blend_constraint_order()
                        .iter()
                        .filter_map(|id| s.blend_constraint(id))
                        .flat_map(|t| t.samples.iter())
                        .map(|v| v as &dyn Record),
                ),
        ),
        ObjectKind::GlueVertexPair => iter!(s
            .glue_order()
            .iter()
            .filter_map(|id| s.glue(id))
            .flat_map(|v| v.pairs.iter())),
        ObjectKind::DrawOrderGroup => iter!(s.draw_order_groups().iter().flat_map(|v| v.iter())),
        ObjectKind::CdiParameterGroup => iter!(s
            .display_info()
            .parameter_groups
            .iter()
            .flat_map(|v| v.iter())),
        ObjectKind::CdiCombinedSet => iter!(s
            .display_info()
            .combined_parameters
            .iter()
            .flat_map(|v| v.iter())),
        ObjectKind::CdiCombinedMember => iter!(s
            .display_info()
            .combined_parameters
            .iter()
            .flat_map(|v| v.iter())
            .flat_map(|v| v.members.iter())),
        ObjectKind::ModelParameterGroup => {
            iter!(s.model3_settings().groups.iter().flat_map(|v| v.iter()))
        }
        ObjectKind::ModelParameterMember => iter!(s
            .model3_settings()
            .groups
            .iter()
            .flat_map(|v| v.iter())
            .flat_map(|v| v.parameters.iter())),
        ObjectKind::ModelHitArea => {
            iter!(s.model3_settings().hit_areas.iter().flat_map(|v| v.iter()))
        }
        ObjectKind::ExpressionEntry => iter!(s
            .expression_order()
            .iter()
            .filter_map(|id| s.expression(id))
            .flat_map(|v| v.entries.iter())),
        ObjectKind::MotionGroup => iter!(s.motion_groups().iter()),
        ObjectKind::MotionRegistration => iter!(s
            .motion_groups()
            .iter()
            .flat_map(|v| v.entries.iter())
            .chain(s.motion_roots().iter())),
        ObjectKind::MotionTrack => iter!(s
            .motion_order()
            .iter()
            .filter_map(|id| s.motion(id))
            .flat_map(|v| v.tracks.iter())),
        ObjectKind::MotionKey => iter!(s
            .motion_order()
            .iter()
            .filter_map(|id| s.motion(id))
            .flat_map(|v| v.tracks.iter())
            .flat_map(|v| v.keys())),
        ObjectKind::MotionEvent => iter!(s
            .motion_order()
            .iter()
            .filter_map(|id| s.motion(id))
            .flat_map(|v| v.events.iter())),
        ObjectKind::Attachment => iter!(s.package_attachments().iter()),
        ObjectKind::Pose => iter!(s.pose().iter().map(|v| v.as_ref())),
        ObjectKind::PoseGroup => iter!(s.pose().iter().flat_map(|v| v.groups.iter())),
        ObjectKind::PoseEntry => iter!(s
            .pose()
            .iter()
            .flat_map(|v| v.groups.iter())
            .flat_map(|v| v.entries.iter())),
        ObjectKind::PoseLink => iter!(s
            .pose()
            .iter()
            .flat_map(|v| v.groups.iter())
            .flat_map(|v| v.entries.iter())
            .flat_map(|v| v.links.iter())
            .flat_map(|v| v.iter())),
        ObjectKind::Physics => iter!(s.physics().iter().map(|v| v.as_ref())),
        ObjectKind::PhysicsGroup => iter!(s
            .physics()
            .iter()
            .flat_map(|v| v.data.settings.iter())
            .map(|v| v.as_ref())),
        ObjectKind::PhysicsInput => iter!(s
            .physics()
            .iter()
            .flat_map(|v| v.data.settings.iter())
            .flat_map(|v| v.inputs.iter())),
        ObjectKind::PhysicsOutput => iter!(s
            .physics()
            .iter()
            .flat_map(|v| v.data.settings.iter())
            .flat_map(|v| v.outputs.iter())),
        ObjectKind::PhysicsNode => iter!(s
            .physics()
            .iter()
            .flat_map(|v| v.data.settings.iter())
            .flat_map(|v| v.vertices.iter())),
    }
}
pub(crate) fn read(
    snapshot: &SourcedSnapshot,
    query: Query,
    max_bytes: usize,
) -> Result<Value, Status> {
    let s = snapshot.snapshot.document();
    let mut remaining = max_bytes;
    let value = match query {
        Query::CollectionCapabilities { target } => {
            bounded(&s.collection_capabilities(&target)?, &mut remaining)?
        }
        Query::Summary => {
            json!({"id":s.id(),"setup":s.setup(),"meshes":s.mesh_order().len(),"parameters":s.parameter_order().len(),"assets":s.asset_order().len()})
        }
        Query::Metadata => {
            #[derive(Serialize)]
            struct Metadata<'a> {
                editor_state: &'a EditorState,
                display_info: &'a DisplayInfo,
                model3_settings: &'a Model3Settings,
                hierarchy_order: &'a HierarchyOrder,
                missing_attachments: &'a [String],
            }
            bounded(
                &Metadata {
                    editor_state: s.editor_state(),
                    display_info: s.display_info(),
                    model3_settings: s.model3_settings(),
                    hierarchy_order: s.hierarchy_order(),
                    missing_attachments: s.missing_attachments(),
                },
                &mut remaining,
            )?
        }
        Query::Objects { addresses } => {
            if addresses.len() > 256 {
                return Err(error("RESOURCE_LIMIT", "Select at most 256 records"));
            }
            let mut values = Vec::with_capacity(addresses.len());
            for address in addresses {
                let record = records(s, address.kind)
                    .find(|r| r.id() == address.id)
                    .ok_or_else(|| {
                        error(
                            "MISSING_OBJECT",
                            "Object ID does not exist in the requested kind",
                        )
                    })?;
                values.push(json!({"address":address,"value":record.value(&mut remaining)?}));
            }
            json!({"records":values})
        }
        Query::Collection {
            kind,
            offset,
            limit,
        } => {
            if limit == 0 || limit > 256 {
                return Err(error("RESOURCE_LIMIT", "Page size must be 1–256"));
            }
            let mut iter = records(s, kind).skip(offset);
            let mut values = vec![];
            for record in iter.by_ref().take(limit) {
                values.push(json!({"address":{"kind":kind,"id":record.id()},"value":record.value(&mut remaining)?}));
            }
            let next = if iter.next().is_some() {
                Some(
                    offset
                        .checked_add(values.len())
                        .ok_or_else(|| error("RESOURCE_LIMIT", "Page offset overflow"))?,
                )
            } else {
                None
            };
            json!({"records":values,"next_offset":next})
        }
    };
    Ok(json!({"source":snapshot.source,"content":snapshot.snapshot.content_id(),"value":value}))
}
