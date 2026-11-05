//! Data-only v2 document boundary. Unavailable capability payloads are opaque,
//! never opened as paths, interpreted as code or silently dropped for rendering.
//! Native raster semantics remain those of the existing v1 Project engine.
use crate::{Layer, Project};
use serde::{de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor}, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt, marker::PhantomData};

pub const VERSION: u32 = 2;
pub const MAX_LAYERS: usize = 32;
pub const MAX_OBJECTS: usize = 1024;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
pub const MAX_TOTAL_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: u64 = crate::MAX_PROJECT_BYTES + (2 * MAX_TOTAL_PAYLOAD_BYTES + 256 * 1024) as u64;
pub const MAX_PROPOSAL_BYTES: u64 = 512 * 1024;
pub const MAX_EDIT_UNITS: usize = 1024;
const REVISION_PREFIX: &str = "sha256-document-v2:";
fn fail(message: &str) -> String { format!("document: {message}") }
fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_id(value: &str) -> bool { hex(value, 32) }
fn label(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn exact_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 10
        && (p.len() == 1 || !p.starts_with('0')) && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u32>().is_ok())
}

// Bound sequence allocation during parsing, not merely after a huge Vec exists.
fn bounded_vec<'de, D, T, const MAX: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where D: Deserializer<'de>, T: Deserialize<'de> {
    struct Bounded<T, const MAX: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for Bounded<T, MAX> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str("a bounded sequence") }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while out.len() < MAX {
                match seq.next_element()? { Some(item) => out.push(item), None => return Ok(out) }
            }
            if seq.next_element::<IgnoredAny>()?.is_some() { return Err(de::Error::custom("sequence exceeds limit")); }
            Ok(out)
        }
    }
    deserializer.deserialize_seq(Bounded::<T, MAX>(PhantomData))
}
fn layers<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DocumentLayer>, D::Error> { bounded_vec::<D, _, MAX_LAYERS>(d) }
fn objects<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<CapabilityObject>, D::Error> { bounded_vec::<D, _, MAX_OBJECTS>(d) }
fn pixels<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<[u8; 4]>, D::Error> { bounded_vec::<D, _, { crate::MAX_DIMENSION as usize * crate::MAX_DIMENSION as usize }>(d) }
fn operations<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Operation>, D::Error> { bounded_vec::<D, _, MAX_EDIT_UNITS>(d) }
fn edits<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PixelEdit>, D::Error> { bounded_vec::<D, _, MAX_EDIT_UNITS>(d) }
fn payload_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Payload;
    impl Visitor<'_> for Payload {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str("bounded lowercase hexadecimal bytes") }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() > 2 * MAX_PAYLOAD_BYTES || value.len() % 2 != 0 || !hex(value, value.len()) {
                return Err(E::custom("invalid bounded opaque payload"));
            }
            Ok(value.into())
        }
    }
    d.deserialize_str(Payload)
}

