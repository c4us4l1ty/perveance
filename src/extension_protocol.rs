//! Data-only prospective ABI-1 call/reply contract. No WASM invocation or permit.
//! Wire acceptance is NOT semantic/behavior verification, editing or rendering.
use crate::{document::{self, CapabilityObject, CapabilityRef, Content, Document, OpaquePayload}, registry::Manifest};
use serde::{de::{self, SeqAccess, Visitor}, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt, ops::Range};

pub const VERSION: u32 = 1;
pub const MAX_INTENT_BYTES: u64 = 64 * 1024;
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REPLY_BYTES: u64 = crate::registry::MAX_OUTPUT_BYTES;
pub const MAX_NEW_IDS: usize = 32;
pub const MAX_CHANGES: usize = 128;
pub const REQUEST_MAGIC: &[u8;8] = b"PVEXRQ01";
pub const REPLY_MAGIC: &[u8;8] = b"PVEXRP01";
pub const REPLY_HEADER_BYTES: usize = 48;
const REQUEST_DOMAIN: &[u8] = b"perveance-extension-request-v1\0";
const LAYER_DOMAIN: &[u8] = b"perveance-extension-layer-v1\0";

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum Error {
    InvalidIntent, UnsupportedContract, InvalidSnapshot, StaleRevision, ReferenceMismatch,
    OperationNotDeclared, IdPool, StaticProfileRequired, FrameLimit, MalformedFrame,
    RequestMismatch, WrongKind, UnknownStatus, PayloadShape, ObjectPrecondition,
    ObjectScope, DuplicateTarget, DocumentLimit, RenderShape, MemoryWindow,
}
impl fmt::Display for Error {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {write!(f,"extension protocol: {:?}",self)}
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T,Error>;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind { ValidatePayload, ApplyEdits, RenderLayer }
impl Kind {
    fn byte(self)->u8 {match self {Self::ValidatePayload=>1,Self::ApplyEdits=>2,Self::RenderLayer=>3}}
    fn read(value:u8)->Result<Self>{match value{1=>Ok(Self::ValidatePayload),2=>Ok(Self::ApplyEdits),3=>Ok(Self::RenderLayer),_=>Err(Error::WrongKind)}}
}
fn new_ids<'de,D:Deserializer<'de>>(d:D)->std::result::Result<Vec<String>,D::Error>{
    struct Ids;
    impl<'de> Visitor<'de> for Ids {
        type Value=Vec<String>;
        fn expecting(&self,f:&mut fmt::Formatter)->fmt::Result {f.write_str("at most 32 stable identifiers")}
        fn visit_seq<A:SeqAccess<'de>>(self,mut seq:A)->std::result::Result<Self::Value,A::Error>{
            let mut ids=Vec::new();while let Some(id)=seq.next_element::<String>()?{if ids.len()==MAX_NEW_IDS{return Err(de::Error::custom("identifier pool exceeds limit"));}ids.push(id);}Ok(ids)
        }
    }
    d.deserialize_seq(Ids)
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditIntent {
    pub operation: String, pub schema_version:u32, pub arguments:OpaquePayload,
    #[serde(deserialize_with="new_ids")]
    pub new_object_ids:Vec<String>,
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub version:u32, pub call_id:String, pub base_revision:String, pub layer_id:String,
    pub kind:Kind, pub edit:Option<EditIntent>,
}
impl Intent {
    pub fn from_bytes(bytes:&[u8])->Result<Self>{
        if bytes.len() as u64>MAX_INTENT_BYTES{return Err(Error::FrameLimit);}
        serde_json::from_slice(bytes).map_err(|_|Error::InvalidIntent)
    }
    #[cfg(target_os="linux")]
    pub fn capture(root:&crate::worker_io::PrivateDirectory,name:&str)->std::result::Result<Self,String>{
        Self::from_bytes(&root.read_private(name,MAX_INTENT_BYTES)?).map_err(|e|e.to_string())
    }
}
#[derive(Clone,Default,Debug,Serialize)]
pub struct Gates {
    pub module_instantiated:bool,pub code_executed:bool,pub behavior_verified:bool,
    pub payload_schemas_verified:bool,pub provenance_verified:bool,pub capability_verified:bool,
    pub runtime_limits_enforced:bool,pub activation_permitted:bool,pub installed:bool,
    pub active:bool,pub available:bool,pub document_modified:bool,pub registry_modified:bool,
    pub png_published:bool,pub provider_network_used:bool,pub autonomous_execution:bool,
    pub authenticated:bool,pub replay_protection_enforced:bool,
}
#[derive(Clone,Debug,Serialize)]
pub struct CallReport {
    pub version:u32,pub diagnostic_only:bool,pub kind:Kind,pub call_id:String,pub context_scope:String,
    pub reference:CapabilityRef,pub manifest_sha256:String,pub artifact_sha256:String,
    pub document_id:String,pub document_revision:String,pub layer_id:String,
    pub request_sha256:String,pub request_bytes:usize,pub input_objects:usize,
    pub input_payload_bytes:usize,pub offered_new_ids:usize,pub gates:Gates,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum CandidateStatus { Success, InvalidPayload, UnsupportedOperation, ResourceDeclined, Failed }
impl CandidateStatus {
    fn read(value:u8)->Result<Self>{match value{0=>Ok(Self::Success),1=>Ok(Self::InvalidPayload),2=>Ok(Self::UnsupportedOperation),3=>Ok(Self::ResourceDeclined),4=>Ok(Self::Failed),_=>Err(Error::UnknownStatus)}}
}
#[derive(Debug,Serialize)]
pub struct ReplyReport {
    pub version:u32,pub diagnostic_only:bool,pub request_sha256:String,pub wire_accepted:bool,
    pub memory_window_checked:bool,pub issue:Option<Error>,pub candidate_status:Option<CandidateStatus>,
    pub proposed_changes:usize,pub projected_objects:Option<usize>,pub projected_payload_bytes:Option<usize>,
    pub projected_layer_sha256:Option<String>,pub rgba_sha256:Option<String>,pub rgba_bytes:Option<usize>,
    pub gates:Gates,
}
fn digest(bytes:&[u8])->[u8;32]{Sha256::digest(bytes).into()}
fn hex(bytes:&[u8])->String{bytes.iter().map(|b|format!("{b:02x}")).collect()}
fn unhex<const N:usize>(value:&str)->Result<[u8;N]>{
    if value.len()!=2*N{return Err(Error::InvalidIntent);}
    let mut out=[0;N];
    fn nibble(b:u8)->Result<u8>{match b{b'0'..=b'9'=>Ok(b-b'0'),b'a'..=b'f'=>Ok(b-b'a'+10),_=>Err(Error::InvalidIntent)}}
    for (i,p) in value.as_bytes().chunks_exact(2).enumerate(){out[i]=nibble(p[0])?*16+nibble(p[1])?;}Ok(out)
}
fn put_u32(out:&mut Vec<u8>,n:usize)->Result<()>{out.extend(u32::try_from(n).map_err(|_|Error::FrameLimit)?.to_le_bytes());Ok(())}
fn blob(out:&mut Vec<u8>,bytes:&[u8])->Result<()>{put_u32(out,bytes.len())?;out.extend(bytes);Ok(())}
fn encode_objects(out:&mut Vec<u8>,objects:&[Object])->Result<()>{
    put_u32(out,objects.len())?;for object in objects{out.extend(object.id);blob(out,&object.payload)?;}Ok(())
}
struct Reader<'a>{bytes:&'a[u8],at:usize}
impl<'a> Reader<'a>{
    fn new(bytes:&'a[u8])->Self{Self{bytes,at:0}}
    fn take(&mut self,n:usize)->Result<&'a[u8]>{let end=self.at.checked_add(n).filter(|n|*n<=self.bytes.len()).ok_or(Error::MalformedFrame)?;let part=&self.bytes[self.at..end];self.at=end;Ok(part)}
    fn array<const N:usize>(&mut self)->Result<[u8;N]>{self.take(N)?.try_into().map_err(|_|Error::MalformedFrame)}
    fn byte(&mut self)->Result<u8>{Ok(self.array::<1>()?[0])}
    fn uint(&mut self)->Result<usize>{Ok(u32::from_le_bytes(self.array()?) as usize)}
    fn blob(&mut self,max:usize)->Result<&'a[u8]>{let n=self.uint()?;if n>max{return Err(Error::FrameLimit);}self.take(n)}
    fn zero(&mut self,n:usize)->Result<()>{if self.take(n)?.iter().any(|b|*b!=0){return Err(Error::MalformedFrame);}Ok(())}
    fn end(&self)->Result<()>{if self.at==self.bytes.len(){Ok(())}else{Err(Error::MalformedFrame)}}
}
#[derive(Clone)]
struct Object{id:[u8;16],payload:Vec<u8>}
fn object(object:&CapabilityObject)->Result<Object>{Ok(Object{id:unhex(&object.id)?,payload:object.payload.bytes().map_err(|_|Error::InvalidSnapshot)?})}
/// Borrowed decode for future SDKs/fixtures. It confers no host snapshot authority.
pub struct WireObject<'a>{pub id:[u8;16],pub payload:&'a[u8]}
pub struct WireEdit<'a>{pub operation:&'a str,pub schema_version:u32,pub arguments:&'a[u8],pub new_object_ids:Vec<[u8;16]>}
pub struct RequestView<'a>{
    pub kind:Kind,pub call_id:[u8;16],pub context_scope:[u8;16],pub manifest_sha256:[u8;32],pub artifact_sha256:[u8;32],
    pub document_revision:[u8;32],pub document_id:[u8;16],pub layer_id:[u8;16],
    pub width:u32,pub height:u32,pub payload_schema:u32,pub objects:Vec<WireObject<'a>>,pub edit:Option<WireEdit<'a>>,
}
pub fn decode_request(bytes:&[u8])->Result<RequestView<'_>>{
    if bytes.len()>MAX_REQUEST_BYTES{return Err(Error::FrameLimit);}
    let mut r=Reader::new(bytes);if r.take(8)?!=REQUEST_MAGIC{return Err(Error::MalformedFrame);}
    let kind=Kind::read(r.byte()?)?;r.zero(3)?;
    let call_id=r.array()?;let context_scope=r.array()?;let manifest_sha256=r.array()?;let artifact_sha256=r.array()?;let document_revision=r.array()?;let document_id=r.array()?;let layer_id=r.array()?;
    let width=r.uint()? as u32;let height=r.uint()? as u32;let payload_schema=r.uint()? as u32;
    if width==0||height==0||width>crate::MAX_DIMENSION||height>crate::MAX_DIMENSION||payload_schema==0{return Err(Error::PayloadShape);}
    let mut ids=BTreeSet::from([document_id,layer_id]);if ids.len()!=2{return Err(Error::ObjectScope);}
    let count=r.uint()?;if count>document::MAX_OBJECTS{return Err(Error::FrameLimit);}
    let mut objects=Vec::new();let mut total=0usize;
    for _ in 0..count{let id=r.array()?;if !ids.insert(id){return Err(Error::ObjectScope);}let payload=r.blob(document::MAX_PAYLOAD_BYTES)?;total+=payload.len();if total>document::MAX_TOTAL_PAYLOAD_BYTES{return Err(Error::DocumentLimit);}objects.push(WireObject{id,payload});}
    let edit=if kind==Kind::ApplyEdits{
        let operation=std::str::from_utf8(r.blob(64)?).map_err(|_|Error::PayloadShape)?;
        if operation.is_empty()||!(operation.as_bytes()[0].is_ascii_alphabetic()||operation.as_bytes()[0]==b'_')||!operation.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){return Err(Error::PayloadShape);}
        let schema_version=r.uint()? as u32;if schema_version==0{return Err(Error::PayloadShape);}
        let arguments=r.blob(document::MAX_PAYLOAD_BYTES)?;let count=r.uint()?;if count>MAX_NEW_IDS{return Err(Error::FrameLimit);}
        let mut new_object_ids=Vec::new();for _ in 0..count{let id=r.array()?;if !ids.insert(id){return Err(Error::IdPool);}new_object_ids.push(id);}Some(WireEdit{operation,schema_version,arguments,new_object_ids})
    }else{None};
    r.end()?;Ok(RequestView{kind,call_id,context_scope,manifest_sha256,artifact_sha256,document_revision,document_id,layer_id,width,height,payload_schema,objects,edit})
}

