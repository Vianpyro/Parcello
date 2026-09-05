# Parcello

**Parcello** is an open-source multiplayer board game built in Rust and Flutter.

It takes inspiration from games like Monopoly and Business Tour, but focuses on shorter, more dynamic matches. The server is authoritative, clients are kept thin, and the game rules are separated from networking and presentation.

## Features

* Multiplayer games over WebSockets
* Authoritative Rust game server
* Cross-platform Flutter client (Windows, Linux, macOS and Web)
* Guest and OIDC authentication
* Custom game content through TOML mods
* Ranked matchmaking with persistent ratings
* Reconnection support
* Spectator mode
* SQLite game history
* CLI client with bots for automated playtesting
* Docker deployment

## Architecture

The project is organized as a Cargo workspace:

| Crate               | Purpose                                 |
| ------------------- | --------------------------------------- |
| `parcello-engine`   | Pure game rules and state               |
| `parcello-mods`     | Data-driven game content and plugins    |
| `parcello-protocol` | Shared client/server protocol           |
| `parcello-server`   | WebSocket server, rooms and matchmaking |
| `parcello-cli`      | Terminal client and automated bots      |
| `clients/flutter`   | Desktop and web client                  |

The game engine has no networking, I/O or asynchronous code, making the core rules independently testable and reusable.

## Getting started

Requires **Rust 1.96+**.

```sh
cargo build --workspace
cargo test --workspace

cargo run -p parcello-server -- --insecure-guest
```

The Flutter client can then be run against the local server:

```sh
cd clients/flutter
flutter run -d windows
```

For headless testing, the CLI can host a game and fill the remaining seats with bots:

```sh
cargo run -p parcello-cli -- --name host --create
cargo run -p parcello-cli -- --name bot --join ABCDE --bot
```

## License

Parcello is licensed under either the [MIT License](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE).
