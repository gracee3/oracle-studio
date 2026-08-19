#!/usr/bin/env bash
set -euo pipefail

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
image="oracle-studio-browser-acceptance:152.0.7977.54"
host="$repository/target/debug/oracle-studio-host"
dist="$repository/crates/oracle-studio-ui/dist"
run_root="$repository/var/browser-acceptance"
run_dir=""
studio_pid=""

cleanup() {
    if [[ -n "$studio_pid" ]] && kill -0 "$studio_pid" 2>/dev/null; then
        kill -TERM "$studio_pid"
        wait "$studio_pid" 2>/dev/null || true
    fi
    if [[ -n "$run_dir" ]]; then
        rm -f -- \
            "$run_dir/acceptance.oracle" \
            "$run_dir/.acceptance.oracle.lock" \
            "$run_dir/host.stderr"
        rmdir -- "$run_dir" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

if [[ ! -x "$host" ]]; then
    echo "missing $host; run cargo build --locked -p oracle-studio-server --bin oracle-studio-host" >&2
    exit 1
fi
if [[ ! -f "$dist/index.html" ]]; then
    echo "missing $dist; run (cd crates/oracle-studio-ui && trunk build --release)" >&2
    exit 1
fi

mkdir -p -- "$run_root"
chmod 0700 "$run_root"
run_dir=$(mktemp -d "$run_root/run.XXXXXX")

docker build \
    --quiet \
    --tag "$image" \
    "$repository/tools/browser-acceptance"

coproc STUDIO_HOST {
    "$host" --dist "$dist" 2>"$run_dir/host.stderr"
}
studio_pid=$STUDIO_HOST_PID

if ! IFS= read -r launch_line <&"${STUDIO_HOST[0]}"; then
    echo "Oracle Studio host exited before producing a launch URL" >&2
    sed -n '1,80p' "$run_dir/host.stderr" >&2
    exit 1
fi
if [[ ! "$launch_line" =~ ^Oracle\ Studio\ is\ ready\ at\ http://127\.0\.0\.1:[0-9]+/\#token=[0-9a-f]{64}$ ]]; then
    echo "Oracle Studio host produced an invalid launch URL" >&2
    exit 1
fi
launch_url=${launch_line#Oracle Studio is ready at }
unset launch_line

printf '%s\n' "$launch_url" | docker run --rm --interactive \
    --network host \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --shm-size 512m \
    --tmpfs /tmp:rw,nosuid,nodev,size=1g \
    --user "$(id -u):$(id -g)" \
    --env "ORACLE_STUDIO_VAULT_PATH=$run_dir/acceptance.oracle" \
    "$image"
unset launch_url

echo "PASS cleanup: the one-use browser session will be locked and removed"
