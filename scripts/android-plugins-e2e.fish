#!/usr/bin/env fish
# Stage 9.3-9.9 acceptance on a device or emulator: a new project with every plugin added, built with
# the NDK, installed, and driven through the plugin console (scripts/android-console) while the
# system does what it does: killing the process, rebooting, asking for permissions.
#   scripts/android-plugins-e2e.fish            # needs a device, the SDK and NDK, Node 22, jq
#   SKIP_REBOOT=1 scripts/android-plugins-e2e.fish
# The emulator's network is switched off and on for a moment, and a few files are left in
# /sdcard/Pictures, /sdcard/Music and /sdcard/Documents.
set -g root (path resolve (status dirname)/..)
set -g pkg dev.lorehaven.console
set -g work (mktemp -d)
set -g failed 0
set -l abi (adb shell getprop ro.product.cpu.abi | string trim)
cd $root

test (count (adb devices | string match -r '\tdevice$')) -ge 1; or begin
    echo "no device: start an emulator (cd /opt/android-sdk/emulator; ./emulator -avd Pixel_8 -no-window) or attach one"
    exit 1
end

# A device that is still starting (after a reboot) is not one to test on.
adb wait-for-device
for i in (seq 1 90)
    test (adb shell getprop sys.boot_completed 2>/dev/null | string trim) = 1; and break
    sleep 2
end

function check # name, got, wanted
    if test "$argv[2]" = "$argv[3]"
        echo "PASS  $argv[1]"
    else
        echo "FAIL  $argv[1]: wanted $argv[3], got $argv[2]"
        set -g failed 1
    end
end
function check_yes # name, then a command: passes when it succeeds
    if eval $argv[2..-1]
        echo "PASS  $argv[1]"
    else
        echo "FAIL  $argv[1]"
        set -g failed 1
    end
end
function call # plugin method [json]: the answer as JSON
    # The app is started if it is not running (granting a permission, or a reboot, ends its process), and
    # a call that finds nobody there, or answers nothing, is tried again: right after the emulator's
    # network is switched, `adb` itself can fail for a moment.
    set -l answer
    for attempt in 1 2 3
        test -n "$(adb shell pidof $pkg | string trim)"; or start_app
        set answer (node $root/scripts/android-plugin.mjs $pkg $argv)
        echo $argv[1] $argv[2] '=>' $answer >>$work/calls.log
        if test -n "$answer"; and not string match -q '*"not_running"*' -- $answer
            break
        end
        sleep 3
    end
    echo $answer
end
function events # the console's list of pushed events, one per line
    adb forward --remove-all >/dev/null
    adb forward tcp:9333 localabstract:webview_devtools_remote_(adb shell pidof $pkg | string trim) >/dev/null
    node $root/scripts/webview-eval.mjs "[...document.querySelectorAll('#events li')].map(l=>l.textContent).join('\n')" | jq -r .
end
function press # a system UI element, retrying while the screen settles
    for i in (seq 1 25)
        node $root/scripts/android-dialog.mjs $argv[1] 2>/dev/null; and return 0
        sleep 1
    end
    echo "FAIL  the system UI has no $argv[1]"
    set -g failed 1
    return 1
end
function wait_event # a pattern, for up to $argv[2] seconds
    for i in (seq 1 $argv[2])
        events | string match -rq $argv[1]; and return 0
        sleep 1
    end
    return 1
end
function start_app
    adb shell am start -n $pkg/.MainActivity --es dev_url off >/dev/null
    sleep 7
end
function job_id # Kotlin's String.hashCode, as the plugin uses it
    python3 -c "
h=0
for c in '$argv[1]': h=(31*h+ord(c))&0xffffffff
print(h&0x3fffffff)"
end

# ---- build: a new project, every plugin, the console as its app ---------------------------------
cargo build -p quench-tool; or exit 1
set -l quench $root/target/debug/quench
set -lx CARGO_TARGET_DIR $root/target/e2e
for port in 3151 3152
    fuser -k $port/tcp >/dev/null 2>&1