/// Immutable scoped host snapshot. No method writes a Document or yields a permit.
pub struct PreparedCall {
    report:CallReport,request:Vec<u8>,request_hash:[u8;32],objects:Vec<Object>,new_ids:BTreeSet<[u8;16]>,
    foreign_objects:usize,foreign_payload_bytes:usize,width:u32,height:u32,output_limit:usize,memory_limit:usize,
}
impl fmt::Debug for PreparedCall {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("PreparedCall").field("request_sha256",&self.report.request_sha256).finish_non_exhaustive()}}
impl PreparedCall {
    pub(crate) fn prepare(manifest:&Manifest,manifest_bytes:&[u8],doc:&Document,intent:&Intent,scope:&str)->Result<Self>{
        manifest.validate().map_err(|_|Error::UnsupportedContract)?;
        if manifest.reference.abi_version!=1||manifest.document_version!=2||doc.version!=2{return Err(Error::UnsupportedContract);}
        if manifest.runtime.output_bytes<REPLY_HEADER_BYTES as u64{return Err(Error::FrameLimit);}
        doc.validate().map_err(|_|Error::InvalidSnapshot)?;
        if intent.version!=VERSION||unhex::<16>(&intent.call_id).is_err()||unhex::<16>(&intent.layer_id).is_err(){return Err(Error::InvalidIntent);}
        let revision=doc.revision().map_err(|_|Error::InvalidSnapshot)?;if intent.base_revision!=revision{return Err(Error::StaleRevision);}
        let layer=doc.layers.iter().find(|l|l.id==intent.layer_id).ok_or(Error::ObjectScope)?;
        let Content::Capability{capability,objects:source}= &layer.content else{return Err(Error::ObjectScope);};
        if capability!=&manifest.reference{return Err(Error::ReferenceMismatch);}
        let mut all_ids=BTreeSet::new();all_ids.insert(unhex::<16>(&doc.id)?);let mut total_objects=0;let mut total_bytes=0;
        for layer in &doc.layers{all_ids.insert(unhex::<16>(&layer.id)?);if let Content::Capability{objects,..}=&layer.content{total_objects+=objects.len();for o in objects{all_ids.insert(unhex::<16>(&o.id)?);total_bytes+=o.payload.data.len()/2;}}}
        let objects:Vec<_>=source.iter().map(object).collect::<Result<_>>()?;
        let input_bytes=objects.iter().map(|o|o.payload.len()).sum::<usize>();let mut pool=BTreeSet::new();
        match (intent.kind,&intent.edit){
            (Kind::ApplyEdits,Some(edit))=>{
                if !manifest.operations.iter().any(|op|op.id==edit.operation&&op.schema_version==edit.schema_version){return Err(Error::OperationNotDeclared);}
                edit.arguments.validate().map_err(|_|Error::InvalidIntent)?;
                if edit.new_object_ids.len()>MAX_NEW_IDS{return Err(Error::IdPool);}
                for id in &edit.new_object_ids{let id=unhex::<16>(id)?;if all_ids.contains(&id)||!pool.insert(id){return Err(Error::IdPool);}}
            }
            (Kind::ValidatePayload|Kind::RenderLayer,None)=>{},_=>return Err(Error::InvalidIntent),
        }
        let manifest_hash=digest(manifest_bytes);let mut request=REQUEST_MAGIC.to_vec();request.extend([intent.kind.byte(),0,0,0]);request.extend(unhex::<16>(&intent.call_id)?);request.extend(unhex::<16>(scope)?);
        request.extend(manifest_hash);request.extend(unhex::<32>(&manifest.reference.artifact_sha256)?);request.extend(unhex::<32>(revision.strip_prefix("sha256-document-v2:").ok_or(Error::InvalidSnapshot)?)?);
        request.extend(unhex::<16>(&doc.id)?);request.extend(unhex::<16>(&layer.id)?);request.extend(doc.width.to_le_bytes());request.extend(doc.height.to_le_bytes());request.extend(capability.payload_schema.to_le_bytes());encode_objects(&mut request,&objects)?;
        if let Some(edit)=&intent.edit{blob(&mut request,edit.operation.as_bytes())?;request.extend(edit.schema_version.to_le_bytes());blob(&mut request,&edit.arguments.bytes().map_err(|_|Error::InvalidIntent)?)?;put_u32(&mut request,edit.new_object_ids.len())?;for id in &edit.new_object_ids{request.extend(unhex::<16>(id)?);}}
        if request.len()>MAX_REQUEST_BYTES||request.len()>=manifest.runtime.memory_bytes as usize{return Err(Error::FrameLimit);}
        // Round-trip our own canonical frame through the same strict byte codec.
        decode_request(&request)?;
        let mut hash=Sha256::new();hash.update(REQUEST_DOMAIN);hash.update(&request);let request_hash:[u8;32]=hash.finalize().into();
        let report=CallReport{version:VERSION,diagnostic_only:true,kind:intent.kind,call_id:intent.call_id.clone(),context_scope:scope.into(),reference:manifest.reference.clone(),manifest_sha256:hex(&manifest_hash),artifact_sha256:manifest.reference.artifact_sha256.clone(),
            document_id:doc.id.clone(),document_revision:revision,layer_id:layer.id.clone(),request_sha256:hex(&request_hash),request_bytes:request.len(),input_objects:objects.len(),input_payload_bytes:input_bytes,offered_new_ids:pool.len(),gates:Gates::default()};
        Ok(Self{report,request,request_hash,foreign_objects:total_objects-objects.len(),foreign_payload_bytes:total_bytes-input_bytes,objects,new_ids:pool,width:doc.width,height:doc.height,output_limit:manifest.runtime.output_bytes as usize,memory_limit:manifest.runtime.memory_bytes as usize})
    }
    pub fn report(&self)->CallReport{self.report.clone()}
    /// Contains selected-layer opaque data. Never log or give to a provider.
    pub fn request_bytes(&self)->&[u8]{&self.request}
    pub fn request_window(&self,memory_bytes:usize,pointer:u32)->Result<Range<usize>>{
        self.window(memory_bytes,pointer,self.request.len())
    }
    fn window(&self,memory_bytes:usize,pointer:u32,length:usize)->Result<Range<usize>>{
        if pointer==0||length==0||memory_bytes==0||memory_bytes%65536!=0||memory_bytes>self.memory_limit{return Err(Error::MemoryWindow);}
        let start=pointer as usize;let end=start.checked_add(length).filter(|end|*end<=memory_bytes).ok_or(Error::MemoryWindow)?;Ok(start..end)
    }
    /// High 32 result bits are pointer, low 32 are length, as unsigned bit fields.
    /// The memory must be a stable snapshot/quiescent future instance, not live
    /// shared memory. This only checks a supplied slice; there is no runtime.
    pub fn inspect_memory_reply(&self,memory:&[u8],packed:i64)->ReplyReport{
        let bits=packed as u64;let pointer=(bits>>32)as u32;let length=(bits&0xffff_ffff)as usize;
        let range=if length>self.output_limit||length as u64>MAX_REPLY_BYTES{Err(Error::FrameLimit)}else{self.window(memory.len(),pointer,length)};
        match range{Ok(range)=>{let mut report=self.inspect_reply(&memory[range]);report.memory_window_checked=true;report},Err(issue)=>self.rejected(issue)}
    }
    fn empty_reply(&self)->ReplyReport{ReplyReport{version:VERSION,diagnostic_only:true,request_sha256:self.report.request_sha256.clone(),wire_accepted:false,memory_window_checked:false,issue:None,candidate_status:None,proposed_changes:0,projected_objects:None,projected_payload_bytes:None,projected_layer_sha256:None,rgba_sha256:None,rgba_bytes:None,gates:Gates::default()}}
    fn rejected(&self,issue:Error)->ReplyReport{let mut r=self.empty_reply();r.issue=Some(issue);r}
    pub fn inspect_reply(&self,bytes:&[u8])->ReplyReport{
        let mut report=self.empty_reply();match self.check_reply(bytes,&mut report){Ok(())=>report.wire_accepted=true,Err(issue)=>{report=self.rejected(issue);}}report
    }
    fn check_reply(&self,bytes:&[u8],report:&mut ReplyReport)->Result<()>{
        if bytes.len()>self.output_limit||bytes.len()as u64>MAX_REPLY_BYTES{return Err(Error::FrameLimit);}
        let mut r=Reader::new(bytes);if r.take(8)?!=REPLY_MAGIC{return Err(Error::MalformedFrame);}
        if Kind::read(r.byte()?)?!=self.report.kind{return Err(Error::WrongKind);}
        let status=CandidateStatus::read(r.byte()?)?;r.zero(2)?;
        if r.array::<32>()?!=self.request_hash{return Err(Error::RequestMismatch);}
        let payload=r.blob(self.output_limit)?;r.end()?;
        report.candidate_status=Some(status);
        if status!=CandidateStatus::Success {if !payload.is_empty(){return Err(Error::PayloadShape);}return Ok(());}
        match self.report.kind{
            Kind::ValidatePayload=>if !payload.is_empty(){return Err(Error::PayloadShape);},
            Kind::ApplyEdits=>self.check_changes(payload,report)?,
            Kind::RenderLayer=>{
                let mut p=Reader::new(payload);if p.uint()?!=self.width as usize||p.uint()?!=self.height as usize||p.byte()?!=1{return Err(Error::RenderShape);}p.zero(3)?;
                let expected=self.width as usize*self.height as usize*4;let rgba=p.blob(expected)?;if rgba.len()!=expected{return Err(Error::RenderShape);}p.end()?;
                report.rgba_sha256=Some(hex(&digest(rgba)));report.rgba_bytes=Some(rgba.len());
            }
        }Ok(())
    }
    fn check_changes(&self,payload:&[u8],report:&mut ReplyReport)->Result<()>{
        let mut r=Reader::new(payload);let count=r.uint()?;if count>MAX_CHANGES{return Err(Error::FrameLimit);}
        let mut objects=self.objects.clone();let mut touched=BTreeSet::new();
        for _ in 0..count{
            let tag=r.byte()?;let id=r.array::<16>()?;if !touched.insert(id){return Err(Error::DuplicateTarget);}
            match tag{
                1=>{
                    if !self.new_ids.contains(&id)||objects.iter().any(|o|o.id==id){return Err(Error::ObjectScope);}
                    let before=match r.byte()?{0=>None,1=>Some(r.array::<16>()?),_=>return Err(Error::PayloadShape)};
                    let bytes=r.blob(document::MAX_PAYLOAD_BYTES)?;let index=match before{None=>objects.len(),Some(anchor)=>objects.iter().position(|o|o.id==anchor).ok_or(Error::ObjectScope)?};
                    objects.insert(index,Object{id,payload:bytes.to_vec()});
                }
                2|3=>{
                    let index=objects.iter().position(|o|o.id==id).ok_or(Error::ObjectScope)?;
                    // Newly inserted IDs cannot be mutated again in the same
                    // proposal; touched also forbids all duplicate targets.
                    if r.array::<32>()?!=digest(&objects[index].payload){return Err(Error::ObjectPrecondition);}
                    if tag==2{objects[index].payload=r.blob(document::MAX_PAYLOAD_BYTES)?.to_vec();}else{objects.remove(index);}
                }
                _=>return Err(Error::PayloadShape),
            }
            self.projection_limits(&objects)?;
        }
        r.end()?;let bytes=self.projection_limits(&objects)?;
        let mut encoded=LAYER_DOMAIN.to_vec();encode_objects(&mut encoded,&objects)?;
        report.proposed_changes=count;report.projected_objects=Some(objects.len());report.projected_payload_bytes=Some(bytes);report.projected_layer_sha256=Some(hex(&digest(&encoded)));Ok(())
    }
    fn projection_limits(&self,objects:&[Object])->Result<usize>{
        let bytes=objects.iter().map(|o|o.payload.len()).sum::<usize>();
        if objects.len()+self.foreign_objects>document::MAX_OBJECTS||bytes+self.foreign_payload_bytes>document::MAX_TOTAL_PAYLOAD_BYTES{return Err(Error::DocumentLimit);}Ok(bytes)
    }
}
