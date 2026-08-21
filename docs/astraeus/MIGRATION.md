# Astraeus full-history migration

Oracle Studio absorbed the complete standalone Astraeus workspace on
2026-08-21 without squashing or renaming its Rust crates or public APIs.

## Source checkpoint

- Source repository: `https://github.com/gracee3/astraeus`
- Final standalone commit: `44af176ef8a85db2bbd7b57228710855a8fe6f3b`
- Annotated source tag: `astraeus-standalone-final`
- Oracle import commit: `f0588b70fbd4462f709cc926ea319e7d892e742d`

The import commit is a two-parent commit whose first parent is Oracle Studio
`main` and whose second parent is the final standalone Astraeus commit. The
source tree was read under the temporary `astraeus-import/` prefix in that
commit, then relocated and integrated in later commits. Therefore the exact
standalone SHA and all of its reachable history are ancestors of the Oracle
migration branch.

## Path mapping

| Standalone path | Consolidated path |
| --- | --- |
| `crates/astraeus-*` | `crates/astraeus-*` |
| `docs/*` | `docs/astraeus/*` |
| `fixtures/*` | `fixtures/astraeus/*` |
| `examples/*` | `examples/astraeus/*` |
| `README.md` | `docs/astraeus/README.md` |
| `AGENTS.md` | `docs/astraeus/STANDALONE_AGENT_GUIDANCE.md`, with active rules in root `AGENTS.md` |
| Swiss `Justfile` recipes | root `Justfile` with `astraeus-*` recipe names |
| Swiss verification workflow | `.github/workflows/astraeus-swiss-files.yml` |
| root Cargo, lock, CI, deny, license, and toolchain policy | consolidated Oracle root files |

All imported packages retain version `0.1.0` and are marked `publish = false`.
Oracle crates consume them through the root workspace path-dependency table;
the consolidated lockfile contains no Astraeus Git source.

## Verification and extraction

Verify ancestry from any clone containing the migration commit:

```text
git merge-base --is-ancestor 44af176ef8a85db2bbd7b57228710855a8fe6f3b HEAD
```

To recover the exact standalone tree, check out or archive the final source
commit. To preserve its complete reachable history in a portable bundle, first
name that commit locally and then bundle the ref:

```text
git branch astraeus-standalone-final 44af176ef8a85db2bbd7b57228710855a8fe6f3b
git archive --format=tar astraeus-standalone-final > astraeus-standalone-final.tar
git bundle create astraeus-standalone-final.bundle astraeus-standalone-final
```

Later extraction of only the integrated subsystem can filter the
`crates/astraeus-*`, `docs/astraeus`, `fixtures/astraeus`, and
`examples/astraeus` paths while retaining commits from this migration onward.