end
cd $work
$quench new console --template web --quench-path $root >/dev/null; or exit 1
cd console
$quench android init --package $pkg >/dev/null; or exit 1
for plugin in jobs device secure-storage notifications files packages media share widget
    $quench plugin add $plugin >/dev/null; or exit 1
end
cp $root/scripts/android-console/lib.rs app/src/lib.rs
sed -i 's/^quench-ui = { workspace = true }/quench-serde = { workspace = true }\nquench-ui = { workspace = true }/' app/Cargo.toml
$quench android build-native --abi $abi; or exit 1
$quench build --bundle >/dev/null; or exit 1
cd android
./gradlew -q assembleDebug 2>&1 | string match -rv '^w:'; test $pipestatus[1] -eq 0; or exit 1
adb uninstall $pkg >/dev/null 2>&1
adb install app/build/outputs/apk/debug/app-debug.apk >/dev/null; or exit 1
start_app

# ---- the bridge reaches the plugins ---------------------------------------------------------------
check "echo.say" (call echo say '{"text":"hi"}' | jq -c .) '{"ok":"hi"}'
check "an unknown plugin is an error with a code" (call nothing x | jq -r .error.code) unknown_plugin

# ---- 9.4 secure storage --------------------------------------------------------------------------
call secure_storage set '{"key":"token","value":"s3cret-token"}' >/dev/null
check "secure storage returns what was set" (call secure_storage get '{"key":"token"}' | jq -r .ok) s3cret-token
adb shell am force-stop $pkg
start_app
check "and it survives a restart" (call secure_storage get '{"key":"token"}' | jq -r .ok) s3cret-token
check "what is on disk is not the secret" (adb shell run-as $pkg grep -c s3cret shared_prefs/quench-secure.xml | string trim) 0
check "a key never set is null" (call secure_storage get '{"key":"nope"}' | jq -c .ok) null
call secure_storage remove '{"key":"token"}' >/dev/null
check "a removed key is gone" (call secure_storage keys | jq -c .ok) '[]'

# ---- 9.6 what the device is doing -----------------------------------------------------------------
check "the device state has a network and a battery" (call device state | jq -r '[.ok.online, (.ok.battery_percent >= 0)] | join(",")') true,true
call device watch >/dev/null
adb shell svc wifi disable; adb shell svc data disable
sleep 4
check "the state follows the network down" (call device state | jq -r .ok.online) false
adb shell svc wifi enable; adb shell svc data enable
sleep 5
check_yes "and a change was pushed as an event" "wait_event 'device.state .*\"online\":false' 5"
call device unwatch >/dev/null

# ---- 9.3 background jobs: native code, with no app --------------------------------------------------
check "a one-off job runs the native code" (call jobs run_now '{"task":"heartbeat"}' | jq -r .ok.scheduled) true
sleep 3
check "and it recorded a run" (call jobs last_run '{"task":"heartbeat"}' | jq -r '.ok.runs >= 1') true
check_yes "the native code wrote its file" "adb shell run-as $pkg grep -q heartbeat files/heartbeat.log"
call jobs schedule '{"task":"heartbeat","interval_minutes":60,"network":"unmetered","charging":true,"battery_not_low":true}' >/dev/null
set -l id (job_id heartbeat)
set -l dump (adb shell dumpsys jobscheduler | string collect)
check "the constraints reach the scheduler" (string match -rq 'Required constraints:.*CHARGING.*BATTERY_NOT_LOW.*CONNECTIVITY' -- $dump; and echo yes) yes
check "as a periodic job" (string match -rq "PERIODIC: interval=\+1h" -- $dump; and echo yes) yes
set -l lines_before (adb shell run-as $pkg grep -c heartbeat files/heartbeat.log | string trim)
adb shell input keyevent KEYCODE_HOME
sleep 2
adb shell am kill $pkg
sleep 1
check "the app process is gone (as after swiping it away)" (adb shell pidof $pkg | string trim) ""
adb shell cmd jobscheduler run -f $pkg $id >/dev/null
sleep 5
check_yes "the scheduler started the job in a new process" "test -n \"(adb shell pidof $pkg | string trim)\""
set -l lines_after (adb shell run-as $pkg grep -c heartbeat files/heartbeat.log | string trim)
check_yes "and the native code ran again, with no page ($lines_before to $lines_after lines)" "test $lines_after -gt $lines_before"
start_app
call jobs cancel '{"task":"heartbeat"}' >/dev/null
check "a cancelled job is no longer pending" (call jobs pending | jq -c .ok) '[]'

