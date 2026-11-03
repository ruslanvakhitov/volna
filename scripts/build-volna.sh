#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/scripts/volna-env.sh"
cd "$root"
SCRAWL_HELPER_PLIST="$root/packaging/macos/scrawl-helper-Info.plist" cargo build --locked --release
bundle="$root/dist/Volna.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp target/release/hex "$bundle/Contents/MacOS/volna"
cp target/release/scrawl-helper "$bundle/Contents/MacOS/scrawl-helper"
cp app/Info.plist "$bundle/Contents/Info.plist"
cp assets/volna/AppIcon.icns LICENSE THIRD_PARTY_NOTICES.md VOLNA_NOTICES.md "$bundle/Contents/Resources/"
python3 - "$bundle/Contents/Info.plist" <<'PY'
import sys, plistlib
p=sys.argv[1]
with open(p,'rb') as f: d=plistlib.load(f)
d.update(CFBundleName='Volna',CFBundleDisplayName='Volna',CFBundleIdentifier='com.ruslan.volna',CFBundleExecutable='volna',CFBundleShortVersionString='0.1.5',CFBundleVersion='105')
d.pop('CFBundleIconName',None)
d['CFBundleURLTypes']=[{'CFBundleURLName':'com.ruslan.volna','CFBundleURLSchemes':['volna']}]
d['NSScreenCaptureUsageDescription']='Volna reads screen text locally only for contextual dictation. Screenshots and extracted text stay locally with retained dictation History.'
with open(p,'wb') as f: plistlib.dump(d,f)
PY
# Include the local inference runtime when it is installed on the build Mac.
# Models stay in Application Support, not in the application or source repo.
engine=/Applications/Ollama.app/Contents/Resources
if [ -x "$engine/ollama" ]; then
  mkdir -p "$bundle/Contents/Resources/engine"
  ditto "$engine" "$bundle/Contents/Resources/engine"
fi
codesign --force --sign - --identifier com.ruslan.volna.scrawl-helper "$bundle/Contents/MacOS/scrawl-helper"
codesign --force --deep --sign - --identifier com.ruslan.volna --entitlements .github/release/hex.entitlements "$bundle"
codesign --verify --deep --strict "$bundle"
printf '%s\n' "$bundle"
