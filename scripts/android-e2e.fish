#!/usr/bin/env fish
# Stage 9.1 and 9.2 acceptance on a device or emulator: the gallery runs in the Android host from the
# bundle with real touch input, the bridge answers (and asks for a permission), a page from another
# origin gets no bridge, and the dev loop works through `adb reverse`.
#   scripts/android-e2e.fish         # needs a device (adb devices), the Android SDK and Node 22
set -g root (path resolve (status dirname)/..)
set -g work (mktemp -d)
set -g failed 0
set -g pkg dev.lorehaven.gallery
set -l port 3141
cd $root

test (count (adb devices | string match -r '\tdevice$')) -ge 1; or begin
    echo "no device: start an emulator (emulator -avd Pixel_8) or attach one"
    exit 1
end
function check
    if test "$argv[2]" = "$argv[3]"
        echo "PASS  $argv[1]"
    else
        echo "FAIL  $argv[1]: wanted $argv[3], got $argv[2]"
        set failed 1
    end
end
function forward_app # the app's WebView devtools socket, on localhost:9333
    set -l pid (adb shell pidof $pkg | string trim)
    adb forward --remove-all >/dev/null
    adb forward tcp:9333 localabstract:webview_devtools_remote_$pid >/dev/null
end
function eval_in_app
    forward_app
    node $root/scripts/webview-eval.mjs $argv
end
function tap_el # taps the element a JS expression returns, by a real touch
    forward_app
    adb shell input tap (string split ' ' (node $root/scripts/webview-eval.mjs --center $argv[1]))
    sleep 1
end
function native_result
    eval_in_app "document.querySelector('#native-result').textContent"
end

cargo build -p quench-tool -p quench-example-ui-web-server; or exit 1
cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app; or exit 1
mkdir -p $work/gallery
printf 'name = "gallery"\ntemplate = "web"\n' >$work/gallery/.quench.toml
QUENCH_EXPORT=$work/gallery/dist/www target/debug/quench-example-ui-web-server >/dev/null; or exit 1
cd $work/gallery
$root/target/debug/quench android init --package $pkg >/dev/null; or exit 1
cd android
./gradlew -q assembleDebug 2>&1 | string match -rv '^w:'; test $pipestatus[1] -eq 0; or exit 1
# A fresh install: the permission checks need a permission nobody has answered yet.
adb uninstall $pkg >/dev/null 2>&1
adb install app/build/outputs/apk/debug/app-debug.apk >/dev/null; or exit 1

# From the bundle, on the https origin the host serves.
adb shell am force-stop $pkg
adb shell am start -n $pkg/.MainActivity --es dev_url off >/dev/null
sleep 7
check "serves the bundle from its own origin" (eval_in_app "location.origin") '"https://app.quench.local"'
check "the stylesheets arrived" (eval_in_app "[...document.styleSheets].every(s=>s.cssRules.length>0)") true
check "the app took the page over" (eval_in_app "!!document.querySelector('.q-appbar')") true
adb shell input tap 204 852 # "Open sheet", by a real touch
sleep 1
check "a touch is handled" (eval_in_app "!!document.querySelector('.q-sheet')") true
tap_el "document.querySelector('.q-sheet button')" # Done: the sheet's backdrop would swallow later taps
check "and closes it again" (eval_in_app "!document.querySelector('.q-sheet')") true
eval_in_app "(history.pushState({},'','/x'), 1)" >/dev/null
adb shell input keyevent KEYCODE_BACK
sleep 1
check "the back button walks the page history" (eval_in_app "location.pathname") '"/"'

# The bridge: calls to the host's plugins, by real touches on the gallery's Native tab.
set -l native_tab "[...document.querySelectorAll('[role=tab]')].find(b=>b.textContent==='Native')"
tap_el $native_tab
tap_el "document.querySelector('#native-say')"
check "echo.say round-trips through the bridge" (native_result) '"say: hi"'
tap_el "document.querySelector('#native-device')"
check "a structured result arrives" (string match -rq '^"device: 3\d+ ' (native_result); and echo yes) yes
tap_el "document.querySelector('#native-fail')"
check "an error keeps the code the plugin chose" (native_result) '"fail: error nope"'
adb shell pm revoke $pkg android.permission.POST_NOTIFICATIONS
tap_el "document.querySelector('#native-notify')"
sleep 1
node $root/scripts/android-dialog.mjs permission_deny_button; or set failed 1
sleep 2
check "a denied permission is permission_denied" (native_result) '"notify: error permission_denied"'
adb shell pm revoke $pkg android.permission.POST_NOTIFICATIONS
tap_el "document.querySelector('#native-notify')"
sleep 1
node $root/scripts/android-dialog.mjs permission_allow_button; or set failed 1
sleep 2
check "an allowed permission is granted" (native_result) '"notify: true"'
tap_el "document.querySelector('#native-notify')"
sleep 1
check "a permission already granted needs no dialog" (native_result) '"notify: true"'

# The dev server, through adb reverse.
set -lx PORT $port
cd $root
target/debug/quench-example-ui-web-server >/dev/null 2>&1 &
set -l server $last_pid
sleep 2
adb reverse tcp:$port tcp:$port >/dev/null

# A page from an origin the host does not know gets no bridge, though it is the same app.
adb logcat -c
forward_app
node $root/scripts/webview-eval.mjs --navigate http://10.0.2.2:$port/gallery
sleep 6
check "the host refuses the origin" (adb logcat -d -s quench-bridge | string match -rq "refused origin http://10.0.2.2:$port"; and echo yes) yes
tap_el $native_tab
tap_el "document.querySelector('#native-say')"
sleep 4
check "so a call from that page is unavailable" (native_result) '"say: error unavailable"'
adb shell am start -n $pkg/.MainActivity --es dev_url http://127.0.0.1:$port/gallery >/dev/null
sleep 7
check "loads the dev server" (eval_in_app "location.href") '"http://127.0.0.1:'$port'/gallery"'
check "and the page works there" (eval_in_app "!!document.querySelector('.q-appbar')") true
adb shell am start -n $pkg/.MainActivity --es dev_url off >/dev/null
sleep 5
check "dev mode can be switched off" (eval_in_app "location.origin") '"https://app.quench.local"'

kill $server 2>/dev/null
adb reverse --remove-all >/dev/null
rm -rf $work
test $failed -eq 0; and echo "android e2e passed"
exit $failed
