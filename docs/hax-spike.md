# Spike: mechanical Rust → Coq extraction with `hax`

**Status**: feasibility report (v0.3 spike, 2026-05-17). NOT production
direction.

## Context

The Coq formalism in `formal/coq/` is a hand-written parallel artifact
that proves filesystem invariants on an abstracted graph model. The
Rust implementation in `sotfs-graph` and `sotfs-ops` is correlated by
convention only — the cross-references added in PR #19 and the runtime
check parity in PR #18 detect drift via tests, not via formal proof.

This spike investigates whether [hax](https://github.com/hacspec/hax)
(Inria/AWS, used in production for Signal Protocol, Kyber, ML-KEM) can
take our Rust source and *mechanically generate* Coq files that
reference the actual Rust functions, enabling formal refinement-style
verification.

## What we tried

1. `cargo +nightly install cargo-hax` — installed `cargo-hax` 0.3.6
   plus `hax-export-json-schemas` to `~/.cargo/bin/`.
2. First run: `cargo hax -C -p sotfs-graph \; into coq`. **Failed**:
   hax-driver requires a specific nightly toolchain
   (`nightly-2025-11-08` at the time of writing) and the
   `rust-src`/`rustc-dev`/`llvm-tools-preview` components for that
   toolchain.
3. Installed the matching components and re-ran. (See "Outcome"
   below.)

## Constraints `hax` imposes on Rust source

Even if the tooling works end-to-end, the Rust subset `hax` accepts is
restrictive. Known incompatibilities for sotFS:

| sotFS feature | hax-supported? | Source |
|---|---|---|
| `unsafe` blocks (RCU snapshots, `MaybeUninit` arena) | ❌ no | hax design |
| FFI to FUSE (sotfs-fuse) | ❌ no | hax can't model the C ABI |
| `no_std` + `alloc` feature gate | ⚠ partial | needs careful setup |
| `BTreeMap` / `BTreeSet` from `alloc` | ✅ yes | hax models them |
| Generic types with trait bounds | ✅ yes | with `#[hax_lib::trait]` |
| Recursive functions (no mutual recursion) | ✅ yes | |
| `match` on enums, struct literals, `if let` | ✅ yes | |
| `dyn Trait` / trait objects | ❌ no | |
| Macros (after expansion) | ✅ yes | hax operates on MIR |

For sotFS specifically:

- **`sotfs-graph::arena` and `sotfs-graph::rcu`** use `unsafe` for
  performance and memory layout. These crates are NOT translatable
  as-is; they'd need a pure-Rust shadow implementation or `hax_lib`
  axioms.
- **`sotfs-fuse`** is entirely out of scope (C ABI).
- **`sotfs-ops::create_file` / `mkdir` / etc.** are the main candidates.
  They're nominally pure functions over `&mut TypeGraph`, but they
  call into `sotfs-graph` helpers (some of which use `unsafe`).

## Realistic plan if `hax` is to be adopted

Even in the optimistic case where hax accepts most of our source:

1. **Factor out a `sotfs-core` crate** containing only the pure
   subset hax accepts: type definitions, DPO rule bodies, invariant
   checks. No unsafe, no FFI.
2. **Axiomatize the unsafe internals** (arena allocator, RCU) via
   `hax_lib` markers so the verified slice doesn't try to translate
   them.
3. **Annotate DPO functions** with `#[hax_lib::requires]` and
   `#[hax_lib::ensures]` matching the Coq pre-/postconditions we
   already wrote in `RmdirPre`, `MkdirPre`, etc.
4. **Run `cargo hax into coq`** to generate Coq files in
   `sotfs-ops/proofs/coq/extraction/`.
5. **Manually link** the generated Coq to the existing
   `SotfsGraph.v` definitions (the generated extraction will use its
   own types unless we provide `hax_lib::map_to` annotations).
6. **Prove** that the generated extraction satisfies the Coq
   `*_preserves_WellFormed` theorems.

Effort estimate: **multi-week refactor** of `sotfs-graph` and
`sotfs-ops` to be hax-compatible, before any verification value is
delivered.

## Outcome of this spike

We got as far as installing the Rust components and confirming the
Rust subset gate, but not as far as a successful translation.

| Step | Status | Notes |
|---|---|---|
| `cargo +nightly install cargo-hax` | ✓ | ~4 min build; requires specific pinned nightly (hax 0.3.6 wants `nightly-2025-11-08`) |
| `rustup component add rust-src rustc-dev llvm-tools-preview` for the pinned nightly | ✓ | Standard rustup |
| `cargo +nightly-2025-11-08 install hax-driver` | ✓ | Provides `driver-hax-frontend-exporter` in `~/.cargo/bin/` |
| `cargo hax -C -p sotfs-graph \; into coq` | ❌ | "The binary [hax-engine] was not found in your [PATH]." The Rust frontend is installed, but the OCaml engine isn't |
| `opam pin add ./engine` (from cloned `hacspec/hax` repo) | ❌ | opam requires system packages (`libgmp`, `m4`, etc.) installable only via `yum`/`sudo`. Halted at this step rather than escalate |

So the spike confirms: **`hax` requires a hybrid Rust/OCaml toolchain
plus system packages plus a specific nightly Rust**. Even on a
well-set-up dev machine with OCaml/Coq already installed (which we
have via PR #15 → #17), getting hax to run end-to-end is a half-day
of setup before any translation attempt.

We did not get to test whether the Rust subset is acceptable on our
specific code, but the constraints listed above (no `unsafe`, no
FFI, no `dyn Trait`) are intrinsic to hax's design — those would
bite regardless of installation success.

## Recommendation

## Recommendation

**Do not** make `hax` extraction a v0.3 goal.

Reasoning:

- The runtime parity (`check_invariants()` runs all 7 Coq invariants;
  cross-check tests assert preservation after each DPO op) gives us
  drift detection at low cost.
- The Coq formalism is *already complete* (PR #17 closed every
  `Admitted`). Its value is design clarity + paper-grade
  verification, not mechanical refinement.
- `hax` adoption would require redesigning `sotfs-graph`'s unsafe
  internals as a separate crate, which is a bigger commitment than
  we should make without a stronger formal-verification driver.

**Alternative for v0.3** (deferred, lower cost, similar value):

- Document the Coq ↔ Rust correspondence at the lemma level (done in
  PR #19).
- Add a CI workflow that runs `coqc` on every PR (PR #21).
- Use proptest sequences + `check_invariants()` to catch
  implementation drift (already in `proptest_ops.rs`).
- Revisit `hax` (or `Aeneas`/`Creusot`) if sotFS gains a customer with
  a "must be formally verified end-to-end" requirement.
