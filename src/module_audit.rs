//! Static byte validation and a narrow prospective memory-call ABI profile.
//! No instantiation, compilation, interpreter, JIT, host import, call or permit.
//! A pass says NOTHING about behavior, payload semantics, termination or safety.
use crate::{document::CapabilityRef, registry::Manifest};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wasmparser::{DataKind, Encoding, ExternalKind, FuncType, MemoryType, Operator, Parser, Payload, ValType, Validator, WasmFeatures};

pub const VERSION: u32 = 1;
pub const DOCUMENT_VERSION: u32 = 2;
pub const MAX_PARSER_EVENTS: u32 = 2048;
pub const MAX_INITIALIZER_BYTES: usize = 32;
pub const ALLOC_EXPORT: &str = "perveance_alloc_v1";
pub const MEMORY_EXPORT: &str = "memory";
pub const MAX_TYPES: u32 = 128;
pub const MAX_FUNCTIONS: u32 = 1024;
pub const MAX_GLOBALS: u32 = 64;
pub const MAX_EXPORTS: u32 = 5;
pub const MAX_PARAMS: usize = 16;
pub const MAX_RESULTS: usize = 4;
pub const MAX_LOCALS_PER_FUNCTION: u32 = 1024;
pub const MAX_TOTAL_LOCALS: u32 = 16 * 1024;
pub const MAX_BODY_BYTES: usize = 256 * 1024;
pub const MAX_INSTRUCTIONS: u32 = 200_000;
pub const MAX_CONTROL_DEPTH: u32 = 128;
pub const MAX_BRANCH_TARGETS: u32 = 4096;
pub const MAX_DATA_SEGMENTS: u32 = 32;
pub const MAX_DATA_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CUSTOM_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum Rejection {
    UnsupportedManifestContract, InvalidWasm, InvalidOrUnsupportedWasm, ResourceLimit,
    ImportsForbidden, StartForbidden, SectionForbidden, TypeForbidden,
    MemoryProfile, DataInitializer, AbiExports, AbiSignatures,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum Validation { NotRun, Passed, Failed }
#[derive(Clone, Default, Debug, Serialize)]
pub struct Counts {
    pub parser_events: u32, pub types: u32, pub functions: u32, pub code_bodies: u32, pub globals: u32,
    pub exports: u32, pub local_values: u32, pub instructions: u32,
    pub data_segments: u32, pub data_bytes: usize, pub custom_bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32, pub policy: &'static str, pub inspector: &'static str,
    pub reference: CapabilityRef, pub manifest_sha256: String, pub artifact_sha256: String,
    pub artifact_bytes: usize, pub static_profile_passed: bool,
    pub structural_validation: Validation, pub abi_signatures_match: bool,
    pub counts_complete: bool, pub counts: Counts, pub rejection: Option<Rejection>,
    pub module_instantiated: bool, pub code_executed: bool, pub runtime_limits_enforced: bool,
    pub behavior_verified: bool, pub payload_schemas_verified: bool, pub provenance_verified: bool,
    pub capability_verified: bool, pub activation_permitted: bool, pub registry_modified: bool,
    pub installed: bool, pub active: bool, pub available: bool, pub autonomous_execution: bool,
}
fn hash(bytes: &[u8]) -> String { format!("{:x}",Sha256::digest(bytes)) }
fn numeric(ty: ValType) -> bool { matches!(ty,ValType::I32|ValType::I64|ValType::F32|ValType::F64) }
fn add(value: &mut u32, amount: u32, maximum: u32) -> Result<(),Rejection> {
    *value=value.checked_add(amount).filter(|n|*n<=maximum).ok_or(Rejection::ResourceLimit)?; Ok(())
}
struct Shape<'a> {
    types: Vec<FuncType>, functions: Vec<u32>, exports: BTreeMap<&'a str,(ExternalKind,u32)>, memory: Option<MemoryType>,
}
fn scan<'a>(artifact: &'a [u8], manifest: &Manifest, counts: &mut Counts) -> Result<Shape<'a>,Rejection> {
    let mut shape=Shape { types:Vec::new(),functions:Vec::new(),exports:BTreeMap::new(),memory:None };
    for payload in Parser::new(0).parse_all(artifact) {
        // Includes the version/end/code-entry events, not just section headers.
        add(&mut counts.parser_events,1,MAX_PARSER_EVENTS)?;
        match payload.map_err(|_|Rejection::InvalidWasm)? {
            Payload::Version { num, encoding, .. } if num==1 && encoding==Encoding::Module => {},
            Payload::TypeSection(reader) => {
                if reader.count()>MAX_TYPES {return Err(Rejection::ResourceLimit);}
                for ty in reader.into_iter_err_on_gc_types() {
                    let ty=ty.map_err(|_|Rejection::TypeForbidden)?;
                    add(&mut counts.types,1,MAX_TYPES)?;
                    if ty.params().len()>MAX_PARAMS || ty.results().len()>MAX_RESULTS {return Err(Rejection::ResourceLimit);}
                    if ty.params().iter().chain(ty.results()).any(|ty|!numeric(*ty)) {return Err(Rejection::TypeForbidden);}
                    shape.types.push(ty);
                }
            }
            Payload::ImportSection(reader) => if reader.count()!=0 {return Err(Rejection::ImportsForbidden);},
            Payload::FunctionSection(reader) => {
                add(&mut counts.functions,reader.count(),MAX_FUNCTIONS)?;
                for index in reader {shape.functions.push(index.map_err(|_|Rejection::InvalidWasm)?);}
            }
            Payload::MemorySection(reader) => {
                if reader.count()!=1 || shape.memory.is_some() {return Err(Rejection::MemoryProfile);}
                for memory in reader {
                    let memory=memory.map_err(|_|Rejection::InvalidWasm)?;
                    if memory.memory64 || memory.shared || memory.page_size_log2.is_some() || memory.initial==0
                        || memory.maximum.is_none_or(|max| max<memory.initial || max>manifest.runtime.memory_bytes/65536) {
                        return Err(Rejection::MemoryProfile);
                    }
                    shape.memory=Some(memory);
                }
            }
            Payload::GlobalSection(reader) => {
                add(&mut counts.globals,reader.count(),MAX_GLOBALS)?;
                for global in reader {
                    let global=global.map_err(|_|Rejection::InvalidWasm)?;
                    if global.ty.shared || !numeric(global.ty.content_type) {return Err(Rejection::TypeForbidden);}
                    if global.init_expr.get_binary_reader().bytes_remaining()>MAX_INITIALIZER_BYTES {return Err(Rejection::ResourceLimit);}
                }
            }
            Payload::ExportSection(reader) => {
                add(&mut counts.exports,reader.count(),MAX_EXPORTS)?;
                for export in reader {
                    let export=export.map_err(|_|Rejection::InvalidWasm)?;
                    if export.name.len()>64 || shape.exports.insert(export.name,(export.kind,export.index)).is_some() {return Err(Rejection::AbiExports);}
                }
            }
            Payload::StartSection {..} => return Err(Rejection::StartForbidden),
            Payload::TableSection(reader) => if reader.count()!=0 {return Err(Rejection::SectionForbidden);},
            Payload::ElementSection(reader) => if reader.count()!=0 {return Err(Rejection::SectionForbidden);},
            Payload::DataCountSection {count,..} => if count>MAX_DATA_SEGMENTS {return Err(Rejection::ResourceLimit);},
            Payload::DataSection(reader) => {
                add(&mut counts.data_segments,reader.count(),MAX_DATA_SEGMENTS)?;
                for data in reader {
                    let data=data.map_err(|_|Rejection::InvalidWasm)?;
                    counts.data_bytes=counts.data_bytes.checked_add(data.data.len()).filter(|n|*n<=MAX_DATA_BYTES).ok_or(Rejection::ResourceLimit)?;
                    if let DataKind::Active {memory_index,offset_expr} = data.kind {
                        let mut ops=offset_expr.get_operators_reader();
                        let offset=match ops.read().map_err(|_|Rejection::InvalidWasm)? {
                            Operator::I32Const {value} if value>=0 => value as u64,
                            _ => return Err(Rejection::DataInitializer),
                        };
                        if !matches!(ops.read().map_err(|_|Rejection::InvalidWasm)?,Operator::End) || !ops.eof() || memory_index!=0
                            || shape.memory.is_none_or(|memory|offset.checked_add(data.data.len() as u64).is_none_or(|end|end>memory.initial*65536)) {
                            return Err(Rejection::DataInitializer);
                        }
                    }
                }
            }
            Payload::CodeSectionStart {count,..} => if count>MAX_FUNCTIONS {return Err(Rejection::ResourceLimit);},
            Payload::CodeSectionEntry(body) => {
                add(&mut counts.code_bodies,1,MAX_FUNCTIONS)?;
                if body.as_bytes().len()>MAX_BODY_BYTES {return Err(Rejection::ResourceLimit);}
                let mut locals=body.get_locals_reader().map_err(|_|Rejection::InvalidWasm)?;
                if locals.get_count()>MAX_LOCALS_PER_FUNCTION {return Err(Rejection::ResourceLimit);}
                let mut local_count=0;
                for _ in 0..locals.get_count() {
                    let (count,ty)=locals.read().map_err(|_|Rejection::InvalidWasm)?;
                    if !numeric(ty) {return Err(Rejection::TypeForbidden);}
                    add(&mut local_count,count,MAX_LOCALS_PER_FUNCTION)?; add(&mut counts.local_values,count,MAX_TOTAL_LOCALS)?;
                }
                let mut ops=body.get_operators_reader().map_err(|_|Rejection::InvalidWasm)?;
                let mut depth=1u32;
                while !ops.eof() {
                    add(&mut counts.instructions,1,MAX_INSTRUCTIONS)?;
                    match ops.read().map_err(|_|Rejection::InvalidWasm)? {
                        Operator::Block {..}|Operator::Loop {..}|Operator::If {..} => add(&mut depth,1,MAX_CONTROL_DEPTH)?,
                        Operator::End => depth=depth.checked_sub(1).ok_or(Rejection::InvalidWasm)?,
                        Operator::BrTable {targets} if targets.len()>MAX_BRANCH_TARGETS => return Err(Rejection::ResourceLimit),
                        _=>{},
                    }
                }
            }
            Payload::CustomSection(reader) => {
                counts.custom_bytes=counts.custom_bytes.checked_add(reader.range().len()).filter(|n|*n<=MAX_CUSTOM_BYTES).ok_or(Rejection::ResourceLimit)?;
            }
            Payload::End(_)=>{},
            _=>return Err(Rejection::SectionForbidden),
        }
    }
    Ok(shape)
}
fn abi(shape: &Shape<'_>, manifest: &Manifest) -> Result<(),Rejection> {
    if shape.memory.is_none() {return Err(Rejection::MemoryProfile);}
    let names=[ALLOC_EXPORT,manifest.entrypoints.validate_payload.as_str(),manifest.entrypoints.apply_edits.as_str(),manifest.entrypoints.render_layer.as_str()];
    if names.iter().any(|name|*name==MEMORY_EXPORT) || names[1..].contains(&ALLOC_EXPORT) || shape.exports.len()!=5
        || shape.exports.get(MEMORY_EXPORT)!=Some(&(ExternalKind::Memory,0)) {return Err(Rejection::AbiExports);}
    for (i,name) in names.iter().enumerate() {
        let (kind,index)=shape.exports.get(name).ok_or(Rejection::AbiExports)?;
        if *kind!=ExternalKind::Func {return Err(Rejection::AbiExports);}
        let ty=shape.functions.get(*index as usize).and_then(|ty|shape.types.get(*ty as usize)).ok_or(Rejection::AbiSignatures)?;
        let (params,results):(&[ValType],&[ValType])=if i==0 {(&[ValType::I32],&[ValType::I32])} else {(&[ValType::I32,ValType::I32],&[ValType::I64])};
        if ty.params()!=params || ty.results()!=results {return Err(Rejection::AbiSignatures);}
    }
    Ok(())
}

