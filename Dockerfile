# Standard multi-stage build; published to GHCR by .github/workflows/release.yml.
FROM rust:1.75-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked -p parcello-server

FROM debian:bookworm-slim
RUN useradd --system --home /srv/parcello parcello
COPY --from=build /app/target/release/parcello-server /usr/local/bin/parcello-server
COPY mods /srv/parcello/mods
WORKDIR /srv/parcello
USER parcello
EXPOSE 7878
# Mount a volume on /srv/parcello/data and add: --history data/parcello.db
ENTRYPOINT ["parcello-server"]
CMD ["--bind", "0.0.0.0:7878", "--mods-dir", "mods"]
