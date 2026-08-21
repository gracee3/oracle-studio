set shell := ["bash", "-uc"]

astraeus_swiss_revision := "cae9ecd4b201544d85e411aced17660932514d43"
astraeus_swiss_base_url := "https://raw.githubusercontent.com/aloistr/swisseph/" + astraeus_swiss_revision + "/ephe"
astraeus_swiss_ephemeris_path := env_var_or_default("ASTRAEUS_SWISS_EPHEMERIS_PATH", env_var_or_default("XDG_DATA_HOME", env_var("HOME") + "/.local/share") + "/astraeus/swisseph")

# List available recipes.
default:
    @just --list

# Download and verify the pinned Swiss Ephemeris data files.
astraeus-swiss-download data_dir=astraeus_swiss_ephemeris_path:
    #!/usr/bin/env bash
    set -euo pipefail
    readonly data_dir="{{ data_dir }}"
    readonly temporary_dir="$(mktemp -d)"
    cleanup() {
        rm -f \
            "$temporary_dir/sepl_18.se1" \
            "$temporary_dir/semo_18.se1" \
            "$temporary_dir/seas_18.se1"
        rmdir "$temporary_dir"
    }
    trap cleanup EXIT
    mkdir -p "$data_dir"
    for file in sepl_18.se1 semo_18.se1 seas_18.se1; do
        curl \
            --fail \
            --location \
            --retry 3 \
            --retry-all-errors \
            --silent \
            --show-error \
            --output "$temporary_dir/$file" \
            "{{ astraeus_swiss_base_url }}/$file"
    done
    (
        cd "$temporary_dir"
        printf '%s  %s\n' \
            ca1393ceab3a44fbc895887cf789c68819ae6a1cbc9b22225872dbe4ccd99a66 sepl_18.se1 \
            1ca07bd67c24374d77226180c20a4f9996cba013697894810518e7eb582ca4f7 semo_18.se1 \
            a2cd8fc33807c78ca9a700c91c2e042258b12fc4796519e00781440b5ad8b2e2 seas_18.se1 \
            | sha256sum --check -
    )
    install -m 0644 "$temporary_dir/sepl_18.se1" "$data_dir/sepl_18.se1"
    install -m 0644 "$temporary_dir/semo_18.se1" "$data_dir/semo_18.se1"
    install -m 0644 "$temporary_dir/seas_18.se1" "$data_dir/seas_18.se1"
    printf 'Swiss Ephemeris data installed in %s\n' "$data_dir"

# Verify the configured Swiss Ephemeris data files without downloading.
astraeus-swiss-check data_dir=astraeus_swiss_ephemeris_path:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ data_dir }}"
    printf '%s  %s\n' \
        ca1393ceab3a44fbc895887cf789c68819ae6a1cbc9b22225872dbe4ccd99a66 sepl_18.se1 \
        1ca07bd67c24374d77226180c20a4f9996cba013697894810518e7eb582ca4f7 semo_18.se1 \
        a2cd8fc33807c78ca9a700c91c2e042258b12fc4796519e00781440b5ad8b2e2 seas_18.se1 \
        | sha256sum --check -

# Run the opt-in Swiss-file adapter integration test.
astraeus-swiss-test data_dir=astraeus_swiss_ephemeris_path:
    just astraeus-swiss-check "{{ data_dir }}"
    ASTRAEUS_SWISS_EPHEMERIS_PATH="{{ data_dir }}" cargo test -p astraeus-swiss --test adapter --locked -- --ignored swiss_
    ASTRAEUS_SWISS_EPHEMERIS_PATH="{{ data_dir }}" cargo test -p astraeus-cli --test cli --locked -- --ignored swiss_

# Download, verify, and test the pinned Swiss Ephemeris data.
astraeus-swiss-setup data_dir=astraeus_swiss_ephemeris_path:
    just astraeus-swiss-download "{{ data_dir }}"
    just astraeus-swiss-test "{{ data_dir }}"
    @printf 'To configure this shell, run: eval "$(just astraeus-swiss-env)"\n'

# Print the export needed to configure the current shell.
astraeus-swiss-env data_dir=astraeus_swiss_ephemeris_path:
    @printf 'export ASTRAEUS_SWISS_EPHEMERIS_PATH=%q\n' "{{ data_dir }}"

# Generate the fictional demo plaintext, manifest, and a fresh encrypted envelope.
demo-generate:
    cargo run --locked --quiet -p oracle-studio-demo -- generate var/demo/generated

# Verify deterministic plaintext/content IDs and fresh encrypted outputs.
demo-verify:
    cargo run --locked --quiet -p oracle-studio-demo -- verify fixtures/demo/oracle-studio-demo.lock.json

# Build the opt-in demo site. Ordinary Trunk and Docker builds remain demo-free.
demo-build: demo-generate
    #!/usr/bin/env bash
    set -euo pipefail
    readonly repository="$(pwd)"
    (
        cd crates/oracle-studio-ui
        env -u NO_COLOR trunk build demo.html --release --locked=true --dist "$repository/var/demo/site"
    )
    install -d var/demo/site/demo
    install -m 0644 \
        var/demo/generated/oracle-studio-demo.oracle-vault \
        var/demo/site/demo/oracle-studio-demo.oracle-vault
    python3 scripts/csp-hashes.py var/demo/site/index.html var/demo/site/oracle-csp.conf

# Serve the generated demo on the loopback interface for local review.
demo-serve: demo-build
    python3 -m http.server --bind 127.0.0.1 --directory var/demo/site 8080

# Run canonical builder checks plus the isolated demo browser acceptance.
demo-test:
    cargo test -p oracle-studio-demo --locked
    just demo-verify
    scripts/demo-acceptance.sh
