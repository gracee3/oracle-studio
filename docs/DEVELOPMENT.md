# Development and repository safeguards

Oracle Studio favors local validation during rapid development. GitHub Actions
remains enabled, but no workflow runs on a push, pull request, merge, or
schedule. The main `CI` workflow is started explicitly with **Run workflow** and
a required `all`, `native`, `wasm`, `release`, `browser`, or `dependencies`
suite. The separately manual `Astraeus Swiss file verification` workflow keeps
licensed data on a self-hosted runner behind the
`swiss-ephemeris-verification` environment.

The lack of automatic hosted checks does not weaken the local validation
boundary. Run the applicable subset and record exact commands and results in
the pull request:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo check --locked --target wasm32-unknown-unknown \
  -p astraeus-moshier -p oracle-studio-worker -p oracle-studio-ui \
  -p oracle-studio-chart-player
(cd crates/oracle-studio-ui && trunk build --release --locked=true)
cargo deny check
git diff --check
```

Docker and browser acceptance, catalog downloads, and the Swiss-file suite are
explicit exceptional checks. Select them only when the change and reviewed
data boundary require them. Ordinary builds and tests remain CPU-only, use
fictional or reviewed non-personal public fixtures, and never require models,
GPUs, personal charts, vaults, or licensed ephemeris files.

## Safeguard audit

The repository settings and checked-in policy were reviewed on 2026-08-21:

- `main` has no branch protection, ruleset, required status check, or required
  review. Auto-merge is disabled; merge, squash, and rebase methods are
  available, and source branches are retained.
- Actions has read-only default workflow permissions and cannot approve pull
  requests. Actions stays enabled solely for deliberate manual dispatch.
- Secret scanning and push protection stay enabled. There is no `CODEOWNERS`,
  dependency-automation configuration, pull-request template, or issue
  template in the repository.
- The Swiss workflow retains a dedicated environment and runner labels. It
  reads only runner-local, checksum-pinned files and never uploads, downloads,
  or caches `.se1` data.
- Encryption boundaries, transactional revision checks, licensed-data rules,
  fixture provenance, privacy constraints, destructive-operation caution, and
  review of schema, dependency, storage, and container changes remain active.

Standalone `gracee3/astraeus` is archived. It was not unarchived or mutated for
this policy change; its imported guidance is marked historical and active
engine development follows Oracle Studio's root instructions.

Oracle Studio is not managed by an external Weekly portfolio. A branch, exact
commit, pull request, validation record, risks or decisions, and next action are
the complete delivery record.
