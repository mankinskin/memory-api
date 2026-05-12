#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
memory_api_root=$(cd -- "$script_dir/../.." && pwd)
toolchain=${RUSTUP_TOOLCHAIN:-$(<"$script_dir/toolchain.txt")}
base_image=${RUST_BASE_IMAGE:-rust:1.91-bookworm}
tag=${DOCKER_IMAGE_TAG:-memory-api-install-validation:${toolchain}}

echo "[docker-build] Building $tag with $toolchain"
docker build \
    --build-arg "RUST_BASE_IMAGE=$base_image" \
    --build-arg "RUSTUP_TOOLCHAIN=$toolchain" \
    -f "$script_dir/Dockerfile" \
    -t "$tag" \
    "$memory_api_root"

echo "[docker-run] Running $tag"
docker run --rm "$tag"
