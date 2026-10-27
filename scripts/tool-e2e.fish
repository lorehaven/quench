#!/usr/bin/env fish
# Stage 8 acceptance, from the command line only: a new project goes from `quench new` to a release
# binary that serves the app from an empty directory.
#   scripts/tool-e2e.fish            # needs node and chromium (for `quench test --browser`)
set -l root (path resolve (status dirname)/..)
set -l work (mktemp -d)
set -l port 3131
cd $root
cargo build -p quench-tool; or exit 1
set -l quench $root/target/debug/quench

# Share the compiled quench crates with the workspace, so the run takes seconds, not minutes.
set -lx CARGO_TARGET_DIR $root/target/e2e

cd $work
$quench new demo --quench-path $root; or exit 1
cd demo
$quench test --browser; or exit 1
$quench build --release; or exit 1

mkdir ../empty
cp dist/demo-server ../empty/
cd ../empty
PORT=$port ./demo-server >/dev/null 2>&1 &
set -l server $last_pid
sleep 1
set -l page (curl -s http://127.0.0.1:$port/)
set -l wasm (string match -r 'wasm=([^"]+)' -- $page)[2]
set -l failed 0
string match -q '*<h1>demo</h1>*' -- $page; or begin; echo "FAIL  page is not rendered"; set failed 1; end
string match -q '*dev.js*' -- $page; and begin; echo "FAIL  release page has the dev script"; set failed 1; end
test (curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$port$wasm) = 200; or begin; echo "FAIL  module not served"; set failed 1; end
test (curl -s -X POST -H 'content-type: application/json' -H 'x-quench-rpc: 1' -d '{"name":"Ola"}' http://127.0.0.1:$port/_fn/greet) = '{"text":"Hello, Ola!"}'; or begin; echo "FAIL  server function"; set failed 1; end
set -l icon (curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:$port/favicon.ico)
test $icon = 204; or begin; echo "FAIL  favicon is $icon"; set failed 1; end
kill $server 2>/dev/null
rm -rf $work
test $failed -eq 0; and echo "tool e2e passed"
exit $failed
