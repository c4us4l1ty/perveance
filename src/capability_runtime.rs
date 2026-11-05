//! Experimental metered interpreter diagnostic for the ABI-1 data-only contract.
//!
//! This is the first place candidate module code is actually executed. It is
//! deliberately **not** a security boundary or activation step: the module runs
//! in this process through the pure-Rust `wasmi` interpreter with an empty
//! linker (no host imports, no WASI), eager bounded translation, fuel metering,
//! linear-memory growth limits, output limits and a fresh store per call. The
//! metered facts reported here are engine facts, not independent verification of
//! behavior, payload schemas, provenance or usefulness, and nothing is mutated,
//! installed, activated, rendered or published.
use crate::{
    extension_protocol::{CandidateStatus, Kind, PreparedCall, ReplyReport},
    registry::Manifest,
};
use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub const VERSION: u32 = 1;
pub const ENGINE: &str = "wasmi-2.0.0";
pub const ALLOCATOR_EXPORT: &str = "perveance_alloc_v1";
pub const MEMORY_EXPORT: &str = "memory";
/// Bounds the interpreter's own call stack; a candidate cannot recurse freely.
pub const MAX_RECURSION_DEPTH: usize = 64;
pub const MAX_STACK_HEIGHT: usize = 4096;

/// Fixed outcome categories. No candidate text is echoed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The engine refused to validate or translate the artifact.
    #[default]
    CompilationRejected,
    /// Well-formed success reply for the exact prepared request.
    ReplyAccepted,
    /// The candidate returned bytes the strict reply decoder refused.
    ReplyRejected,
    /// A well-formed bounded non-success candidate status.
    CandidateFailure,
    /// The allocator returned zero/negative or an unusable request window.
    AllocationRejected,
    /// Instantiation, export lookup or fuel setup failed.
    InstantiationRejected,
    /// Fuel was exhausted before the entrypoint returned.
    OutOfFuel,
    /// A deterministic WebAssembly trap (unreachable, bounds, divide by zero...).
    Trapped,
    /// The interpreter panicked; the process survived because it was contained.
    InterpreterPanicked,
}

/// Runtime facts that are true of *this* diagnostic, plus every authority that
/// stays false. `fuel_metering_enabled` etc. describe configuration, not proof
/// that a candidate terminates usefully or that limits match production policy.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RuntimeGates {
    pub engine_compiled: bool,
    pub module_instantiated: bool,
    pub code_executed: bool,
    pub fuel_metering_enabled: bool,
    pub memory_limit_enabled: bool,
    pub output_limit_enabled: bool,
    pub deterministic_mode_enabled: bool,
    pub eager_translation: bool,
    pub host_imports_linked: bool,
    pub os_confinement_verified: bool,
    pub behavior_verified: bool,
    pub payload_schemas_verified: bool,
    pub provenance_verified: bool,
    pub capability_verified: bool,
    pub activation_permitted: bool,
    pub installed: bool,
    pub active: bool,
    pub available: bool,
    pub document_modified: bool,
    pub registry_modified: bool,
    pub png_published: bool,
    pub provider_network_used: bool,
    pub autonomous_execution: bool,
    pub authenticated: bool,
    pub replay_protection_enforced: bool,
}

#[derive(Debug, Serialize)]
pub struct RuntimeReport {
    pub version: u32,
    pub diagnostic_only: bool,
    pub engine: &'static str,
    pub kind: Kind,
    pub request_sha256: String,
    pub outcome: Outcome,
    pub fuel_declared: u64,
    pub fuel_remaining: Option<u64>,
    pub memory_declared_bytes: u64,
    pub output_declared_bytes: u64,
    pub instance_memory_bytes: Option<u64>,
    pub reply: Option<ReplyReport>,
    pub gates: RuntimeGates,
}

#[derive(Default)]
struct Run {
    outcome: Outcome,
    compiled: bool,
    instantiated: bool,
    executed: bool,
    fuel_remaining: Option<u64>,
    instance_memory_bytes: Option<u64>,
    reply: Option<ReplyReport>,
}

fn trap_outcome(error: &wasmi::Error) -> Outcome {
    match error.as_trap_code() {
        Some(wasmi::TrapCode::OutOfFuel) => Outcome::OutOfFuel,
        _ => Outcome::Trapped,
    }
}

fn engine() -> wasmi::Engine {
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    // Eager translation keeps first-use work inside the bounded setup phase and
    // avoids lazy code generation during a candidate call.
    config.compilation_mode(wasmi::CompilationMode::Eager);
    config.allow_start_fn(false);
    // memory64/simd/relaxed-simd are not compiled in, so they cannot be enabled.
    config.wasm_reference_types(false);
    config.wasm_tail_call(false);
    config.wasm_extended_const(false);
    config.wasm_multi_memory(false);
    config.wasm_custom_page_sizes(false);
    config.wasm_wide_arithmetic(false);
    config.set_max_recursion_depth(MAX_RECURSION_DEPTH);
    config.set_max_stack_height(MAX_STACK_HEIGHT);
    wasmi::Engine::new(&config)
}

