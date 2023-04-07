#!/bin/bash

echo "Building magic Docker container!"
docker build -t craftip-crosscompiler ./crosscompiler
echo "Stopping old Dockercontainer"
docker stop craftip-crosscompiler
echo "Removing old Dockercontainer"
docker rm craftip-crosscompiler
echo "Starting new Dockercontainer, with volumes..."
docker run -v $(pwd)/target-cross:/build/target                               \
           -v $(pwd)/Cargo.toml:/build/Cargo.toml:ro                          \
           -v $(pwd)/src:/build/src:ro                                        \
           --name craftip-crosscompiler craftip-crosscompiler                 \
           -d                                                                 \
           sleep infinity

alias runincontainer="docker exec -it craftip-crosscompiler"
echo "Container running... Building application... x86_64-apple-darwin"
runincontainer cargo build --target=x86_64-apple-darwin --bin client-gui --features gui --config /root/.cargo/config # --release

echo "Container running... Building application... aarch64-apple-darwin"
runincontainer cargo build --target=aarch64-apple-darwin --bin client-gui --features gui --config /root/.cargo/config # --release

echo "Container running... Building application... x86_64-pc-windows-gnu"
runincontainer cargo build --target=x86_64-pc-windows-gnu --bin client-gui --features gui --config /root/.cargo/config # --release

