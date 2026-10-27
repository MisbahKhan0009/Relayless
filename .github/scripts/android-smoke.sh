#!/usr/bin/env bash
# Checks the launched app on the emulator: process alive, engine started, no crash.
set -u
pkg=io.github.misbahkhan0009.relayless
pid="$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')"
ps_line="$(adb shell ps -A 2>/dev/null | tr -d '\r' | grep -w "$pkg" | head -1)"
log="$(adb logcat -d -v brief 2>/dev/null)"
deaths="$(echo "$log" | grep -E "Killing [0-9]+:$pkg|Process $pkg .*has died|ANR in $pkg|has died: |Force stopping $pkg|lowmemorykiller|am_kill|FATAL EXCEPTION|Fatal signal|SIGSEGV|SIGABRT|backtrace|tombstone|panicked|Abort message" | grep -iE "$pkg|fatal|signal|panick|abort|backtrace|tombstone" | head -30)"
echo "---- process ----"; echo "pidof=$pid"; echo "ps=$ps_line"
echo "---- death / crash lines ----"; echo "$deaths"
echo "---- relevant logcat ----"
echo "$log" | grep -E "Relayless|AndroidRuntime|$pkg|panicked|tauri|DEBUG" | tail -150
started=no; echo "$log" | grep -q "Relayless engine started" && started=yes
alive=no; { [ -n "$pid" ] || [ -n "$ps_line" ]; } && alive=yes
crashed=no; echo "$deaths" | grep -qiE "fatal|signal|panick|abort|backtrace|ANR" && crashed=yes
mem="$(adb shell dumpsys meminfo "$pkg" 2>/dev/null | tr -d '\r' | grep -m1 'TOTAL PSS' | awk '{print $3}')"
summary="alive=$alive started=$started crashed=$crashed pss=${mem:-?}KB"
echo "$summary"
if [ "$alive" = yes ] && [ "$started" = yes ] && [ "$crashed" = no ]; then
  echo "::notice title=Android smoke test passed::$summary"
  exit 0
fi
enc() { sed 's/%/%25/g; s/\r//g' | awk '{printf "%s%%0A", $0}'; }
echo "::error title=Android smoke test failed ($summary)::$( { echo "DEATHS:"; echo "$deaths"; echo "PS: $ps_line"; echo "TAIL:"; echo "$log" | grep -E "$pkg|AndroidRuntime|DEBUG|Relayless" | tail -25; } | enc)"
exit 1
