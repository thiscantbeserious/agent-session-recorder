FROM rust:latest AS base
WORKDIR /app

FROM base AS test
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY tests ./tests
RUN cargo test

FROM base AS builder
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release && \
    cp /app/target/release/asr /asr

# Final stage - just contains the binary
FROM builder AS final
CMD ["cat", "/asr"]
