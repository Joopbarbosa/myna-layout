# Session tests

These tests need a real COSMIC session (and, for most, a real Myna). They are
marked `#[ignore]`, so `cargo test` skips them. Run them by hand:

```sh
cargo test --test sessao -- --ignored
```

Name each test after the scenario it checks (for example `cen_novo_15_...`), so a
failure points straight to the scenario in the specification.
