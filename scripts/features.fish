#!/usr/bin/env fish
# Builds and lints every crate that has cargo features with the defaults off, and with each
# feature on its own, so a feature that only compiles next to the others is caught.
#
#   scripts/features.fish          the whole matrix
#   scripts/features.fish -t       also runs the tests of every combination
#
# Features that are implied by another (`jwt` brings `ed25519` and `rsa`) still get their own run.

cd (dirname (status filename))/..

set -l matrix \
    "quench-proto:server client ws multipart sse tls" \
    "quench-http:tls" \
    "quench-client:tls blocking" \
    "quench-pg:tls" \
    "quench-redis:tls" \
    "quench-cache:request-cache data-cache redis tls" \
    "quench-db:postgres tls" \
    "quench-auth:http redis postgres tls" \
    "quench-crypto:argon2 ed25519 rsa jwt totp aead" \
    "quench-cli:repl" \
    "quench-serde:toml yaml"

set -l failed 0
for entry in $matrix
    set -l parts (string split : $entry)
    set -l crate $parts[1]
    set -l combos "--no-default-features" defaults
    for feature in (string split ' ' $parts[2])
        set -a combos "--no-default-features --features $feature"
    end
    for combo in $combos
        printf '%-14s %s\n' $crate "$combo"
        set -l flags (test $combo = defaults; or string split ' ' -- $combo)
        cargo clippy -p $crate $flags --all-targets -q </dev/null; or set failed (math $failed + 1)
        if contains -- -t $argv
            cargo test -p $crate $flags -q </dev/null >/dev/null; or set failed (math $failed + 1)
        end
    end
end

test $failed -eq 0; and echo "all feature combinations pass"; or begin
    echo "$failed feature combination(s) failed"
    exit 1
end
