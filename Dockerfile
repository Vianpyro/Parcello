# Standard multi-stage build; published to GHCR by .github/workflows/release.yml.

# Flutter Web client (ADR-0025): manual, checksummed SDK install from
# Google's official release CDN, not a third-party prebuilt image, to keep
# every stage traceable to an official source like the Rust/Debian ones
# below. Self-contained so `docker build .` needs nothing but this repo -
# release.yml's separate `web` job does not feed this stage.
FROM debian:bookworm-slim AS webbuild
ARG FLUTTER_VERSION=3.44.6
ARG FLUTTER_SHA256=a6320fd72e9a2690c08e2a6a70874a30cb120dee7c78f49d2c628bd7c9e20525
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git unzip xz-utils \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSLO "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_${FLUTTER_VERSION}-stable.tar.xz" \
    && echo "${FLUTTER_SHA256}  flutter_linux_${FLUTTER_VERSION}-stable.tar.xz" | sha256sum -c - \
    && tar -xJf "flutter_linux_${FLUTTER_VERSION}-stable.tar.xz" -C /opt \
    && rm "flutter_linux_${FLUTTER_VERSION}-stable.tar.xz" \
    && git config --global --add safe.directory /opt/flutter
ENV PATH="/opt/flutter/bin:${PATH}"
WORKDIR /app/clients/flutter
COPY clients/flutter .
RUN flutter build web --release

FROM rust:1.96-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked -p parcello-server

FROM debian:bookworm-slim
RUN useradd --system --home /srv/parcello parcello \
    && mkdir -p /srv/parcello/data \
    && chown -R parcello:parcello /srv/parcello
COPY --from=build /app/target/release/parcello-server /usr/local/bin/parcello-server
COPY --chown=parcello:parcello mods /srv/parcello/mods
COPY --chown=parcello:parcello --from=webbuild /app/clients/flutter/build/web /srv/parcello/web
WORKDIR /srv/parcello
USER parcello
EXPOSE 7878
ENTRYPOINT ["parcello-server"]
CMD ["--bind", "0.0.0.0:7878", "--mods-dir", "mods", "--web-dir", "web"]