fn execute(engine: &wasmi::Engine, manifest: &Manifest, artifact: &[u8], call: &PreparedCall) -> Run {
    let mut run = Run::default();
    let module = match wasmi::Module::new(engine, artifact) {
        Ok(module) => module,
        Err(_) => return Run { outcome: Outcome::CompilationRejected, ..run },
    };
    run.compiled = true;
    let limits = wasmi::StoreLimitsBuilder::new()
        .memory_size(manifest.runtime.memory_bytes as usize)
        .table_elements(0)
        .instances(1)
        .tables(0)
        .memories(1)
        .build();
    let mut store = wasmi::Store::new(engine, limits);
    store.limiter(|limits| limits as &mut dyn wasmi::ResourceLimiter);
    if store.set_fuel(manifest.runtime.fuel).is_err() {
        return Run { outcome: Outcome::InstantiationRejected, ..run };
    }
    let linker = wasmi::Linker::<wasmi::StoreLimits>::new(engine);
    let instance = match linker.instantiate_and_start(&mut store, &module) {
        Ok(instance) => instance,
        Err(_) => return Run { outcome: Outcome::InstantiationRejected, ..run },
    };
    run.instantiated = true;
    let Some(memory) = instance.get_memory(&store, MEMORY_EXPORT) else {
        return Run { outcome: Outcome::InstantiationRejected, ..run };
    };
    run.instance_memory_bytes = Some(memory.data(&store).len() as u64);
    let Ok(allocator) = instance.get_typed_func::<i32, i32>(&store, ALLOCATOR_EXPORT) else {
        return Run { outcome: Outcome::InstantiationRejected, ..run };
    };
    let entrypoint = match call.report().kind {
        Kind::ValidatePayload => manifest.entrypoints.validate_payload.clone(),
        Kind::ApplyEdits => manifest.entrypoints.apply_edits.clone(),
        Kind::RenderLayer => manifest.entrypoints.render_layer.clone(),
    };
    let Ok(entry) = instance.get_typed_func::<(i32, i32), i64>(&store, &entrypoint) else {
        return Run { outcome: Outcome::InstantiationRejected, ..run };
    };
    let request = call.request_bytes();
    let request_length = request.len() as i32;
    let pointer = match allocator.call(&mut store, request_length) {
        Ok(pointer) => pointer,
        Err(error) => {
            run.fuel_remaining = store.get_fuel().ok();
            return Run { outcome: trap_outcome(&error), ..run };
        }
    };
    run.executed = true;
    if pointer <= 0 {
        run.fuel_remaining = store.get_fuel().ok();
        return Run { outcome: Outcome::AllocationRejected, ..run };
    }
    let memory_bytes = memory.data(&store).len();
    let Ok(window) = call.request_window(memory_bytes, pointer as u32) else {
        run.fuel_remaining = store.get_fuel().ok();
        return Run { outcome: Outcome::AllocationRejected, ..run };
    };
    if memory.write(&mut store, window.start, request).is_err() {
        run.fuel_remaining = store.get_fuel().ok();
        return Run { outcome: Outcome::AllocationRejected, ..run };
    }
    let packed = match entry.call(&mut store, (pointer, request_length)) {
        Ok(packed) => packed,
        Err(error) => {
            run.fuel_remaining = store.get_fuel().ok();
            return Run { outcome: trap_outcome(&error), ..run };
        }
    };
    run.fuel_remaining = store.get_fuel().ok();
    let reply = call.inspect_memory_reply(memory.data(&store), packed);
    run.outcome = match reply.candidate_status {
        Some(CandidateStatus::Success) if reply.wire_accepted => Outcome::ReplyAccepted,
        Some(_) if reply.wire_accepted => Outcome::CandidateFailure,
        _ => Outcome::ReplyRejected,
    };
    run.reply = Some(reply);
    run
}

/// Run one prepared call against the captured artifact. Never mutates, installs,
/// activates or publishes anything; panics are contained and reported.
pub fn invoke(manifest: &Manifest, artifact: &[u8], call: &PreparedCall) -> RuntimeReport {
    let engine = engine();
    let mut gates = RuntimeGates {
        fuel_metering_enabled: true, memory_limit_enabled: true, output_limit_enabled: true,
        deterministic_mode_enabled: true, eager_translation: true, ..RuntimeGates::default()
    };
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(|| execute(&engine, manifest, artifact, call)));
    std::panic::set_hook(previous_hook);
    let run = match outcome {
        Ok(run) => run,
        Err(_) => {
            let call = call.report();
            return RuntimeReport {
                version: VERSION, diagnostic_only: true, engine: ENGINE, kind: call.kind,
                request_sha256: call.request_sha256, outcome: Outcome::InterpreterPanicked,
                fuel_declared: manifest.runtime.fuel, fuel_remaining: None,
                memory_declared_bytes: manifest.runtime.memory_bytes,
                output_declared_bytes: manifest.runtime.output_bytes,
                instance_memory_bytes: None, reply: None, gates,
            };
        }
    };
    gates.engine_compiled = run.compiled;
    gates.module_instantiated = run.instantiated;
    gates.code_executed = run.executed;
    let call = call.report();
    RuntimeReport {
        version: VERSION, diagnostic_only: true, engine: ENGINE, kind: call.kind,
        request_sha256: call.request_sha256, outcome: run.outcome,
        fuel_declared: manifest.runtime.fuel, fuel_remaining: run.fuel_remaining,
        memory_declared_bytes: manifest.runtime.memory_bytes,
        output_declared_bytes: manifest.runtime.output_bytes,
        instance_memory_bytes: run.instance_memory_bytes, reply: run.reply, gates,
    }
}