pub(crate) fn inspect(manifest: &Manifest, manifest_bytes: &[u8], artifact: &[u8]) -> Report {
    let mut report=Report { version:VERSION,policy:"perveance-static-abi-v1",inspector:"wasmparser-0.240.0",
        reference:manifest.reference.clone(),manifest_sha256:hash(manifest_bytes),artifact_sha256:hash(artifact),artifact_bytes:artifact.len(),
        static_profile_passed:false,structural_validation:Validation::NotRun,abi_signatures_match:false,counts_complete:false,
        counts:Counts::default(),rejection:None,module_instantiated:false,code_executed:false,runtime_limits_enforced:false,
        behavior_verified:false,payload_schemas_verified:false,provenance_verified:false,capability_verified:false,
        activation_permitted:false,registry_modified:false,installed:false,active:false,available:false,autonomous_execution:false };
    if manifest.reference.abi_version!=1 || manifest.document_version!=DOCUMENT_VERSION {
        report.rejection=Some(Rejection::UnsupportedManifestContract);return report;
    }
    let shape=match scan(artifact,manifest,&mut report.counts) {
        Ok(shape)=>shape,
        Err(issue)=>{report.rejection=Some(issue);return report;}
    };
    report.counts_complete=true;
    // Explicit feature set, not the parser's evolving defaults. Floats are
    // permitted, but determinism/NaN behavior is NOT proved. No imports, start,
    // tables, memory64, shared memory, threads, references, SIMD or components.
    let features=WasmFeatures::MUTABLE_GLOBAL|WasmFeatures::SATURATING_FLOAT_TO_INT|WasmFeatures::SIGN_EXTENSION
        |WasmFeatures::MULTI_VALUE|WasmFeatures::BULK_MEMORY|WasmFeatures::FLOATS;
    if Validator::new_with_features(features).validate_all(artifact).is_err() {
        report.structural_validation=Validation::Failed; report.rejection=Some(Rejection::InvalidOrUnsupportedWasm);return report;
    }
    report.structural_validation=Validation::Passed;
    if let Err(issue)=abi(&shape,manifest) {report.rejection=Some(issue);return report;}
    report.abi_signatures_match=true;report.static_profile_passed=true;report
}
