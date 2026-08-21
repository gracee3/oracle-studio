#!/usr/bin/env bash
set -euo pipefail

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
product_image="oracle-studio-browser-local:acceptance"
browser_image="oracle-studio-browser-acceptance:152.0.7977.54"
container_name="oracle-studio-browser-local-acceptance-$$"

cleanup() {
    docker rm --force "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

if ss -H -ltn 'sport = :8080' | grep -q .; then
    echo "127.0.0.1:8080 is already in use; acceptance will not disturb it" >&2
    exit 1
fi

docker build --target acceptance-runtime --tag "$product_image" "$repository"
docker build --tag "$browser_image" "$repository/tools/browser-acceptance"
docker run --detach \
    --name "$container_name" \
    --publish 127.0.0.1:8080:8080 \
    --read-only \
    --tmpfs /tmp:rw,nosuid,nodev,size=16m \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    "$product_image" >/dev/null

for _ in $(seq 1 100); do
    if curl --fail --silent --show-error http://127.0.0.1:8080/ >/dev/null; then
        break
    fi
    sleep 0.2
done
curl --fail --silent --show-error http://127.0.0.1:8080/ >/dev/null

docker run --rm \
    --network host \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --shm-size 512m \
    --tmpfs /tmp:rw,nosuid,nodev,size=1g \
    "$browser_image"
