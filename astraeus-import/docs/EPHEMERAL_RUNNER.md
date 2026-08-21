# Ephemeral Swiss verification runner

The `Swiss file verification` workflow uses a self-hosted runner because
licensed `.se1` files must remain outside the public repository and hosted CI.
Never leave this runner persistently registered for the public Astraeus
repository. Register it with `--ephemeral`, run one manually dispatched job,
and confirm that GitHub removes the registration afterward.

## Pinned runner

- GitHub Actions runner: `v2.336.0`
- Linux x64 archive SHA-256:
  `04cf0be1aff4c3ec3554466c39124ca250e3effd8873bb7e8d68535aa9505d5d`

Download the archive from the official `actions/runner` release, verify the
digest, and extract it outside the repository. The runner-local ephemeris
directory must contain files matching `fixtures/swetest-v2.10.03/SWISS_PROVENANCE.md`.

## One verification run

With GitHub CLI authenticated for `gracee3/astraeus`, obtain a short-lived
registration token without printing it, then configure the extracted runner:

```text
runner_token=$(gh api --method POST \
  repos/gracee3/astraeus/actions/runners/registration-token --jq .token)
./config.sh --url https://github.com/gracee3/astraeus \
  --token "$runner_token" --name astraeus-swiss-ephemeral \
  --labels astraeus-swiss --unattended --ephemeral --work _work
./run.sh
```

Once the runner reports `Listening for Jobs`, dispatch `swiss-files.yml` from
`main` in another terminal. Wait for success and verify that the runner exits
and disappears from the repository runner list. Do not commit the runner,
registration files, work directory, or `.se1` data.
