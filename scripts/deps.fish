#!/usr/bin/env fish
# Prints how many third-party crates the workspace pulls in, so Stage 0 progress
# is a number. Counts unique crate names (not name+version) across all targets
# and features - the same graph `cargo deny check bans` enforces.
#
#   scripts/deps.fish          the count
#   scripts/deps.fish -v       the crate names too

cd (dirname (status filename))/..

set -l names (cargo metadata --format-version 1 --all-features | jq -r \
    '.workspace_members as $m | .packages[] | select(.id as $i | $m | index($i) | not) | .name' | sort -u)

contains -- -v $argv; and printf '%s\n' $names
echo "third-party crates: "(count $names)
