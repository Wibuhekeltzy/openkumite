# Third-party notices

OpenKumite's own code is under either of [LICENSE-MIT](LICENSE-MIT) or [LICENSE-APACHE](LICENSE-APACHE), at your
option. It contains, or is derived from, the following third-party work. The Rust crates it depends on are under
their own licences (MIT, Apache-2.0, BSD, Zlib, MPL-2.0, ...), checked by `cargo deny` against `deny.toml`.

## matchbox_socket 0.14 (`vendor/matchbox_socket/`)

A patched copy of [matchbox_socket](https://github.com/johanhelsing/matchbox) by Johan Helsing and the matchbox
contributors, licensed MIT OR Apache-2.0; used here under the MIT licence ([its text](vendor/matchbox_socket/LICENSE-MIT)).
The patch replaces its TLS backend with rustls.

## No game data

The repository contains no game data. Everything game-specific is
read by the converter (kumite_converter, a separate project) from the user's own copies, and by OpenKumite from the
converter's output.
