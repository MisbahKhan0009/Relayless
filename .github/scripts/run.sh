#!/usr/bin/env bash
# Runs a command. On failure, publishes the important part of its output as
# workflow annotations so failures are readable from the checks API.
log="$(mktemp)"
"$@" 2>&1 | tee "$log"
code=${PIPESTATUS[0]}
if [ "$code" -ne 0 ]; then
  enc() { sed 's/%/%25/g; s/\r//g' | awk '{printf "%s%%0A", $0}'; }
  key="$(grep -E 'error(\[|:)|panicked|FAILED|failures:|timed out|-->|^ *[0-9]+ \||thread .* panicked|assertion|Error:' "$log" | grep -v 'Compiling' | tail -70 | enc)"
  echo "::error title=Key lines::${key}"
  echo "::error title=Last lines::$(tail -45 "$log" | enc)"
fi
exit "$code"
