#!/usr/bin/env fish
# Builds the example app for the browser and runs the browser checks against it.
#   scripts/ui-web-test.fish          # needs chromium and node
set -l root (path resolve (status dirname)/..)
cd $root
set -l port 3101

node --check quench-ui-web/js/quench.js; and node --check quench-ui-web/tests/browser.mjs; or exit 1 # a syntax error stops the app before anything else shows it
cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app; or exit 1
cargo build -p quench-example-ui-web-server; or exit 1

PORT=$port target/debug/quench-example-ui-web-server >/dev/null 2>&1 &
set -l server $last_pid
sleep 1
node quench-ui-web/tests/browser.mjs http://127.0.0.1:$port
set -l result $status
kill $server 2>/dev/null
exit $result
