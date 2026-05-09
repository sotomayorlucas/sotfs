# Contributing to sotFS

Thanks for your interest. sotFS is small and the rules are dense:

## Hard rules

1. **Every change must cite an invariant or DEUDA-NNN it protects.** No
   "general cleanup" PRs. If a refactor doesn't close a concrete hole, it
   doesn't merge. The original sotX hardening contract still applies —
   see [docs/state.md](docs/state.md) §threat-model.
2. **Every change has a regression test.** A unit test, a proptest case,
   a fuzz seed, or a TLA+ counterexample model — pick the one that
   actually fails on the *previous* code. Tests that pass before and
   after the change don't count.
3. **Public API changes need SemVer thinking.** The crates here are
   consumed by sotX (and others). Breaking a re-exported type triggers a
   minor or major bump per [SemVer](https://semver.org).
4. **Formal-touched code requires a TLA+ delta.** If you change
   `sotfs-graph::TypeGraph` invariants or `sotfs-tx::Gtxn` semantics,
   update the corresponding spec in `formal/` and re-run `just formal`
   in the same PR.

## Workflow

```sh
# 1. Fork + clone, then create a branch.
git checkout -b fix/<area>-<short-name>

# 2. Run the suite locally before pushing.
just test           # fast subset
just test-full      # release + 10 000 proptest cases
just formal         # TLC on all six specs (slow)

# 3. Commit message format: <scope>: <imperative summary>
#    Example: fix(sotfs-graph): rename in-place updates dir_name_idx
#    Body must explain the bug and reference the invariant.

# 4. Push and open a PR. CI runs ci.yml + coverage.yml + fuzz.yml.
```

## What to send

- **Bug fixes**: minimal patch + regression test. Cite the failing input.
- **POSIX completeness**: pick a missing FUSE callback, implement,
  add an integration test using a real shell command.
- **Performance**: include `cargo bench` numbers vs `main` and a
  reproducer fixture.
- **Formal**: counterexamples found by TLC are gold. Open an issue, paste
  the trace.

## What to skip

- Cosmetic refactors with no measurable effect.
- New features not in [README.md#roadmap](README.md#status). If you
  think one belongs, open an issue first.
- Changes to `services/system/sotfs/` — that wrapper lives in
  [sotX](https://github.com/sotomayorlucas/sotX) and depends on the
  kernel ABI.

## License

By contributing you agree your contributions are licensed under MIT.
