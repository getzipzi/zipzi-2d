//! Selected mesh arrays are encoded from immutable borrowed SDK fields.
use super::*;
use crate::authoring::MeshSnapshot;
use zipzi_core::binary_wire::WireWrite;
#[derive(zipzi_api_derive::BinaryOutput)]
struct BorrowedGeometry<'a> {
    vertex_ids: &'a [u32],
    positions: &'a [Vec2],
    uvs: &'a [Vec2],
    triangles: &'a [[u32; 3]],
}
struct GeometryRef<'a>(BorrowedGeometry<'a>);
impl WireWrite for GeometryRef<'_> {
    const SIGNATURE: u128 = zipzi_sdk::MeshGeometry::SIGNATURE;
    fn encode<W: std::io::Write>(&self, out: &mut W) -> zipzi_core::binary_wire::Result<()> {
        self.0.encode(out)
    }
    fn expansion_bytes(&self) -> usize {
        self.0.expansion_bytes()
    }
}
#[derive(zipzi_api_derive::BinaryOutput)]
struct BorrowedMesh<'a> {
    source: SourceRef,
    id: &'a str,
    name: &'a str,
    geometry: GeometryRef<'a>,
}
const _: () = assert!(BorrowedGeometry::FIELD_LAYOUT == zipzi_sdk::MeshGeometry::FIELD_LAYOUT);
const _: () = assert!(BorrowedMesh::FIELD_LAYOUT == MeshSnapshot::FIELD_LAYOUT);
impl Runtime {
    pub fn mesh_read_bytes(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        ids: &[String],
    ) -> Result<usize> {
        self.with_authoring_source(client, session, edit, |doc, _| {
            if ids.len() > 256 {
                return Err(RuntimeError::Budget("mesh batch count"));
            }
            let mut bytes = 4096usize;
            for id in ids {
                let mesh = doc.get_mesh(id).ok_or_else(|| {
                    RuntimeError::Sdk(zipzi_sdk::SdkError {
                        code: "MISSING_MESH".into(),
                        message: "Mesh does not exist".into(),
                        operation: "meshes.get",
                        object_ids: vec![id.clone()],
                        field_path: None,
                        expected_version: None,
                        actual_version: None,
                        referrers: Box::new([]),
                    })
                })?;
                bytes = bytes
                    .saturating_add(
                        mesh.base_positions
                            .len()
                            .saturating_add(mesh.uvs.len())
                            .saturating_mul(512),
                    )
                    .saturating_add(mesh.triangles.len().saturating_mul(256))
                    .saturating_add(mesh.vertex_ids.len().saturating_mul(64))
                    .saturating_add(
                        mesh.id
                            .len()
                            .saturating_add(mesh.name.len())
                            .saturating_mul(8),
                    );
            }
            Ok(bytes)
        })
    }
    pub fn mesh_response(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        ids: Vec<String>,
        single: bool,
    ) -> Result<Option<serde_json::Value>> {
        let _indexes = self.reserve_transfer(ids.len().saturating_mul(512).saturating_add(4096))?;
        let bytes = self.mesh_read_bytes(client, session, edit, &ids)?;
        self.check_bytes(bytes, 0, 0)?;
        let output = self.with_authoring_source(client, session, edit, |doc, source| {
            let values = ids
                .iter()
                .map(|id| {
                    let m = doc.get_mesh(id).expect("admitted mesh");
                    BorrowedMesh {
                        source,
                        id: &m.id,
                        name: &m.name,
                        geometry: GeometryRef(BorrowedGeometry {
                            vertex_ids: &m.vertex_ids,
                            positions: &m.base_positions,
                            uvs: &m.uvs,
                            triangles: &m.triangles,
                        }),
                    }
                })
                .collect::<Vec<_>>();
            let length = zipzi_core::binary_wire::encoded_len(&values)
                .map_err(|_| RuntimeError::Budget("binary mesh response"))?;
            if length <= 8192 {
                return Ok(None);
            }
            if self.blobs.values().filter(|b| b.client == client).count()
                >= self.limits.blobs_per_client
                || self
                    .blobs
                    .values()
                    .map(|b| b.length)
                    .sum::<usize>()
                    .saturating_add(length)
                    > self.limits.blob_bytes
            {
                return Err(RuntimeError::Budget("binary mesh blob bytes/count"));
            }
            self.refresh_memory_budget();
            let reservation = self
                .image_budget
                .reserve(length.saturating_mul(2))
                .map_err(|_| RuntimeError::Budget("binary mesh preparation"))?;
            let mut data = Vec::new();
            data.try_reserve_exact(length)
                .map_err(|_| RuntimeError::Budget("binary mesh allocation"))?;
            values
                .encode(&mut data)
                .map_err(|_| RuntimeError::InvalidReference)?;
            let data: Arc<[u8]> = data.into();
            reservation
                .commit(&data, zipzi_project::ImageBufferKind::Blob)
                .map_err(|_| RuntimeError::Budget("binary mesh registration"))?;
            Ok(Some((data, length)))
        })?;
        match output {
            None => Ok(None),
            Some((data, length)) => {
                let blob = self.retain_blob(client, data)?;
                Ok(Some(
                    serde_json::json!({"blob":blob,"length":length,"signature":format!("{:032x}",Vec::<MeshSnapshot>::SIGNATURE),"single":single}),
                ))
            }
        }
    }
}
