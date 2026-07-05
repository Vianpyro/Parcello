# Standard multi-stage build; published to GHCR by .github/workflows/release.yml.
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
WORKDIR /srv/parcello
USER parcello
EXPOSE 7878
ENTRYPOINT ["parcello-server"]
CMD ["--bind", "0.0.0.0:7878", "--mods-dir", "mods"]
