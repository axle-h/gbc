# gbc

A Game Boy, Game Boy Color and Super Game Boy emulator in Rust, published on crates.io as
[`gbc`](https://crates.io/crates/gbc). [`gbc/README.md`](gbc/README.md) is the crate's own page:
what it emulates, what it passes and how to drive it.

Its main user is [axle-h/poke-agent](https://github.com/axle-h/poke-agent), which has an LLM play
Pokémon Red on it.

## Tests

```shell
cargo test --release                          # every test ROM, in seconds
cargo test --release --features slow-tests    # adds the throughput benches
```

The test ROMs are under `gbc/src/roms/` and are left out of the published crate. All but
`workload/` are third-party; `workload/` is our own, built by its `build.sh`.

## Releasing

Bump `version` in `gbc/Cargo.toml`, `cargo build` so `Cargo.lock` follows, commit and push, then
push a `gbc-v<version>` tag. `publish-gbc.yml` checks the tag against the version, runs the tests
and publishes through crates.io Trusted Publishing.

## Licence

MIT OR Apache-2.0, with both texts in `gbc/`.
