FROM rust:1.67

RUN apt-get update && apt-get upgrade -y
RUN apt-get install -y \
    build-essential \
    cmake \
    git \
    curl \
    wget
RUN apt-get install -y \
        clang \
        gcc \
        g++ \
        zlib1g-dev \
        libmpc-dev \
        libmpfr-dev \
        libgmp-dev \
        libxml2-dev \
        libssl-dev clang zlib1g-dev

# https://wapl.es/rust/2019/02/17/rust-cross-compile-linux-to-macos.html
RUN git clone https://github.com/tpoechtrager/osxcross.git
WORKDIR /osxcross
RUN wget -nc https://github.com/joseluisq/macosx-sdks/releases/download/12.3/MacOSX12.3.sdk.tar.xz
RUN mv MacOSX12.3.sdk.tar.xz tarballs/
RUN UNATTENDED=yes OSX_VERSION_MIN=12.3 ./build.sh
RUN rustup target add x86_64-apple-darwin
RUN rustup target add aarch64-apple-darwin
WORKDIR /tmp
# cargo init takes super long so caching it!
RUN cargo search openssl
WORKDIR /root/.cargo
RUN echo "[target.x86_64-apple-darwin]" > /root/.cargo/config
RUN echo "linker = \"x86_64-apple-darwin21.4-clang\"" >> /root/.cargo/config
RUN echo "ar = \"x86_64-apple-darwin21.4-ar\"" >> /root/.cargo/config
RUN echo "[target.aarch64-apple-darwin]" >> /root/.cargo/config
RUN echo "linker = \"aarch64-apple-darwin21.4-clang\"" >> /root/.cargo/config
RUN echo "ar = \"aarch64-apple-darwin21.4-ar\"" >> /root/.cargo/config

RUN apt install -y g++-mingw-w64-x86-64
RUN rustup target add x86_64-pc-windows-gnu
RUN rustup toolchain install stable-x86_64-pc-windows-gnu

WORKDIR /build
RUN echo '#!/bin/bash' > /entrypoint.sh
RUN echo 'PATH="/osxcross/target/bin:$PATH"' >> /entrypoint.sh
RUN echo 'exec "$@"' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD sleep infinity