# Protocol fuzz targets

Developer tooling only, never installed. With cargo-fuzz and a nightly toolchain:

```sh
cargo fuzz run decoder -- -max_len=1048577 -max_total_time=600
cargo fuzz run merge_patch -- -max_len=1048576 -max_total_time=600
```

The decoder checks size limits, malformed JSON, duplicate keys and arbitrary
chunk boundaries. Merge patch checks the diff/apply round trip including the
snapshot fallback for unrepresentable null members. Ordinary Cargo property and
transcript tests run on stable. Nightly scheduling and the remaining targets are
deferred to the foundation rails step; these commands are not a claim of a soak.
