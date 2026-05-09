# sotFS — Formal verification

Six TLA+ specs + Coq DPO proofs that verify the core invariants of the
type graph and the rewrite rules that operate on it.

## TLA+ specs

| Spec                           | Property                                                    |
|--------------------------------|-------------------------------------------------------------|
| `sotfs_graph.tla`              | Graph well-formedness: no cycles excluding `.`/`..`, unique names per dir, link-count = incoming Contains edges (G3), root has no `..`, etc. |
| `sotfs_transactions.tla`       | GTXN atomicity: commit ⇒ visible ∧ invariants; rollback ⇒ snapshot restored; 2PC PREPARE/COMMIT/ABORT phase rules. |
| `sotfs_capabilities.tla`       | Capability monotonicity: `derived ⊆ parent`; revoke transitively kills descendants; epoch invalidates stale handles. |
| `sotfs_crash.tla`              | WAL-based crash recovery: any prefix of the log replays to a valid state; uncommitted ops are dropped. |
| `sotfs_crash_refinement.tla`   | Refinement proof: crash model implements abstract spec under stuttering. |
| `sotfs_curvature.tla`          | Ollivier-Ricci curvature monitor: bounded structural deviation under adversarial mutations. |

Each spec has three sized config files (`*.cfg`, `*_medium.cfg`,
`*_large.cfg`) for bounded model checking under different state-space
budgets.

## Running

```sh
# All specs, all sizes:
just formal
# Equivalent to:
cd formal && bash run_tlc.sh

# Single size:
bash run_tlc.sh small
bash run_tlc.sh medium
bash run_tlc.sh large

# Single spec (all sizes):
bash run_tlc.sh sotfs_graph
```

Output goes to `formal/tlc_output/` plus a summary table on stdout.

## Setup

Requires Java 17+ on `PATH` and the `tla2tools.jar`:

```sh
# Place tla2tools.jar in formal/lib/, or set TLC_JAR env var.
mkdir -p formal/lib
curl -L -o formal/lib/tla2tools.jar \
  https://github.com/tlaplus/tlaplus/releases/latest/download/tla2tools.jar
```

The Dockerfile in the repo root pre-installs this.

## Coq proofs

`coq/` contains Rocq/Coq machine-checked proofs of the DPO rewrite rules:
`DpoCreate.v`, `DpoMkdir.v`, `DpoLink.v`, `DpoRename.v`, `DpoRmdir.v`,
`DpoUnlink.v`, plus the shared graph model in `SotfsGraph.v`. These prove
each rewrite preserves the seven graph invariants.

```sh
cd formal/coq && coq_makefile -f _CoqProject -o Makefile && make
```

## Status (at `v0.2.0`)

- TLC: 14/14 PASS at `small` and `medium` sizes (per `docs/state.md`).
  `large` configs may TIMEOUT depending on host RAM; gating is `medium`.
- Coq: 6 DPO files + the graph model compile and discharge their proofs;
  no `Admitted` lemmas.
