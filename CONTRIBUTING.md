# Contributing

## Tests

```shell
cargo test --release                          # every test ROM, in seconds
cargo test --release --features slow-tests    # adds the throughput benches
```

The test ROMs are under `src/roms/` and are left out of the published crate. All but `workload/` are
third-party; `workload/` is our own, built by its `build.sh`.

The heavy user is [axle-h/poke-agent](https://github.com/axle-h/poke-agent), which plays Pokémon Red
on `gbc`, and its pre-push gates are the real regression suite.

## Releasing

Bump `version` in `Cargo.toml`, `cargo build` so `Cargo.lock` follows, commit and push, then push a
`gbc-v<version>` tag. `publish-gbc.yml` checks the tag against the version, runs the tests and
publishes through crates.io Trusted Publishing.
