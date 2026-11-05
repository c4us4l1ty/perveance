# Scoped model broker and durable reservation ledger

**2026-11-05 — implemented Linux component; shipped transport is OFFLINE ONLY.**
This does not enable generated execution, a real provider, a coding harness or
an autonomy intake gate. The existing manually invoked OpenRouter prototype is
unchanged and is **not** covered by this ledger. Do not mistake synthetic fixture
reservations for actual provider spending or an account-wide billing cap.

## Authority and credential boundary

`broker::Broker` owns a `Credential`, a locked `Ledger` and a trusted `Transport`.
A peer receives only one end of an already connected Unix socket pair. There is
no pathname listener, credential/token in JSON, URL/model selection, filesystem
operation, refund or policy-update command. Possession of that connected endpoint
is the scoped authority; request IDs are deduplication keys, **not** passwords or
authentication. The trusted launcher must pass only the intended endpoint and
keep credential/ledger roots, process memory and host descriptors inaccessible.

`Credential::from_bytes` takes ownership of a bounded printable ASCII allocation;
`from_private_file` uses the descriptor-rooted private-file checks from
[WORKER_IO.md](WORKER_IO.md). There is no automatic environment or command-line
key loader. Debug is redacted; the type is not Clone/Serialize/Display. Only
trusted adapter code can borrow the credential with `with_exposed`. The source
credential file, when used, is host-private plaintext, **not** an OS keychain or
worker mount. A best-effort wipe covers only the owned allocation, not transport
copies, swap, core dumps or a compromised host. A hardened broker process and OS
credential-store integration remain separate work.

The server never serializes a credential. Requests containing its literal value
are rejected before reservation, and provider text containing that value is
withheld. This is a defensive literal-echo check, not arbitrary secret taint
tracking, encoded-secret detection or a claim that a malicious trusted adapter
cannot leak a key. Prompt Debug is redacted too. Model text remains untrusted
data, not permission to run code, fetch a URL or install a capability.

## Immutable trusted policy, bounded wire protocol

The host selects and persists a `Route` (provider/model/billing revision), call
count, total reservation ceiling, a **positive known per-call upper charge** and
maximum output tokens. The transport's route must match exactly. Worker JSON
contains only:

```json
{"version":1,"id":"00000000000000000000000000000001","prompt":"offline fixture request","max_output_tokens":10}
```

A real caller should generate a fresh stable 128-bit lowercase hexadecimal ID
per intended call, retaining it across uncertainty instead of inventing a new
retry ID. The same ID with a different canonical request is a conflict, including
changed output-token limits. The trusted route and budget never come from that
request. Missing/unknown/duplicate fields, unsupported versions, invalid IDs,
empty/over-4,000-byte prompts and excessive/zero output-token limits are denied.
Zero total call/cost ceilings prohibit calls. Policies allow at most 128 calls
and 4,096 output tokens; arithmetic is checked, including `u64` cost overflow.

Frames use a four-byte big-endian byte length then strict UTF-8 JSON. Request
frames are at most 8 KiB, reply frames 64 KiB and completion text 8 KiB. JSON
escaping can make an otherwise bounded prompt exceed the encoded-frame cap;
it is still rejected. Invalid lengths are rejected **before allocation**. Reads
and writes use nonblocking I/O, poll and one absolute 1.5-second deadline each,
not a timeout restarted by every byte. Each `serve_once` handles one exchange
then closes its endpoint. There is no general network listener or connection
accept loop in this increment.

The trusted adapter's own duration, allocation and I/O are not bounded by these
socket deadlines. Closing/losing a client channel is **not cancellation** of an
already reserved/in-flight call. The default client helper is for short offline
exchanges; a real harness must implement its separately verified provider,
request/response bounds, deadlines, cancellation and lifecycle contract.

## Durable accounting before dispatch, no refunds or ambiguous retries

One host-authorized mode-0700 directory is one budget domain. Its sole broker
file, `model-budget.v1`, is owned/private/single-link and mode 0600 when created.
Mandatory `openat2` and held-inode checks reject links/special files before
readable/writable opening. File descriptors are close-on-exec. Renaming the
original root path cannot redirect a live ledger. `O_EXCL` initialization never
adopts, replaces, truncates or resets any occupied name, even a broken old file.

An exclusive nonblocking `flock` is held for the **entire Ledger lifetime**;
another cooperating process fails busy rather than falling back to stale or
unlocked counters. The immutable header has a random scope ID and policy.
Length-framed, SHA-256-chained records encode exactly these transitions:

1. **Reserved**: stable call ID, scope-bound canonical-request fingerprint and
   full trusted upper cost. Check both ceilings, then write and sync the journal
   and its directory **before** invoking any provider adapter.
2. **Finished**: bounded state/usage/response-size metadata. Only after the
   settlement is also synced may a successful completion text reach the peer.

Actual reported cost never frees capacity. Every reservation permanently uses
one call and the entire upper cost, even if actual usage is zero or a request
fails, is cancelled, times out, crashes or loses its reply. There is no refund,
reset, resume, automatic retry or policy replacement API. This deliberately
sacrifices availability to avoid pretending a possibly billed request was free.

