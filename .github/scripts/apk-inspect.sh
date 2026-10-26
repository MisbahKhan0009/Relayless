#!/usr/bin/env bash
# Prints signature, SDK levels, ABIs and permissions for every APK in out/, as annotations.
set -u
BT=$(ls -d "$ANDROID_HOME"/build-tools/*/ | sort -V | tail -1)
report=""
for apk in out/*.apk; do
  sig=$("$BT/apksigner" verify --verbose --print-certs "$apk" 2>&1 | grep -E "Verifies|Verified using|Signer #1 certificate DN|WARNING|ERROR|DOES NOT" | head -8)
  badge=$("$BT/aapt2" dump badging "$apk" 2>&1 | grep -E "^package:|sdkVersion|targetSdkVersion|native-code|^application-label:|ERROR" | head -6)
  libs=$(unzip -l "$apk" | grep -oE "lib/[^/]+/[^ ]+\.so" | head -6)
  report+="### $(basename "$apk") ($(du -h "$apk" | cut -f1))%0A$(echo "$sig"; echo "$badge"; echo "$libs" | sed 's/%/%25/g' )%0A"
  echo "== $apk"; echo "$sig"; echo "$badge"; echo "$libs"
done
echo "::notice title=APK inspection::$(echo "$report" | sed 's/%0A/\n/g' | awk '{printf "%s%%0A", $0}')"