/// Digest is an immutable reference, NOT a signature, installed-state lookup,
/// trust decision or execution permit. No URL, path or command is accepted.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRef {
    pub id: String, pub version: String, pub artifact_sha256: String,
    pub abi_version: u32, pub payload_schema: u32,
}
impl CapabilityRef {
    pub fn validate(&self) -> Result<(), String> {
        if !label(&self.id) || self.version.len() > 64 || !exact_version(&self.version)
            || !hex(&self.artifact_sha256, 64) || self.abi_version == 0 || self.payload_schema == 0 {
            return Err(fail("invalid immutable capability reference"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadEncoding { Hex }
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpaquePayload {
    pub encoding: PayloadEncoding,
    #[serde(deserialize_with = "payload_text")]
    pub data: String,
}
impl fmt::Debug for OpaquePayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.debug_struct("OpaquePayload").field("bytes", &(self.data.len()/2)).finish_non_exhaustive() }
}
impl OpaquePayload {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PAYLOAD_BYTES { return Err(fail("opaque payload exceeds per-object limit")); }
        Ok(Self { encoding: PayloadEncoding::Hex, data: bytes.iter().map(|b| format!("{b:02x}")).collect() })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.data.len() > 2 * MAX_PAYLOAD_BYTES || self.data.len() % 2 != 0 || !hex(&self.data, self.data.len()) {
            return Err(fail("invalid bounded opaque payload"));
        }
        Ok(())
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        fn nibble(b: u8) -> u8 { if b.is_ascii_digit() { b-b'0' } else { b-b'a'+10 } }
        Ok(self.data.as_bytes().chunks_exact(2).map(|p| nibble(p[0])*16+nibble(p[1])).collect())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityObject { pub id: String, pub payload: OpaquePayload }
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Raster { pixels: Vec<[u8; 4]> },
    Capability { capability: CapabilityRef, objects: Vec<CapabilityObject> },
}
// A derived internally-tagged enum first buffers arbitrary serde Content. That
// would bypass the sequence allocation bounds until AFTER a huge pixel/object
// tree was allocated. Parse typed fields directly, in any field order, instead.
impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field { Kind, Pixels, Capability, Objects }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind { Raster, Capability }
        #[derive(Deserialize)]
        struct Pixels(#[serde(deserialize_with = "pixels")] Vec<[u8; 4]>);
        #[derive(Deserialize)]
        struct Objects(#[serde(deserialize_with = "objects")] Vec<CapabilityObject>);
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Content;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str("bounded native or opaque layer content") }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Content, A::Error> {
                let (mut kind, mut pixels, mut capability, mut objects) = (None, None, None, None);
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Kind => { if kind.is_some() { return Err(de::Error::duplicate_field("kind")); } kind = Some(map.next_value::<Kind>()?); }
                        Field::Pixels => { if pixels.is_some() { return Err(de::Error::duplicate_field("pixels")); } pixels = Some(map.next_value::<Pixels>()?.0); }
                        Field::Capability => { if capability.is_some() { return Err(de::Error::duplicate_field("capability")); } capability = Some(map.next_value::<CapabilityRef>()?); }
                        Field::Objects => { if objects.is_some() { return Err(de::Error::duplicate_field("objects")); } objects = Some(map.next_value::<Objects>()?.0); }
                    }
                }
                match kind.ok_or_else(|| de::Error::missing_field("kind"))? {
                    Kind::Raster if capability.is_none() && objects.is_none() => Ok(Content::Raster { pixels: pixels.ok_or_else(|| de::Error::missing_field("pixels"))? }),
                    Kind::Capability if pixels.is_none() => Ok(Content::Capability {
                        capability: capability.ok_or_else(|| de::Error::missing_field("capability"))?,
                        objects: objects.ok_or_else(|| de::Error::missing_field("objects"))? }),
                    _ => Err(de::Error::custom("content kind and fields disagree")),
                }
            }
        }
        d.deserialize_map(Fields)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentLayer { pub id: String, pub name: String, pub visible: bool, pub content: Content }
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32, pub id: String, pub width: u32, pub height: u32,
    #[serde(deserialize_with = "layers")]
    pub layers: Vec<DocumentLayer>,
}
impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document").field("version", &self.version).field("canvas", &(self.width,self.height))
            .field("layers", &self.layers.len()).finish_non_exhaustive()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelEdit { pub x: u32, pub y: u32, pub color: [u8; 4] }
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    RenameLayer { layer_id: String, name: String },
    SetVisibility { layer_id: String, visible: bool },
    MoveLayer { layer_id: String, before: Option<String> },
    RasterPixels { layer_id: String, #[serde(deserialize_with = "edits")] edits: Vec<PixelEdit> },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub version: u32, pub base_revision: String,
    #[serde(deserialize_with = "operations")]
    pub operations: Vec<Operation>,
}
impl Proposal {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_PROPOSAL_BYTES { return Err(fail("proposal exceeds byte limit")); }
        serde_json::from_slice(bytes).map_err(|_| fail("invalid document proposal JSON or schema"))
    }
    #[cfg(target_os = "linux")]
    pub fn capture(root: &crate::worker_io::PrivateDirectory, relative: &str) -> Result<Self, String> {
        Self::from_bytes(&root.read_private(relative, MAX_PROPOSAL_BYTES)?)
    }
}