A reservation without a settlement is `reserved_uncertain`. Unknown billing or
transport outcome, rejected/oversized/key-echo text and a reported charge above
the bound latch the ledger closed to **new** calls. An uncertain reservation is
never redispatched on reopen or same-ID retry. Completed same-ID requests return
`already_finished` without text: responses are not durably cached. A lost reply
may therefore lose the useful completion; it must not silently cause a new call.
A real job/response-recovery design remains required.

Write/sync failure poisons the live handle and withholds success/text. Reports
mark `accounting_uncertain:true`; counters then describe only transitions that
handle confirmed, **not** spare call authority. Inspect durable state with a new
handle after the old owner exits. Complete visible records are synced on open;
partial tails, missing header, bad chain, unknown records and illegal sequences
fail closed without truncation, repair, refund or replay. A completely visible
settlement after an uncertain sync still consumes its full reservation and
cannot dispatch again.

The journal is capped at 1 MiB/257 records; state replay is bounded. Raw prompts,
credentials and model text are not journaled. Fingerprints/lengths are metadata,
not encryption: low-entropy prompts may be guessable from their hashes. The
inspection report intentionally omits fingerprints as well as all raw content.

**Limits of evidence:** hashes are not MACs/signatures. Malicious same-user host
processes, host/root compromise, filesystem rollback, copying a valid ledger,
whole-valid-tail removal and dishonest/network filesystems are not prevented.
Callers must protect and reuse one authorized budget root; workers must not
create fresh roots, edit files or roll back budgets. Successful fsync is syscall
evidence, not a guarantee against every power-loss scenario. Kernel/filesystem
I/O can block outside worker quotas; this is not a real-time service.

## Useful offline CLI and packaged regression

No real credential is accepted by these commands. They use a fixed synthetic
key inside the server and a constant text transport with synthetic usage.

```sh
mkdir -m 700 broker-ledger broker-requests
printf '%s' '{"version":1,"id":"00000000000000000000000000000001","prompt":"offline fixture request","max_output_tokens":10}' > broker-requests/call.json
chmod 600 broker-requests/call.json
perveance-cli broker-ledger-init /absolute/broker-ledger 2 20 10
perveance-cli broker-fixture-call /absolute/broker-ledger /absolute/broker-requests call.json
perveance-cli broker-ledger-inspect /absolute/broker-ledger
```

A second identical call returns `already_finished`, not another dispatch or
cached text. A new ID uses another full reservation if the ledger is healthy
and both ceilings permit it. `ok:true`/CLI exit success means the offline exchange
was processed: always inspect `fixture.reply.status` and `ledger.halted`. It does
not mean a model call succeeded, billing is safe or autonomy is authorized.
Inspection takes the same exclusive lock and can fail busy while a broker lives.
No worker, cgroup, provider network, API key or generated code is needed.

## Verification and still-open gates

New Rust tests cover durable reopen/dedup/conflicts, zero/exhausted/overflowed
budgets, immutable occupied files/links/special files, private key capture,
cooperating-process contention, frame/schema/override attacks, fragmented and
slow-drip input, lost replies, credential echo, unknown/over-bound billing,
three reservation/settlement sync fault points and malformed journal replay.
A separate fixed Python peer checks framed IPC and descriptor/environment
hygiene; it is **not** sandbox or hostile same-user isolation proof. Another
subprocess is really SIGKILLed after entering its fake adapter: the reservation
survives and neither an identical retry nor a new call dispatches afterward.
The downloaded Linux CLI now exercises restart/dedup/exhaustion too.

At implementation commit `258cc80`, [Rust run 42011716176](https://github.com/c4us4l1ty/perveance/actions/runs/42011716176)
passed all 14 new unit tests and the eight-case integration harness (including
its subprocess helper/SIGKILL paths), the full Rust/native-GUI/bridge/recovery
regression and the downloaded CLI's reservation/restart/dedup/exhaustion checks.
[Feasibility run 42011716112](https://github.com/c4us4l1ty/perveance/actions/runs/42011716112)
also passed all seven existing real cgroup-worker tests on the delegated Ubuntu
26.04 service: kernel `7.0.0-1019-azure`, bubblewrap `0.11.1`, AppArmor userns
restriction still `1`. This is regression evidence for the fixed worker, not
proof that the new broker has been integrated into those protected roles. The
ordinary Rust runner's optional namespace probe still could not start. All 41
local Python tests pass; Rust/kernel evidence comes from CI because no local
Rust toolchain is available. None of these tests made a real provider call.

Protected broker/worker role separation, production task/intake/GUI integration,
provider-specific token/billing enforcement, a real compatible harness/route,
provider cancellation/progress, response recovery and independent adversarial
review remain open. A positive cost declaration alone is **not** proof of a
provider's maximum charge; hidden retries, tokenization, pricing changes and
unknown billing must be resolved before enabling any paid autonomous route.
The Rust Transport trait is a trusted host-adapter seam, not an extension or
worker escape hatch. Only the offline transport is shipped. Autonomy stays off.
