FROM rust:latest AS base
WORKDIR /app

FROM base AS test
COPY Cargo.toml Cargo.lock* ./
COPY build.rs ./
COPY src ./src
COPY tests ./tests
RUN cargo test --features release

FROM base AS builder
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY build.rs ./
RUN cargo build --release --features release && \
    cp /app/target/release/agr /agr

# Final stage - just contains the binary
FROM builder AS final
CMD ["cat", "/agr"]
