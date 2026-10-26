//! Bounded metadata discovery and atomic, source-checked selection.
use super::*;
use crate::authoring::*;
use serde::{Deserialize, Serialize};
use zipzi_core::binary_wire::{Reader, WireRead, WireWrite};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(rename_all = "snake_case")]
pub enum CollectionDomain {
    Meshes,
    Parameters,
    Bindings,
    Resources,
    Scene,
}

/// Small owned metadata. Collection reads never copy geometry, keyforms or pixels.
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct ObjectSummary {
    /// Persistent canonical UUID in the selected document.
    pub id: String,
    /// Exact, case-sensitive display name; binding names use their mesh name.
    pub name: String,
    /// Stable SDK kind tag, e.g. mesh, parameter, mesh_binding or image.
    pub object_kind: String,
    /// SDK runtime label; empty when this kind has no runtime label.
    pub runtime_id: String,
    /// Owning Part UUID when applicable; None means no Part ownership.
    pub part_id: Option<String>,
    /// Deformation or organization parent UUID when applicable.
    pub parent_id: Option<String>,
    /// Whether the scene object is enabled; true for metadata without that field.
    pub enabled: bool,
    /// Bound mesh UUID for mesh bindings; None for other collections.
    pub mesh_id: Option<String>,
    /// Decoded image width in pixels when known, without reading pixels.
    pub width: Option<u32>,
    /// Decoded image height in pixels when known, without reading pixels.
    pub height: Option<u32>,
    /// Number of mesh vertices, without cloning vertex data.
    pub vertex_count: Option<usize>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionQuery {
    pub session: SessionId,
    pub edit: Option<EditId>,
    pub domain: CollectionDomain,
    pub name: Option<String>,
    pub limit: usize,
    pub cursor: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct CollectionPage {
    pub source: SourceRef,
    pub items: Vec<ObjectSummary>,
    pub next_cursor: Option<String>,
}
#[derive(zipzi_api_derive::BinaryWire)]
struct Cursor {
    source: SourceRef,
    domain: CollectionDomain,
    name: Option<String>,
    position: usize,
}

impl Cursor {
    fn token(&self) -> Result<String> {
        let mut bytes = Vec::new();
        self.encode(&mut bytes)
            .map_err(|_| RuntimeError::InvalidReference)?;
        let mut token = String::with_capacity(bytes.len() * 2);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in bytes {
            token.push(HEX[(byte >> 4) as usize] as char);
            token.push(HEX[(byte & 15) as usize] as char);
        }
        Ok(token)
    }
    fn read(token: &str) -> Result<Self> {
        if token.len() % 2 != 0 || !token.is_ascii() {
            return Err(RuntimeError::InvalidReference);
        }
        let bytes = token
            .as_bytes()
            .chunks_exact(2)
            .map(|v| {
                let digit = |v: u8| -> Result<u8> {
                    match v {
                        b'0'..=b'9' => Ok(v - b'0'),
                        b'a'..=b'f' => Ok(v - b'a' + 10),
                        _ => Err(RuntimeError::InvalidReference),
                    }
                };
                Ok((digit(v[0])? << 4) | digit(v[1])?)
            })
            .collect::<Result<Vec<u8>>>()?;
        let mut measured = Reader::new(&bytes).map_err(|_| RuntimeError::InvalidReference)?;
        if Self::input_cost(&mut measured).map_err(|_| RuntimeError::InvalidReference)? > 65536 {
            return Err(RuntimeError::Budget("cursor expansion"));
        }
        measured
            .finish()
            .map_err(|_| RuntimeError::InvalidReference)?;
        let mut reader = Reader::new(&bytes).map_err(|_| RuntimeError::InvalidReference)?;
        let value = Self::decode(&mut reader).map_err(|_| RuntimeError::InvalidReference)?;
        reader
            .finish()
            .map_err(|_| RuntimeError::InvalidReference)?;
        Ok(value)
    }
}

/// Runtime implementation capabilities, independent of the cached connection handshake.
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct RuntimeCapabilities {
    /// Finite domain names supported by this host; absence means the domain is unavailable.
    pub domains: Vec<String>,
    /// Whether this host supports source-bound bounded collection pagination.
    pub pagination: bool,
    /// Whether selected reads accept an exact expected Source.
    pub source_checked_reads: bool,
    /// Whether this host supports explicit Session detach/close.
    pub session_close: bool,
    /// Whether this host supports the paired binary array transport.
    pub binary_arrays: bool,
    /// Whether Session window display is supported by this host.
    pub editor_windows: bool,
    /// Whether GPU operations are available from this host; CPU sampling is independent.
    pub gpu: bool,
}

pub enum Selection {
    Meshes(MeshBatch),
    Parameters(ParameterBatch),
    Bindings(MeshBindingBatch),
    Resources(SourceRef, Vec<ImageSnapshot>),
    Scene(SceneBatch),
}

impl Runtime {
    pub fn create_session_auto(
        &mut self,
        client: ClientId,
        document_id: Option<&str>,
        setup: ModelSetup,
    ) -> Result<SessionId> {
        self.create_session(
            client,
            document_id.unwrap_or(&uuid::Uuid::new_v4().to_string()),
            setup,
        )
    }

    pub fn runtime_capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            domains: vec![
                "meshes",
                "parameters",
                "bindings",
                "resources",
                "scene",
                "modeling",
                "sampling",
                "events",
                "references",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            pagination: true,
            source_checked_reads: true,
            session_close: true,
            binary_arrays: true,
            editor_windows: false,
            gpu: false,
        }
    }
    pub fn check_authoring_source(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        expected: Option<SourceRef>,
    ) -> Result<SourceRef> {
        self.with_authoring_source(client, session, edit, |_, source| {
            if expected.is_some_and(|s| s != source) {
                return Err(RuntimeError::Sdk(zipzi_sdk::SdkError {
                    code: "STALE_SOURCE".into(),
                    message: "The collection source changed; read again from its first page".into(),
                    operation: "collection.read",
                    object_ids: vec![],
                    field_path: None,
                    expected_version: expected.map(|s| Box::new(s.version())),
                    actual_version: Some(Box::new(source.version())),
                    referrers: Box::new([]),
                }));
            }
            Ok(source)
        })
    }
    pub fn collection_page(
        &self,
        client: ClientId,
        input: CollectionQuery,
    ) -> Result<CollectionPage> {
        if input.limit == 0 || input.limit > self.limits.collection_records {
            return Err(RuntimeError::Budget("collection_records"));
        }
        if input.name.as_ref().is_some_and(|s| s.len() > 4096)
            || input.cursor.as_ref().is_some_and(|s| s.len() > 65536)
        {
            return Err(RuntimeError::Budget("collection selector bytes"));
        }
        let cursor = input
            .cursor
            .as_deref()
            .map(Cursor::read)
            .transpose()
            .map_err(|_| RuntimeError::InvalidReference)?;
        if cursor
            .as_ref()
            .is_some_and(|c| c.domain != input.domain || c.name != input.name)
        {
            return Err(RuntimeError::InvalidReference);
        }
        let source = self.check_authoring_source(
            client,
            input.session,
            input.edit,
            cursor.as_ref().map(|c| c.source),
        )?;
        self.check_bytes(self.limits.collection_bytes.saturating_mul(4), 0, 0)?;
        self.with_authoring_source(client, input.session, input.edit, |doc, _| {
            // Borrow SDK storage order; no model clone and no full selection list.
            let groups: Vec<&[String]> = match input.domain {
                CollectionDomain::Meshes => vec![doc.mesh_order()],
                CollectionDomain::Parameters => vec![doc.parameter_order()],
                CollectionDomain::Bindings => vec![doc.binding_order()],
                CollectionDomain::Resources => vec![doc.asset_order()],
                CollectionDomain::Scene => vec![
                    doc.mesh_order(),
                    doc.transform_order(),
                    doc.part_order(),
                    doc.glue_order(),
                    doc.offscreen_order(),
                ],
            };
            let group_ids = doc
                .display_info()
                .parameter_groups
                .as_deref()
                .unwrap_or_default();
            let groups_len = groups.iter().map(|g| g.len()).sum::<usize>();
            let total = groups_len
                + if input.domain == CollectionDomain::Scene {
                    group_ids.len()
                } else {
                    0
                };
            let mut position = cursor.as_ref().map_or(0, |c| c.position);
            if position > total {
                return Err(RuntimeError::InvalidReference);
            }
            let mut items = Vec::with_capacity(input.limit);
            let mut bytes = 4096usize;
            let mut scanned = 0;
            while position < total
                && items.len() < input.limit
                && scanned < self.limits.collection_scan
            {
                let mut offset = position;
                let id = if position >= groups_len {
                    group_ids[position - groups_len].id.as_str()
                } else {
                    let mut found = None;
                    for group in &groups {
                        if offset < group.len() {
                            found = Some(group[offset].as_str());
                            break;
                        }
                        offset -= group.len();
                    }
                    found.ok_or(RuntimeError::InvalidReference)?
                };
                let (name, text_bytes) = match input.domain {
                    CollectionDomain::Meshes => {
                        let v = doc.get_mesh(id).ok_or(RuntimeError::InvalidReference)?;
                        (
                            v.name.as_str(),
                            zipzi_sdk::SceneSummary::response_bytes(doc, id).unwrap_or(4096),
                        )
                    }
                    CollectionDomain::Parameters => {
                        let v = doc
                            .get_parameter(id)
                            .ok_or(RuntimeError::InvalidReference)?;
                        (
                            v.name.as_str(),
                            4096 + (v.id.len() + v.name.len() + v.runtime_id.len()) * 8,
                        )
                    }
                    CollectionDomain::Bindings => {
                        let v = doc.get_binding(id).ok_or(RuntimeError::InvalidReference)?;
                        let m = doc
                            .get_mesh(&v.mesh_id)
                            .ok_or(RuntimeError::InvalidReference)?;
                        (
                            m.name.as_str(),
                            4096 + (id.len() + v.mesh_id.len() + m.name.len()) * 8,
                        )
                    }
                    CollectionDomain::Resources => {
                        let v = doc.get_asset(id).ok_or(RuntimeError::InvalidReference)?;
                        (v.name.as_str(), 4096 + (v.id.len() + v.name.len()) * 8)
                    }
                    CollectionDomain::Scene => {
                        let v = doc
                            .get_mesh(id)
                            .map(|v| v.name.as_str())
                            .or_else(|| doc.get_transform(id).map(|v| v.name.as_str()))
                            .or_else(|| doc.get_part(id).map(|v| v.name.as_str()))
                            .or_else(|| doc.get_glue(id).map(|v| v.name.as_str()))
                            .or_else(|| doc.get_offscreen(id).map(|v| v.name.as_str()))
                            .or_else(|| doc.get_parameter_group(id).map(|v| v.name.as_str()))
                            .ok_or(RuntimeError::InvalidReference)?;
                        (
                            v,
                            zipzi_sdk::SceneSummary::response_bytes(doc, id).unwrap_or(4096),
                        )
                    }
                };
                scanned += 1;
                if input.name.as_deref().is_some_and(|n| n != name) {
                    position += 1;
                    continue;
                }
                if bytes.saturating_add(text_bytes) > self.limits.collection_bytes {
                    if items.is_empty() {
                        return Err(RuntimeError::Budget("collection_bytes"));
                    }
                    break;
                }
                bytes += text_bytes;
                let mut value = ObjectSummary {
                    id: id.into(),
                    name: name.into(),
                    object_kind: String::new(),
                    runtime_id: String::new(),
                    part_id: None,
                    parent_id: None,
                    enabled: true,
                    mesh_id: None,
                    width: None,
                    height: None,
                    vertex_count: None,
                };
                match input.domain {
                    CollectionDomain::Meshes | CollectionDomain::Scene => {
                        let s = zipzi_sdk::SceneSummary::read(doc, id)
                            .ok_or(RuntimeError::InvalidReference)?;
                        value.object_kind = s.object_kind;
                        value.runtime_id = s.runtime_id;
                        value.part_id = s.part_id;
                        value.parent_id = s.parent_id;
                        value.enabled = s.enabled;
                        if let Some(m) = doc.get_mesh(id) {
                            value.vertex_count = Some(m.vertex_ids.len());
                        }
                    }
                    CollectionDomain::Parameters => {
                        value.object_kind = "parameter".into();
                        value.runtime_id = doc.get_parameter(id).unwrap().runtime_id.clone();
                    }
                    CollectionDomain::Bindings => {
                        value.object_kind = "mesh_binding".into();
                        value.mesh_id = Some(doc.get_binding(id).unwrap().mesh_id.clone());
                    }
                    CollectionDomain::Resources => {
                        value.object_kind = "image".into();
                        if let Some(c) = &doc.get_asset(id).unwrap().content {
                            value.width = Some(c.width);
                            value.height = Some(c.height);
                        }
                    }
                }
                items.push(value);
                position += 1;
            }
            let next_cursor = if position < total {
                Some(
                    Cursor {
                        source,
                        domain: input.domain,
                        name: input.name.clone(),
                        position,
                    }
                    .token()?,
                )
            } else {
                None
            };
            Ok(CollectionPage {
                source,
                items,
                next_cursor,
            })
        })
    }
    pub fn read_selection(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        domain: CollectionDomain,
        ids: Vec<String>,
        expected: Option<SourceRef>,
    ) -> Result<Selection> {
        let source = self.check_authoring_source(client, session, edit, expected)?;
        if ids.len() > self.limits.collection_records {
            return Err(RuntimeError::Budget("collection_records"));
        }
        match domain {
            CollectionDomain::Meshes => Ok(Selection::Meshes(
                self.mesh_batch(client, MeshBatchQuery { session, edit, ids })?,
            )),
            CollectionDomain::Parameters => Ok(Selection::Parameters(self.parameter_snapshot(
                client,
                ParameterQuery {
                    session,
                    edit,
                    ids: Some(ids),
                },
            )?)),
            CollectionDomain::Bindings => Ok(Selection::Bindings(self.mesh_binding_snapshot(
                client,
                MeshBindingQuery {
                    session,
                    edit,
                    ids: Some(ids),
                },
            )?)),
            CollectionDomain::Scene => Ok(Selection::Scene(
                self.scene_snapshot(client, SceneQuery { session, edit, ids })?,
            )),
            CollectionDomain::Resources => {
                // Preflight the complete selection before taking any pixel owners.
                self.with_authoring_source(client, session, edit, |doc, _| {
                    let mut size = 4096usize;
                    for id in &ids {
                        let v = doc.get_asset(id).ok_or_else(|| {
                            RuntimeError::Sdk(zipzi_sdk::SdkError {
                                code: "MISSING_IMAGE".into(),
                                message: "Image does not exist".into(),
                                operation: "resources.get_many",
                                object_ids: vec![id.clone()],
                                field_path: None,
                                expected_version: None,
                                actual_version: None,
                                referrers: Box::new([]),
                            })
                        })?;
                        let c = v
                            .content
                            .as_ref()
                            .ok_or(RuntimeError::Budget("image snapshot"))?;
                        size = size.saturating_add(
                            (c.width as usize)
                                .saturating_mul(c.height as usize)
                                .saturating_mul(4),
                        );
                    }
                    if size > 256 << 20 {
                        return Err(RuntimeError::Budget("image selection bytes"));
                    }
                    self.check_bytes(size.saturating_mul(4), 0, 0)
                })?;
                Ok(Selection::Resources(
                    source,
                    ids.iter()
                        .map(|id| self.image_snapshot(client, session, edit, id))
                        .collect::<Result<Vec<_>>>()?,
                ))
            }
        }
    }
    pub fn image_selection_blob(
        &mut self,
        client: ClientId,
        source: SourceRef,
        images: Vec<ImageSnapshot>,
    ) -> Result<serde_json::Value> {
        let length = zipzi_core::binary_wire::encoded_len(&images)
            .map_err(|_| RuntimeError::Budget("image selection binary"))?;
        let _lease = self.reserve_transfer(length.saturating_mul(2))?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| RuntimeError::Budget("image selection allocation"))?;
        images
            .encode(&mut data)
            .map_err(|_| RuntimeError::InvalidReference)?;
        let blob = self.retain_blob(client, data.into())?;
        Ok(
            serde_json::json!({"source":source,"blob":blob,"length":length,"signature":format!("{:032x}",Vec::<ImageSnapshot>::SIGNATURE)}),
        )
    }
}
