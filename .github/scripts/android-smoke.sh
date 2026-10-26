#!/usr/bin/env bash
# Checks the launched app on the emulator: process alive, engine started, no crash.
set -u
pkg=app.relayless
pid="$(adb shell pidof "$pkg" | tr -d '\r')"
log="$(adb logcat -d 2>/dev/null)"
echo "---- relevant logcat ----"
echo "$log" | grep -E "Relayless|AndroidRuntime|FATAL|$pkg|panicked|tauri" | tail -120
started=no; echo "$log" | grep -q "Relayless engine started" && started=yes
crashed=no; echo "$log" | grep -A3 "FATAL EXCEPTION" | grep -q "$pkg" && crashed=yes
echo "$log" | grep -q "Process: $pkg" && crashed=yes
mem="$(adb shell dumpsys meminfo "$pkg" 2>/dev/null | tr -d '\r' | grep -m1 'TOTAL PSS' | awk '{print $3}')"
summary="alive=${pid:+yes} started=$started crashed=$crashed pss=${mem:-?}KB"
echo "$summary"
if [ -n "$pid" ] && [ "$started" = yes ] && [ "$crashed" = no ]; then
  echo "::notice title=Android smoke test passed::$summary"
  exit 0
fi
enc() { sed 's/%/%25/g; s/\r//g' | awk '{printf "%s%%0A", $0}'; }
echo "::error title=Android smoke test failed ($summary)::$(echo "$log" | grep -E "AndroidRuntime|FATAL|Relayless|panicked|$pkg|E/" | tail -60 | enc)"
exit 1
