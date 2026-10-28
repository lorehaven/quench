#!/usr/bin/env fish
# The native renderer on Android, on a device or emulator: a new project with the `surface` plugin, built with the
# NDK, installed, started as SurfaceActivity, and used with touches and keys while the accessibility tree is read
# back the way TalkBack reads it (uiautomator). Screenshots are of the device only.
#   scripts/android-surface-e2e.fish        # needs a device, the SDK and NDK
set -g root (path resolve (status dirname)/..)
set -g pkg dev.lorehaven.surfacetest
set -g work (mktemp -d)
set -g failed 0
set -l abi (adb shell getprop ro.product.cpu.abi | string trim)
test (count (adb devices | string match -r '\tdevice$')) -ge 1; or begin
    echo "no device: start an emulator (cd /opt/android-sdk/emulator; ./emulator -avd Pixel_8 -no-window) or attach one"
    exit 1
end
function check # name, got, wanted
    if test "$argv[2]" = "$argv[3]"
        echo "PASS  $argv[1]"
    else
        echo "FAIL  $argv[1]: wanted $argv[3], got $argv[2]"
        set -g failed 1
    end
end
function tree # the accessibility tree as `class text` lines
    adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1
    adb exec-out cat /sdcard/ui.xml | python3 -c "
import sys,re
for m in re.finditer(r'<node [^>]*>', sys.stdin.read()):
    t=m.group(0); g=lambda k:(re.search(k+'=\"([^\"]*)\"',t) or [0,''])[1]
    print(g('class').split('.')[-1], g('text'), 'checked='+g('checked'), g('bounds'))"
end
function center # text: the middle of the node with that text, as `x y`
    tree | string match -r -- "^\S+ $argv[1] .*\[(\d+),(\d+)\]\[(\d+),(\d+)\]" | python3 -c "
import sys,re
m=re.search(r'\[(\d+),(\d+)\]\[(\d+),(\d+)\]', sys.stdin.read()); a=list(map(int,m.groups())); print((a[0]+a[2])//2, (a[1]+a[3])//2)"
end

cargo build -p quench-tool; or exit 1
set -l quench $root/target/debug/quench
set -lx CARGO_TARGET_DIR $root/target/e2e
cd $work
$quench new surfacetest --template web --quench-path $root >/dev/null; or exit 1
cd surfacetest
$quench android init --package $pkg >/dev/null; or exit 1
$quench plugin add surface >/dev/null; or exit 1
$quench android launcher surface >/dev/null; or exit 1
$quench android build-native --abi $abi; or exit 1
cd android
./gradlew -q assembleDebug 2>&1 | string match -rv '^w:'; test $pipestatus[1] -eq 0; or exit 1
adb uninstall $pkg >/dev/null 2>&1
adb install app/build/outputs/apk/debug/app-debug.apk >/dev/null; or exit 1
# Started the way the launcher starts it: the surface is the launcher activity.
adb shell monkey -p $pkg -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 6

check "the program runs as a process of its own" (adb shell "ps -A | grep -c libquench_surface" | string trim) 1
check "the accessibility tree has the page" (tree | string match -r 'Button Press' | count) 1
check "a field is an EditText" (tree | string match -r '^EditText' | count) 1
set -l press (center Press)
adb shell input tap $press
adb shell input tap $press
sleep 1
check "two taps press the button twice" (tree | string match -r 'pressed 2 times' | count) 1
adb shell input tap (center "Your name" | string split ' ')
sleep 1
adb shell input text hello
sleep 1
check "typing reaches the field" (tree | string match -r '^EditText hello' | count) 1
adb shell input tap (center "I agree" | string split ' ')
sleep 1
check "a tap checks the box" (tree | string match -r '^CheckBox I agree checked=true' | count) 1
adb shell input tap (center "Ask the host" | string split ' ')
sleep 2
check "a plugin of the host answers the program" (tree | string match -r '^\S+ host: (?!not asked|failed)\S' | count) 1
adb exec-out screencap -p >$work/screen.png
echo "screenshot: $work/screen.png"
exit $failed
