ARG BUILD_IMAGE=rust:slim-bookworm
ARG RUNTIME_IMAGE=ubuntu:noble

FROM ${BUILD_IMAGE} AS builder
WORKDIR /app
ENV DEBIAN_FRONTEND=noninteractive
ENV PNPM_VERSION=11 NODE_VERSION=22
RUN apt-get update && apt-get install -y \
    curl clang pkg-config libssl-dev ca-certificates gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y nodejs \
    && corepack enable \
    && corepack prepare pnpm@${PNPM_VERSION} --activate \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

FROM ${RUNTIME_IMAGE} AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/rmf2_task_orchestrator /app/rmf2_task_orchestrator
COPY config.toml /app/config.toml
COPY diagrams /app/diagrams
EXPOSE 2727
ENTRYPOINT ["/app/rmf2_task_orchestrator"]
