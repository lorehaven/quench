# quench-fuzz

Coverage-guided fuzzing (libFuzzer via `cargo-fuzz`) of everything that parses attacker-controlled
bytes. Separate workspace so `libfuzzer-sys` stays out of the main dependency graph. Needs nightly.

    cargo install cargo-fuzz
    cargo +nightly fuzz run request_head corpus/request_head seeds/request_head -- -max_total_time=300

Targets: `request_head` `response_head` `chunked_decoder` `ws_frame` `uri` `cookie`
`multipart_boundary` `jwt`. They assert properties, not just "no panic": no CR/LF/NUL in accepted
heads, CL+TE never framed, chunked decoding independent of read splits, WebSocket caps honoured,
no input ever verifies as a JWT. `scripts/security.fish --fuzz` runs all of them.

A crash lands in `artifacts/<target>/`; replay with `cargo +nightly fuzz run <target> <file>` and
copy a minimised reproducer into `quench-proto/tests/` as a regression test.

The nightly run is `cargo test -p quench-security --features gate --test fuzz` (one test case per target,
`QUENCH_FUZZ_SECONDS` each, 3x for `request_head`, `chunked_decoder` and `jwt`), so its results are part of the
security report conveyor shows. `scripts/security.fish --fuzz` runs the same locally.
