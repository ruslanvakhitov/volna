#!/usr/bin/env python3
"""Isolate stale CLT headers/interfaces without changing Apple's installation."""
from pathlib import Path
import json
import shutil

root = Path(__file__).resolve().parents[1]
out = root / '.build-tools'
out.mkdir(exist_ok=True)
clt = Path('/Library/Developer/CommandLineTools')
(out / 'empty.modulemap').write_text('')
overlay = {'version': 0, 'roots': []}
if (clt / 'usr/include/swift/bridging.modulemap').exists():
    overlay['roots'].append({'type': 'file', 'name': str(clt / 'usr/include/swift/module.modulemap'), 'external-contents': str(out / 'empty.modulemap')})
(out / 'swift-overlay.json').write_text(json.dumps(overlay))
manifest = out / 'ManifestAPI'
manifest.mkdir(exist_ok=True)
for name in ['PackageDescription', 'CompilerPluginSupport']:
    source = clt / 'usr/lib/swift/pm/ManifestAPI' / (name + '.swiftmodule')
    destination = manifest / source.name
    destination.mkdir(exist_ok=True)
    for interface in source.glob('*-apple-macos.swiftinterface'):
        shutil.copy2(interface, destination / interface.name)
library = manifest / 'libPackageDescription.dylib'
if not library.exists():
    library.symlink_to(clt / 'usr/lib/swift/pm/ManifestAPI/libPackageDescription.dylib')
scripts = {
'swiftc': '''#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")" && pwd)
args=()
for arg in "$@"; do
  if [ "$arg" = /Library/Developer/CommandLineTools/usr/lib/swift/pm/ManifestAPI ]; then arg="$root/ManifestAPI"; fi
  args+=("$arg")
done
exec /Library/Developer/CommandLineTools/usr/bin/swiftc -vfsoverlay "$root/swift-overlay.json" -module-cache-path "$root/swift-modules-v3" "${args[@]}"
''',
'swift': '''#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")" && pwd)
export SWIFT_EXEC="$root/swiftc"
export SWIFTPM_MODULECACHE_OVERRIDE="$root/swift-modules-v3"
if [ "${1:-}" = build ]; then
  shift
  exec /Library/Developer/CommandLineTools/usr/bin/swift build --manifest-cache none "$@"
fi
exec /Library/Developer/CommandLineTools/usr/bin/swift "$@"
''',
'xcrun': '''#!/bin/bash
set -euo pipefail
if [ "${1:-}" = swiftc ]; then
  shift
  exec "$(dirname "$0")/swiftc" "$@"
fi
exec /usr/bin/xcrun "$@"
''',
}
for name, text in scripts.items():
    path = out / name
    path.write_text(text)
    path.chmod(0o755)
