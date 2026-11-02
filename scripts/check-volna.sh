#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source "$root/scripts/volna-env.sh"
cd "$root"
cargo fmt --all --check
python3 scripts/code-shape.py --check
# Use the release graph already built for the app to keep disk use bounded.
for suite in screen_context history recognition::tests dictation::processor::tests feedback::tests desktop::i18n::tests app_settings::tests labs::tests; do
  cargo test --locked --release --config profile.release.package.hex.opt-level=1 --bin hex "$suite"
done
git diff --check
