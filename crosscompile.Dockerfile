FROM debian:bullseye

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

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc


# https://wapl.es/rust/2019/02/17/rust-cross-compile-linux-to-macos.html
RUN git clone https://github.com/tpoechtrager/osxcross.git
WORKDIR /osxcross
RUN wget -nc https://github.com/joseluisq/macosx-sdks/releases/download/12.3/MacOSX12.3.sdk.tar.xz
RUN mv MacOSX12.3.sdk.tar.xz tarballs/
RUN UNATTENDED=yes OSX_VERSION_MIN=12.3 ./build.sh
RUN /root/.cargo/bin/rustup target add x86_64-apple-darwin
RUN /root/.cargo/bin/rustup target add aarch64-apple-darwin
#RUN /osxcross/target/bin/x86_64-apple-darwin21.4-ar
#RUN /osxcross/target/bin/x86_64-apple-darwin21.4-clang --version
WORKDIR /tmp
# cargo init takes super long so caching it!
RUN /root/.cargo/bin/cargo search openssl
RUN echo "[target.x86_64-apple-darwin]" > /root/.cargo/config
RUN echo "linker = \"x86_64-apple-darwin21.4-clang\"" >> /root/.cargo/config
RUN echo "ar = \"x86_64-apple-darwin21.4-ar\"" >> /root/.cargo/config
RUN echo "[target.aarch64-apple-darwin]" >> /root/.cargo/config
RUN echo "linker = \"aarch64-apple-darwin21.4-clang\"" >> /root/.cargo/config
RUN echo "ar = \"aarch64-apple-darwin21.4-ar\"" >> /root/.cargo/config
RUN cat /root/.cargo/config
#RUN echo ". \"/root/.cargo/env\"" >> /root/.bashrc
WORKDIR /build
COPY . .

# prebuild dependencies
#COPY scripts/crosscompile.sh .

#RUN bash crosscompile.sh

RUN PATH="/osxcross/target/bin:$PATH" && /root/.cargo/bin/cargo build --target=x86_64-apple-darwin --bin client-gui --features gui # --release
RUN PATH="/osxcross/target/bin:$PATH" && /root/.cargo/bin/cargo build --target=aarch64-apple-darwin --bin client-gui --features gui # --release

CMD sleep infinity