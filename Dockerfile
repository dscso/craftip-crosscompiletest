FROM rust:1.67 as builder
RUN useradd -d /craftip -s /bin/bash -u 1001 craftip
WORKDIR /craftip

COPY Cargo.toml .
RUN chown -R craftip:craftip /craftip
USER craftip
# caching dependencies, let build fail on purpose
RUN cargo build --release || true
COPY src ./src
COPY Cargo.toml .
RUN cargo build --release --bin server


FROM debian:bullseye-slim
RUN useradd -d /craftip -s /bin/bash -u 1001 craftip
USER craftip
COPY --from=builder /craftip/target/release/server /usr/local/bin/server
CMD ["server"]
