#!/bin/bash
# Source from build/check scripts. No global Xcode selection or SDK edits.
volna_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
volna_xcode_major=$(xcodebuild -version 2>/dev/null | sed -n '1s/Xcode \([0-9]*\).*/\1/p')
if [ "${volna_xcode_major:-0}" -lt 26 ]; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
  export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.1.sdk
  if [ ! -d "$SDKROOT" ]; then
    echo 'Volna needs Xcode 26 or the macOS 26.1 Command Line Tools SDK.' >&2
    return 1
  fi
  python3 "$volna_root/scripts/prepare-volna-toolchain.py"
  export PATH="$volna_root/.build-tools:/Library/Developer/CommandLineTools/usr/bin:$PATH"
  # CLT has FoundationModels but not its macro plugin. Dictation's local
  # Qwen corrector is independent; only optional meeting summaries lack FM.
  export VOLNA_NO_FOUNDATION_MODELS=1
fi
export HEX_LOCAL_BUILD=1
export HEX_VERSION=0.1.1
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_DEBUG=0
export MACOSX_DEPLOYMENT_TARGET=15.0

export SCRAWL_HELPER_PLIST="$volna_root/packaging/macos/scrawl-helper-Info.plist"
