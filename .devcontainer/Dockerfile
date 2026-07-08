# Parcello dev toolchain. Big image (~5 GB with the Flutter precache) but
# it covers the whole project: Rust workspace, Flutter Linux desktop,
# docs, deploy tooling. See devcontainer.json for what is deliberately
# left to CI.
FROM mcr.microsoft.com/devcontainers/base:bookworm

# Native build deps: Rust linker + libs, Flutter Linux desktop toolchain
# (clang/cmake/ninja/GTK), gstreamer for the client's sfx at runtime,
# sqlite for poking at history databases.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev \
        clang cmake ninja-build libgtk-3-dev liblzma-dev \
        libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
        gstreamer1.0-plugins-good \
        curl git unzip xz-utils zip libglu1-mesa \
        sqlite3 libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# typst compiles docs/architecture.typ.
RUN arch="$(uname -m)" && case "$arch" in \
        x86_64) t=x86_64-unknown-linux-musl ;; \
        aarch64) t=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported arch $arch" && exit 1 ;; \
    esac \
    && curl -fsSL "https://github.com/typst/typst/releases/latest/download/typst-$t.tar.xz" \
       | tar -xJ --strip-components=1 -C /usr/local/bin "typst-$t/typst"

USER vscode

# Rust: stable for daily work, 1.96 pinned for MSRV checks
# (`cargo +1.96 test --workspace --locked` must stay green).
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --component rustfmt --component clippy \
    && "$HOME/.cargo/bin/rustup" toolchain install 1.96 --component rustfmt --component clippy
ENV PATH=/home/vscode/.cargo/bin:$PATH

# Release hygiene tools (CLAUDE.md): binstall fetches prebuilt binaries
# instead of compiling for minutes.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
      https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y cargo-audit cargo-license

# Flutter (stable channel) with the Linux desktop artifacts warmed up.
RUN git clone --depth 1 -b stable https://github.com/flutter/flutter.git /home/vscode/flutter
ENV PATH=$PATH:/home/vscode/flutter/bin
RUN flutter config --no-analytics --enable-linux-desktop && flutter precache --linux
