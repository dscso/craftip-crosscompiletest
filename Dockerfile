FROM rust:1.67 as builder
RUN adduser --no-create-home --disabled-password builder
RUN mkdir /craftip
WORKDIR /craftip

COPY Cargo.toml .
RUN chown -R builder:builder /craftip
USER builder
# caching dependencies, let build fail on purpose
RUN cargo build --release || true
COPY src ./src
COPY Cargo.toml .
RUN cargo build --release --bin server


FROM debian:bullseye-slim
RUN adduser --no-create-home --disabled-password craftip
USER craftip
COPY --from=builder /craftip/target/release/server /usr/local/bin/server
CMD ["server"]
