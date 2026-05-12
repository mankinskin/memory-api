# syntax=docker/dockerfile:1

ARG RUST_BASE_IMAGE=rust:1.91-bookworm
FROM ${RUST_BASE_IMAGE}

ARG RUSTUP_TOOLCHAIN=nightly-2025-10-09
ENV RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN}

RUN rustup toolchain install "${RUSTUP_TOOLCHAIN}" --profile minimal \
    && rustup default "${RUSTUP_TOOLCHAIN}"

WORKDIR /workspace
COPY . /workspace

ENTRYPOINT ["bash", "tools/install-validation/run-in-container.sh"]
