# Volna

Local voice dictation with vocabulary that understands the app you are using.
An independently branded macOS fork of [Hex](https://github.com/anomalyco/hex).

- **Option**: hold, speak, release. Dictation reuses the active app’s learned terms.
- **Right Command**: hold, speak, release. Volna takes one screenshot of the active
  app’s display and reads its Russian/English text locally while you speak.
- **Per-app dictionaries**: terms learned in Telegram stay with Telegram. The
  screen excerpt helps correct names and technical vocabulary after speech recognition.
- **Local processing**: Apple Vision OCR and Qwen3.5-0.8B through a private,
  cloud-disabled Ollama runtime. No API key. Screenshots and OCR excerpts are not
  saved; learned terms stay in owner-only files and expire after 30 days.
- **New identity**: wave icon, sea-glass theme, voice ribbon, glass-ripple sounds.

This version targets Apple silicon on macOS. Give Volna its own Microphone,
Accessibility, and Screen Recording permissions. Upstream Hex’s files and grants
are independent. Once Volna’s dictation setup is ready, it asks the old Hex to quit
so they do not both answer Option. It never deletes the old application.

## Build on a Mac

Requires Rust 1.97 or later, Xcode 26 (or macOS 26.1 Command Line Tools), and Ollama
on the build Mac. The local correction model is about 1.3 GB.

```sh
scripts/setup-volna.sh
scripts/build-volna.sh
scripts/check-volna.sh
open dist/Volna.app
```

Output: `dist/Volna.app`, ad-hoc signed for local use. The builder includes the
installed Ollama runtime and its licenses. The model lives separately in
`~/Library/Application Support/volna/context-models`. This is a local build,
without Developer ID notarization or upstream automatic updates.

Settings → General includes the contextual shortcut, screen access, learning,
local correction and inspectable app dictionaries. Forgetting a dictionary also
invalidates learning from a screenshot already in flight. Shared, manually edited
vocabulary and the upstream speech-model picker remain available.

```sh
# Read status or inspect learned terms; these do not take a screenshot.
dist/Volna.app/Contents/MacOS/volna context status
dist/Volna.app/Contents/MacOS/volna context dictionaries
# Test OCR on your own image without learning.
dist/Volna.app/Contents/MacOS/volna context ocr sample.png
# Test local correction on supplied text, without learning.
dist/Volna.app/Contents/MacOS/volna context rewrite \
  'Посмотрим кодекс в 15 часов' --screen 'Codex — developer tools'
```

See [screen-context behavior and verification](docs/features/screen-context.md).
The rest of the inherited capabilities are documented under `docs/features/`.
The CLT fallback cannot build Apple’s FoundationModels macro plugin; optional
meeting summaries using Apple Intelligence are unavailable in that build.
Dictation, screen OCR and local Qwen correction do not use FoundationModels.

## License

MIT. Upstream copyright and license are retained in [LICENSE](LICENSE).
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
[VOLNA_NOTICES.md](VOLNA_NOTICES.md) for third-party components and models.
