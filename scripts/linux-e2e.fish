#!/usr/bin/env fish
# Stage 10 acceptance on a desktop session: a project goes from `quench linux init` and `quench build linux`
# to a window that serves its bundle offline, and the gallery runs as a Linux binary from the same UI code with
# the bridge answering through the real quench.js. It opens windows on your display for a few seconds each.
#   scripts/linux-e2e.fish           # needs a display, webkit2gtk-4.1, node 22, dpkg-deb (for the .deb check)
set -g root (path resolve (status dirname)/..)
set -g work (mktemp -d)
set -g failed 0
set -g sock $work/control.sock
cd $root

function check
    if test "$argv[2]" = "$argv[3]"
        echo "PASS  $argv[1]"
    else
        echo "FAIL  $argv[1]: wanted $argv[3], got $argv[2]"
        set failed 1
    end
end
function app_eval
    node $root/scripts/linux-eval.mjs $sock $argv 2>&1
end
function wait_for_app # until the page answers
    for i in (seq 1 100)
        test -S $sock; and test (app_eval "1") = 1; and return 0
        sleep 0.2
    end
    echo "FAIL  the window did not come up"
    set failed 1
    return 1
end
function stop_app
    set -q app_pid; and kill $app_pid 2>/dev/null
    set -e app_pid
    rm -f $sock
end

cargo build -p quench-tool -p quench-example-ui-web-server; or exit 1
cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app; or exit 1
set -g quench $root/target/debug/quench
test -n "$DISPLAY$WAYLAND_DISPLAY"; or begin
    echo "no display: run this inside a desktop session"
    exit 1
end

# --- A project made by `quench new`: init, build, run from an empty directory, package.
set -lx CARGO_TARGET_DIR $root/target/e2e
cd $work
$quench new demo --quench-path $root >/dev/null; or exit 1
cd demo
$quench linux init --id dev.lorehaven.demo --quench-path $root; or exit 1
$quench build linux; or exit 1
mkdir ../empty
cp dist/demo-linux ../empty/
cd ../empty
env QUENCH_DESKTOP_CONTROL=$sock ./demo-linux >/dev/null 2>&1 &
set -g app_pid $last_pid
if wait_for_app
    check "the project's page is served from the app origin" (app_eval "location.origin") '"quench://app"'
    check "the app took the page over" (app_eval "document.querySelector('h1').textContent") '"demo"'
    # The counter works only if the module, embedded in the binary, is running.
    app_eval "(document.querySelector('#up').click(), 1)" >/dev/null
    sleep 1
    check "the module embedded in the binary is running" (app_eval "document.querySelector('#count').textContent") '"1"' 
end
stop_app
cd $work/demo
$quench build deb; or exit 1
set -l deb (ls dist/*.deb)
check "the package is named for the project" (basename $deb) demo_0.1.0_amd64.deb
check "dpkg-deb reads it" (dpkg-deb -f $deb Package) demo
check "it installs the binary" (dpkg-deb -c $deb | string match -rq 'usr/bin/demo$'; and echo yes) yes
check "and the menu entry" (dpkg-deb -c $deb | string match -rq 'applications/dev.lorehaven.demo.desktop$'; and echo yes) yes
cd $root
set -e CARGO_TARGET_DIR

# --- The gallery, as a Linux binary from the same UI code.
mkdir -p $work/gallery
printf 'name = "gallery"\ntemplate = "web"\n' >$work/gallery/.quench.toml
QUENCH_EXPORT=$work/gallery/dist/www target/debug/quench-example-ui-web-server >/dev/null; or exit 1
cd $work/gallery
$quench linux init --id dev.lorehaven.gallery --quench-path $root >/dev/null; or exit 1
cargo build --manifest-path desktop/Cargo.toml 2>&1 | string match -rv '^\s*(Compiling|Finished|Blocking)'; test $pipestatus[1] -eq 0; or exit 1
env QUENCH_DESKTOP_CONTROL=$sock desktop/target/debug/gallery-desktop >/dev/null 2>&1 &
set -g app_pid $last_pid
if wait_for_app
    sleep 2
    check "serves the bundle from its own origin" (app_eval "location.origin") '"quench://app"'
    check "the stylesheets arrived" (app_eval "[...document.styleSheets].every(s=>s.cssRules.length>0)") true
    check "the app took the page over" (app_eval "!!document.querySelector('.q-appbar')") true
    # The sheet opens from a button of the Components tab.
    app_eval "([...document.querySelectorAll('button')].find(b => b.textContent.includes('Open sheet')).click(), 1)" >/dev/null
    sleep 1
    check "a click is handled" (app_eval "!!document.querySelector('.q-sheet')") true
    app_eval "(document.querySelector('.q-sheet button').click(), 1)" >/dev/null
    sleep 1
    check "and closes it again" (app_eval "!document.querySelector('.q-sheet')") true
    app_eval "(history.pushState({}, '', '/x'), history.back(), 1)" >/dev/null
    sleep 1
    check "history walks back" (app_eval "location.pathname") '"/"'
    # The bridge, through the real quench.js: the gallery's Native tab.
    app_eval "([...document.querySelectorAll('[role=tab]')].find(b => b.textContent === 'Native').click(), 1)" >/dev/null
    sleep 1
    app_eval "(document.querySelector('#native-say').click(), 1)" >/dev/null
    sleep 1
    check "echo.say round-trips through the bridge" (app_eval "document.querySelector('#native-result').textContent") '"say: hi"'
    app_eval "(document.querySelector('#native-device').click(), 1)" >/dev/null
    sleep 1
    check "a structured result arrives" (string match -rq '^"device: 0 ' (app_eval "document.querySelector('#native-result').textContent"); and echo yes) yes
    app_eval "(document.querySelector('#native-fail').click(), 1)" >/dev/null
    sleep 1
    check "an error keeps the code the plugin chose" (app_eval "document.querySelector('#native-result').textContent") '"fail: error nope"'
    app_eval "(document.querySelector('#native-notify').click(), 1)" >/dev/null
    sleep 1
    check "the permission call is granted" (app_eval "document.querySelector('#native-result').textContent") '"notify: true"'
end
stop_app
rm -rf $work
test $failed -eq 0; and echo "linux e2e passed"
exit $failed
