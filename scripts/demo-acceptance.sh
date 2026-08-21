#!/usr/bin/env bash
set -euo pipefail

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
product_image="oracle-studio-demo-local:acceptance"
browser_image="oracle-studio-browser-acceptance:152.0.7977.54"
container_name="oracle-studio-demo-local-acceptance-$$"
acceptance_port="${ORACLE_ACCEPTANCE_PORT:-8080}"

case "$acceptance_port" in
    ''|*[!0-9]*) echo "ORACLE_ACCEPTANCE_PORT must be a numeric TCP port" >&2; exit 2 ;;
esac

cleanup() {
    docker rm --force "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

if ss -H -ltn "sport = :$acceptance_port" | grep -q .; then
    echo "127.0.0.1:$acceptance_port is already in use; demo acceptance will not disturb it" >&2
    exit 1
fi

docker build --target demo-runtime --tag "$product_image" "$repository"
docker build --tag "$browser_image" "$repository/tools/browser-acceptance"
docker run --detach \
    --name "$container_name" \
    --publish "127.0.0.1:$acceptance_port:8080" \
    --read-only \
    --tmpfs /tmp:rw,nosuid,nodev,size=16m \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    "$product_image" >/dev/null

for _ in $(seq 1 100); do
    if curl --fail --silent --show-error "http://127.0.0.1:$acceptance_port/" >/dev/null; then
        break
    fi
    sleep 0.2
done
curl --fail --silent --show-error "http://127.0.0.1:$acceptance_port/" >/dev/null

docker run --rm \
    --network host \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --shm-size 512m \
    --tmpfs /tmp:rw,nosuid,nodev,size=1g \
    --env "ORACLE_STUDIO_URL=http://127.0.0.1:$acceptance_port/" \
    --entrypoint python3 \
    "$browser_image" /opt/oracle-studio/demo_acceptance.py
