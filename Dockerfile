FROM rust:latest AS builder
WORKDIR /app

## Cache build dependencies
COPY Cargo.toml Cargo.lock .
# Create a fake main.rs file to build, so Docker can cache the dependencies.
RUN mkdir -p ./src && echo 'fn main() {}' > ./src/main.rs
RUN cargo fetch
RUN RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-gnu
RUN rm -rf ./src

## Actual build
COPY src/ ./src/
# The last modified time of main.rs needs to be updated manually for cargo to rebuild it.
RUN touch -a -m ./src/main.rs
RUN RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-gnu


FROM gcr.io/distroless/base
WORKDIR /
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/recast /
CMD ["/recast"]
