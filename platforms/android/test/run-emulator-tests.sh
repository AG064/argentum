#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
#
# Installs the debug APK and its test APK on a running emulator, points the
# app's 127.0.0.1:PORT at the fake provider on the host, runs the UI
# instrumentation, and collects screenshots and logcat into OUT.
#
# Usage: run-emulator-tests.sh APK TEST_APK PORT OUT
set -euo pipefail

apk=$1
test_apk=$2
port=$3
out=$4
mkdir -p "$out"

adb wait-for-device
until [ "$(adb shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
adb shell input keyevent KEYCODE_WAKEUP
adb shell wm dismiss-keyguard || true

adb install -r -g "$apk"
# The test APK is marked test-only, so it needs -t.
adb install -r -t -g "$test_apk"
# The app's own 127.0.0.1:PORT reaches the fake provider on the host.
adb reverse "tcp:$port" "tcp:$port"
adb logcat -c

status=0
adb shell am instrument -w -r \
  -e fake_endpoint "http://127.0.0.1:$port/v1/" \
  com.argentum.test/com.argentum.uitest.ArgentumUiInstrumentation \
  | tr -d '\r' | tee "$out/instrumentation.txt" || status=$?

adb logcat -d > "$out/logcat.txt" || true
adb shell run-as com.argentum cat files/ui-test/state.json > "$out/state.json" 2>/dev/null || true
adb pull /sdcard/Android/data/com.argentum/files/ui-test-shots "$out/screenshots" > /dev/null 2>&1 || true

if [ "$status" -ne 0 ]; then
  echo "am instrument exited with $status" >&2
  exit 1
fi
for test in open first_run settings back message; do
  line=$(grep -E "^INSTRUMENTATION_RESULT: ARGENTUM_UI_${test}=" "$out/instrumentation.txt" || true)
  echo "${line:-ARGENTUM_UI_${test}: no result}"
  case "$line" in
    *=pass|*=skipped) ;;
    *) status=1 ;;
  esac
done
if grep -q '^INSTRUMENTATION_RESULT: ARGENTUM_UI_failure=' "$out/instrumentation.txt"; then
  grep '^INSTRUMENTATION_RESULT: ARGENTUM_UI_failure=' "$out/instrumentation.txt" >&2
  status=1
fi
if ! grep -q '^INSTRUMENTATION_CODE: -1' "$out/instrumentation.txt"; then
  echo 'The instrumentation did not report success' >&2
  status=1
fi
if grep -q 'argentum panicked' "$out/logcat.txt"; then
  echo 'The app panicked; see logcat.txt' >&2
  status=1
fi
exit "$status"
