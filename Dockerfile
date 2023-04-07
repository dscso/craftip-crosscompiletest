FROM rust:1.67 as builder
WORKDIR /craftip
COPY Cargo.toml .
RUN echo "\n\n[[bin]]\nname = \"dependencies\"\npath = \"src/dependencies.rs\"" >> Cargo.toml
RUN mkdir src && echo "fn main() {panic!(\"This should never run! Check Docker\");}" > src/dependencies.rs
RUN cargo build --release --bin dependencies
RUN rm -r ./src
RUN rm ./Cargo.toml
COPY . .
RUN cargo build --release --bin server



FROM debian:bullseye-slim
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /craftip/target/release/server /usr/local/bin/server
CMD ["server"]