#[derive(Debug, Serialize)]
pub struct LayerReport {
    pub id: String, pub kind: &'static str, pub visible: bool, pub objects: usize,
    pub payload_bytes: usize, pub capability: Option<CapabilityRef>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32, pub id: String, pub revision: String, pub width: u32, pub height: u32,
    pub layers: Vec<LayerReport>, pub unavailable_capabilities: Vec<CapabilityRef>,
    pub rendering: &'static str, pub opaque_payloads_validated: bool,
    pub code_loaded: bool, pub registry_consulted: bool, pub autonomous_execution: bool,
}
impl Document {
    /// Deterministic import identities, scoped to a document, not global UUIDs or
    /// authority. Copies retain IDs; subsequent edits never regenerate them.
    pub fn from_project(project: &Project) -> Result<Self, String> {
        project.validate().map_err(|_| fail("invalid source raster project"))?;
        let seed = project.revision().map_err(|_| fail("cannot fingerprint source raster"))?;
        let identity = |domain: &str| format!("{:x}", Sha256::digest(format!("perveance-v2:{domain}:{seed}").as_bytes()))[..32].to_owned();
        let layers = project.layers.iter().enumerate().map(|(i, layer)| DocumentLayer {
            id: identity(&format!("layer:{i}")), name: layer.name.clone(), visible: layer.visible,
            content: Content::Raster { pixels: layer.pixels.clone() },
        }).collect();
        let document = Self { version: VERSION, id: identity("document"), width: project.width, height: project.height, layers };
        document.validate()?; Ok(document)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION || !valid_id(&self.id) || self.width == 0 || self.height == 0
            || self.width > crate::MAX_DIMENSION || self.height > crate::MAX_DIMENSION
            || self.layers.is_empty() || self.layers.len() > MAX_LAYERS { return Err(fail("invalid document version, identity or canvas")); }
        let mut ids = BTreeSet::new(); ids.insert(self.id.as_str());
        let mut count = 0usize; let mut total = 0usize;
        for layer in &self.layers {
            if !valid_id(&layer.id) || !ids.insert(layer.id.as_str()) || layer.name.len() > 256 {
                return Err(fail("invalid or duplicate layer identity or metadata"));
            }
            match &layer.content {
                Content::Raster { pixels } => if pixels.len() != (self.width*self.height) as usize { return Err(fail("invalid native raster pixel count")); },
                Content::Capability { capability, objects } => {
                    capability.validate()?;
                    count = count.checked_add(objects.len()).ok_or_else(|| fail("object count overflow"))?;
                    if count > MAX_OBJECTS { return Err(fail("document object limit exceeded")); }
                    for object in objects {
                        if !valid_id(&object.id) || !ids.insert(object.id.as_str()) { return Err(fail("invalid or duplicate object identity")); }
                        object.payload.validate()?;
                        total = total.checked_add(object.payload.data.len()/2).ok_or_else(|| fail("payload size overflow"))?;
                        if total > MAX_TOTAL_PAYLOAD_BYTES { return Err(fail("aggregate opaque payload limit exceeded")); }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES { return Err(fail("document exceeds byte limit")); }
        let document: Self = serde_json::from_slice(bytes).map_err(|_| fail("invalid document JSON or schema"))?;
        document.validate()?; Ok(document)
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| fail("cannot serialize document"))?;
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES { return Err(fail("document exceeds byte limit")); }
        Ok(bytes)
    }
    pub fn revision(&self) -> Result<String, String> {
        Ok(format!("{REVISION_PREFIX}{:x}", Sha256::digest(self.canonical_bytes()?)))
    }
    pub fn inspect(&self) -> Result<Report, String> {
        self.validate()?;
        let mut unavailable = BTreeSet::new();
        let layers = self.layers.iter().map(|layer| {
            let (kind, objects, payload_bytes, capability) = match &layer.content {
                Content::Raster { .. } => ("raster", 0, 0, None),
                Content::Capability { capability, objects } => {
                    unavailable.insert(capability.clone());
                    ("capability", objects.len(), objects.iter().map(|o| o.payload.data.len()/2).sum(), Some(capability.clone()))
                }
            };
            LayerReport { id: layer.id.clone(), kind, visible: layer.visible, objects, payload_bytes, capability }
        }).collect();
        Ok(Report { version: VERSION, id: self.id.clone(), revision: self.revision()?, width: self.width, height: self.height,
            layers, rendering: if unavailable.is_empty() { "native_raster" } else { "blocked_unavailable_capabilities" },
            unavailable_capabilities: unavailable.into_iter().collect(), opaque_payloads_validated: false,
            code_loaded: false, registry_consulted: false, autonomous_execution: false })
    }
    /// No preview-as-success, partial render, fallback rasterization or dropping
    /// hidden unavailable layers. This is the only renderer shipped for v2.
    pub fn render_raster(&self) -> Result<Project, String> {
        self.validate()?;
        let mut layers = Vec::new();
        for layer in &self.layers {
            let Content::Raster { pixels } = &layer.content else {
                return Err(fail("rendering blocked: unavailable capability content must not be omitted"));
            };
            layers.push(Layer { name: layer.name.clone(), visible: layer.visible, pixels: pixels.clone() });
        }
        let project = Project { version: crate::FORMAT_VERSION, width: self.width, height: self.height, layers };
        project.validate().map_err(|_| fail("invalid native raster rendering input"))?; Ok(project)
    }
    /// Wrapper/raster edits are transactional and stable-ID addressed. Opaque
    /// object mutation, migrations and extension render execution are absent;
    /// they require a future independently verified capability contract.
    pub fn apply(&mut self, proposal: &Proposal) -> Result<(), String> {
        self.validate()?;
        if proposal.version != 1 || proposal.operations.is_empty() || proposal.operations.len() > MAX_EDIT_UNITS {
            return Err(fail("invalid document edit version or operation count"));
        }
        let hash = proposal.base_revision.strip_prefix(REVISION_PREFIX).ok_or_else(|| fail("invalid document edit revision"))?;
        if !hex(hash, 64) || proposal.base_revision != self.revision()? { return Err(fail("stale or invalid document edit revision")); }
        let mut units = 0usize;
        for operation in &proposal.operations {
            let count = match operation { Operation::RasterPixels { edits, .. } => {
                if edits.is_empty() { return Err(fail("empty native pixel edit")); } edits.len()
            }, _ => 1 };
            units = units.checked_add(count).ok_or_else(|| fail("document edit size overflow"))?;
            if units > MAX_EDIT_UNITS { return Err(fail("document edit unit limit exceeded")); }
        }
        let mut candidate = self.clone();
        for operation in &proposal.operations {
            let target = match operation { Operation::RenameLayer { layer_id, .. } | Operation::SetVisibility { layer_id, .. }
                | Operation::MoveLayer { layer_id, .. } | Operation::RasterPixels { layer_id, .. } => layer_id };
            if !valid_id(target) { return Err(fail("invalid target layer ID")); }
            let index = candidate.layers.iter().position(|layer| layer.id == *target).ok_or_else(|| fail("target layer not found"))?;
            match operation {
                Operation::RenameLayer { name, .. } => {
                    if name.trim().is_empty() || name.len() > 256 { return Err(fail("invalid layer name")); }
                    candidate.layers[index].name = name.clone();
                }
                Operation::SetVisibility { visible, .. } => candidate.layers[index].visible = *visible,
                Operation::MoveLayer { before, .. } => {
                    if before.as_ref() == Some(target) { continue; }
                    if let Some(id) = before {
                        if !valid_id(id) || !candidate.layers.iter().any(|layer| &layer.id == id) { return Err(fail("move destination not found")); }
                    }
                    let layer = candidate.layers.remove(index);
                    let to = before.as_ref().and_then(|id| candidate.layers.iter().position(|layer| &layer.id == id)).unwrap_or(candidate.layers.len());
                    candidate.layers.insert(to, layer);
                }
                Operation::RasterPixels { edits, .. } => {
                    let layer = &candidate.layers[index];
                    let Content::Raster { pixels } = &layer.content else { return Err(fail("opaque capability objects are not editable by the native raster engine")); };
                    let mut raster = Project { version: crate::FORMAT_VERSION, width: candidate.width, height: candidate.height,
                        layers: vec![Layer { name: layer.name.clone(), visible: layer.visible, pixels: pixels.clone() }] };
                    for edit in edits { raster.set_pixel(0, edit.x, edit.y, edit.color).map_err(|_| fail("invalid native pixel edit"))?; }
                    candidate.layers[index].content = Content::Raster { pixels: raster.layers.remove(0).pixels };
                }
            }
        }
        candidate.validate()?; *self = candidate; Ok(())
    }
    #[cfg(target_os = "linux")]
    pub fn capture(root: &crate::worker_io::PrivateDirectory, relative: &str) -> Result<Self, String> {
        Self::from_bytes(&root.read_private(relative, MAX_DOCUMENT_BYTES)?)
    }
    #[cfg(target_os = "linux")]
    pub fn publish(&self, root: &crate::worker_io::PrivateDirectory, name: &str) -> Result<crate::worker_io::FilePublication, String> {
        root.publish_bytes(name, &self.canonical_bytes()?, MAX_DOCUMENT_BYTES)
    }
    #[cfg(target_os = "linux")]
    pub fn export_png(&self, root: &crate::worker_io::PrivateDirectory, name: &str) -> Result<crate::worker_io::FilePublication, String> {
        // Rendering must succeed before any staging/output file is created.
        let bytes = self.render_raster()?.png_bytes(1).map_err(|_| fail("native raster export failed"))?;
        root.publish_bytes(name, &bytes, crate::MAX_PNG_BYTES)
    }
}