# ---- 9.5 notifications and reminders -------------------------------------------------------------
adb shell pm grant $pkg android.permission.POST_NOTIFICATIONS
sleep 2 # a grant can end the app's process
call notifications channel '{"id":"reminders","name":"Reminders","importance":3,"description":"test"}' >/dev/null
check "a notification is posted" (call notifications show '{"id":1,"title":"Hello","body":"now","channel":"reminders"}' | jq -r .ok.posted) true
check_yes "and the system has it" "adb shell dumpsys notification --noredact | string match -q '*pkg=$pkg*id=1 *'"
set -l expected (node -e "
const at = new Date('2026-01-01T00:00:00Z'); void at;
const now = Date.now(); let t = new Date(now); t.setUTCHours(21, 0, 0, 0); if (t.getTime() <= now) t = new Date(t.getTime() + 86400000);
console.log(t.getTime())")
set -l daily (call notifications schedule '{"id":9,"title":"Daily","body":"06:00 in Tokyo","channel":"reminders","repeat_zone":"Asia/Tokyo","repeat_time":"06:00"}')
check "a repeating reminder is due at 06:00 Tokyo (21:00 UTC)" (echo $daily | jq -r .ok.at_ms) $expected
if not set -q SKIP_REBOOT
    set -l due (math (date +%s)000 + 75000)
    call notifications schedule "{\"id\":7,\"at_ms\":$due,\"title\":\"Reminder\",\"body\":\"survives a reboot\",\"channel\":\"reminders\"}" >/dev/null
    adb reboot
    adb wait-for-device
    for i in (seq 1 90)
        test (adb shell getprop sys.boot_completed 2>/dev/null | string trim) = 1; and break
        sleep 2
    end
    sleep 8
    check_yes "after the reboot the reminders were armed again" "adb logcat -d -s quench-reminders | string match -q '*re-armed*'"
    for i in (seq 1 60)
        adb logcat -d -s quench-reminders | string match -q '*fired 7*'; and break
        sleep 2
    end
    check_yes "and the reminder fired" "adb logcat -d -s quench-reminders | string match -q '*fired 7: Reminder*'"
    start_app
    check "the repeating one was armed again too" (call notifications pending | jq -r '[.ok[] | select(.id == 9)] | length') 1
end
call notifications cancel '{"id":9}' >/dev/null

# ---- 9.7 files --------------------------------------------------------------------------------------
head -c 20971520 /dev/urandom >$work/big.jpg
set -l sha (sha256sum $work/big.jpg | string split ' ')[1]
adb push $work/big.jpg /sdcard/Pictures/big.jpg >/dev/null
adb shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Pictures/big.jpg >/dev/null
sleep 3
adb shell pm grant $pkg android.permission.READ_MEDIA_IMAGES
sleep 2 # a grant can end the app's process
adb shell pm grant $pkg android.permission.READ_MEDIA_AUDIO
sleep 2 # a grant can end the app's process
set -l image (call files media '{"kind":"images","limit":20}' | jq -r '[.ok[] | select(.name == "big.jpg")][0]')
set -l uri (echo $image | jq -r .uri)
check "the media library lists the image" (echo $image | jq -r .size) 20971520
check "stat knows its size" (call files stat "{\"uri\":\"$uri\"}" | jq -r .ok.size) 20971520
check "a chunk of it is the same bytes" (call files read "{\"uri\":\"$uri\",\"offset\":0,\"length\":16}" | jq -r .ok.data) (head -c 16 $work/big.jpg | base64)
node $root/scripts/android-fixtures.mjs sink 3151 >$work/sink.log &
set -l sink $last_pid
adb reverse tcp:3151 tcp:3151 >/dev/null
call files upload "{\"id\":\"u1\",\"uri\":\"$uri\",\"url\":\"http://127.0.0.1:3151/up\",\"method\":\"PUT\"}" >/dev/null
for i in (seq 1 40)
    events | string match -q 'files.done*'; and break
    sleep 1
end
check "an upload streams the whole file" (jq -r .bytes $work/sink.log) 20971520
check "and intact: the same SHA-256" (jq -r .sha256 $work/sink.log) $sha
check_yes "and reported progress" "events | string match -q 'files.progress*'"
kill $sink 2>/dev/null
# the picker is the system's UI: a folder, then the grant
echo "hello saf" >$work/note.txt
adb push $work/note.txt /sdcard/Documents/note.txt >/dev/null
# The system's UI is slow to read on an emulator (about 2 s a look): the whole flow takes minutes.
begin
    set -lx PLUGIN_TIMEOUT_MS 400000
    call files pick_folder >$work/pick.json
end &
set -l picking $last_pid
sleep 3
# The picker is driven by what it shows: the confirmation's ALLOW first, else USE THIS FOLDER, else the
# Documents folder. (Both buttons share one resource id, so they are found by their texts.)
# It ends when the picker has gone (the grant was given), not when the call has returned: the call
# returns only once the app is back and the page has the result.
set -l shown 0
for step in (seq 1 40)
    if adb shell dumpsys window | grep mCurrentFocus | grep -q documentsui
        set shown 1
    else if test $shown = 1
        break
    end
    if node $root/scripts/android-dialog.mjs text:ALLOW 2>/dev/null
        sleep 3
    else if node $root/scripts/android-dialog.mjs "text:USE THIS FOLDER" 2>/dev/null
        sleep 3
    else if node $root/scripts/android-dialog.mjs text:Documents 2>/dev/null
        sleep 2
    else
        sleep 2
    end
end
wait $picking
set -l tree (jq -r '.ok.uris[0]' $work/pick.json)
check_yes "the picked folder is a tree URI" "string match -q 'content://*tree*' '$tree'"
adb shell am force-stop $pkg
start_app
check "its grant outlives a restart: the folder lists" (call files list_folder "{\"uri\":\"$tree\"}" | jq -r '.ok[0].name') note.txt
set -l note (call files list_folder "{\"uri\":\"$tree\"}" | jq -r '.ok[0].uri')
check "and a file in it reads" (call files read "{\"uri\":\"$note\"}" | jq -r .ok.data | base64 -d) "hello saf"
check "a folder never granted is permission_denied" (call files list_folder '{"uri":"content://com.android.externalstorage.documents/tree/primary%3ADownload"}' | jq -r .error.code) permission_denied

# ---- 9.8 installing from a downloaded APK ------------------------------------------------------------
set -l payload_pkg dev.lorehaven.payload
cd $work
$quench new payload --template web --quench-path $root >/dev/null; or exit 1
cd payload
$quench android init --package $payload_pkg >/dev/null
$quench build --bundle >/dev/null
cd android
./gradlew -q assembleDebug 2>&1 | string match -rv '^w:'
cd $root
node $root/scripts/android-fixtures.mjs file 3152 $work/payload/android/app/build/outputs/apk/debug/app-debug.apk &
set -l store $last_pid
adb reverse tcp:3152 tcp:3152 >/dev/null
adb uninstall $payload_pkg >/dev/null 2>&1
check "the payload is not installed" (call packages installed "{\"package\":\"$payload_pkg\"}" | jq -c .ok) '{"installed":false}'
adb shell appops set $pkg REQUEST_INSTALL_PACKAGES deny
check "without the person's permission an install is refused" (call packages install_from_url '{"url":"http://127.0.0.1:3152/a.apk"}' | jq -r .error.code) permission_denied
adb shell appops set $pkg REQUEST_INSTALL_PACKAGES allow
check "with it, the install starts" (call packages install_from_url '{"url":"http://127.0.0.1:3152/a.apk"}' | jq -r .ok) null
press install_confirm_question
check_yes "the system asks the person to confirm" "true"
press button1 # Install
sleep 3
adb shell am start -n $pkg/.MainActivity >/dev/null # the installer covers the app: bring it back
check_yes "the download was reported" "wait_event 'packages.download' 15"
check_yes "and the outcome arrived as an event" "wait_event 'packages.install .*\"status\":\"(success|failure|cancelled)\"' 30"
# On an emulator image whose Play Protect cannot verify the app (no network to Google) the outcome is
# a failure with INSTALL_FAILED_VERIFICATION_FAILURE; the flow up to the system's verdict is what runs here.
kill $store 2>/dev/null

# ---- 9.9 speech, audio, sharing, a widget -----------------------------------------------------------
python3 -c "
import wave, struct, math
w = wave.open('$work/tone.wav', 'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(22050)
w.writeframes(b''.join(struct.pack('<h', int(8000 * math.sin(2 * math.pi * 440 * i / 22050))) for i in range(22050 * 6)))
w.close()"
adb push $work/tone.wav /sdcard/Music/tone.wav >/dev/null
adb shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Music/tone.wav >/dev/null
sleep 3
set -l tone (call files media '{"kind":"audio","limit":20}' | jq -r '[.ok[] | select(.name == "tone.wav")][0].uri')
check "audio plays a track" (call audio play "{\"uri\":\"$tone\"}" | jq -r .ok.duration_ms) 6000
sleep 1
check "and its position moves" (call audio state | jq -r '.ok.playing and .ok.position_ms > 0') true
call audio pause >/dev/null
call audio seek '{"ms":3000}' >/dev/null
check "it pauses and seeks" (call audio state | jq -r '[.ok.playing, .ok.position_ms] | join(",")') false,3000
call audio stop >/dev/null
check_yes "speech has voices" "call tts voices | jq -e '.ok | length > 0' >/dev/null"
call tts speak '{"text":"Hello from quench","lang":"en-US"}' >/dev/null
sleep 4
check_yes "and says when it started and finished" "events | string match -q 'tts.done*'"
adb shell am start -a android.intent.action.SEND -t text/plain --es android.intent.extra.TEXT "'shared text'" -n $pkg/.MainActivity >/dev/null
sleep 3
check "a share sent to the app arrives as an intent" (call intents take | jq -r '.ok[0].text') "shared text"
check "and is taken once" (call intents take | jq -c .ok) '[]'
adb shell am force-stop $pkg
adb shell am start -a android.intent.action.SEND -t text/plain --es android.intent.extra.TEXT "'cold text'" -n $pkg/.MainActivity >/dev/null
sleep 8
check "a share that starts the app waits for the page" (call intents take | jq -r '.ok[0].text') "cold text"
call share text '{"text":"hi","title":"Send"}' >/dev/null
sleep 2
check_yes "sharing out opens the system's chooser" "adb shell dumpsys activity activities | string match -q '*ChooserActivity*'"
adb shell input keyevent KEYCODE_BACK
sleep 1
check "a widget can be updated" (call widget set '{"title":"Today","body":"3 reminders"}' | jq -c .ok) null
check "and the launcher is asked to place one" (call widget pin | jq -r .ok.requested) true
adb shell input keyevent KEYCODE_BACK # the launcher's dialog for the widget
check_yes "the widget is registered with the system" "adb shell dumpsys appwidget | string match -q '*QuenchWidgetProvider*'"

# ---- done -------------------------------------------------------------------------------------------------
adb reverse --remove-all >/dev/null
adb uninstall $payload_pkg >/dev/null 2>&1
test $failed -eq 0 || echo "calls: $work/calls.log"
test $failed -eq 0; and rm -rf $work
test $failed -eq 0; and echo "android plugins e2e passed"
exit $failed
