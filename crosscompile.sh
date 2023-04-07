#!/bin/bash

echo "Building magic Docker container!"
#docker build -t dscso/rust-crosscompiler:latest https://github.com/dscso/rust-crosscompiler.git#main
docker pull dscso/rust-crosscompiler:latest
echo "Stopping old Dockercontainer"
docker stop crosscompiler
echo "Removing old Dockercontainer"
docker rm crosscompiler
echo "Starting new Dockercontainer, with volumes..."
docker run -v $(pwd)/target-cross:/build/target                               \
           -v $(pwd)/Cargo.toml:/build/Cargo.toml:ro                          \
           -v $(pwd)/src:/build/src:ro                                        \
           --name crosscompiler    -d                                         \
           dscso/rust-crosscompiler:latest                                    \
           sleep infinity

export runincontainer="docker exec -it crosscompiler /bin/bash -c "

echo "Container running... Building application... x86_64-apple-darwin"
$runincontainer "source /entrypoint.sh && cargo build --target=x86_64-apple-darwin --bin client-gui --features gui --config /root/.cargo/config" # --release

echo "Container running... Building application... aarch64-apple-darwin"
$runincontainer "source /entrypoint.sh && cargo build --target=aarch64-apple-darwin --bin client-gui --features gui --config /root/.cargo/config" # --release

echo "Container running... Building application... x86_64-pc-windows-gnu"
$runincontainer "source /entrypoint.sh && cargo build --target=x86_64-pc-windows-gnu --bin client-gui --features gui --config /root/.cargo/config" # --release

docker stop crosscompiler
docker rm crosscompiler
