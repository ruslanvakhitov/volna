# Volna contextual dictation

Implemented in this fork; native verification results are recorded separately
from source behavior below.

```text
Option → audio capture → transcription → this app’s dictionary → local correction → paste
Right Command → intentional hold (300 ms) ┬→ audio capture → transcription ─┐
                                         └→ active display → Vision OCR ────┤
                        app bundle ID → learn bounded terms → Qwen edits ──┴→ paste
```

Both shortcuts use the inherited hold/toggle and double-tap state machine. The
second shortcut is enabled and bound to Right Command by default in Volna’s
macOS settings. Its binding remains configurable. Command taps and ordinary
Command chords discarded before the hold threshold never request a screenshot.
A Command chord added after an intentional hold can cancel dictation; a screen
read already begun cannot be retracted.

The foreground app name, bundle identifier and PID are fixed at the shortcut
press. The native bridge verifies that PID before the screenshot. Later focus
changes cannot move the learned terms to another app or select another app’s
saved dictionary for that dictation. Browser tabs currently share their browser’s
dictionary; website-specific dictionaries are not implemented.

Apple Vision’s accurate recognizer is used because its fast recognizer does not
support Cyrillic on all macOS versions. It runs off the audio and UI threads, in
parallel with recording. Only one OCR worker runs; at most two more requests wait.
Capture/recognition has a 12 second bridge deadline, including ScreenCaptureKit cold startup. A real Telegram screen exceeded the old 3 second deadline even during a 9 second dictation; OCR on the saved image itself took about 0.7 seconds. Processing waits at most 450 ms
for an outstanding OCR result, then falls back to saved terms. OCR can misread
letters, especially Latin I/l; learned vocabulary is a hint, not a verified transcript.

Qwen3.5-0.8B runs through a cloud-disabled, loopback-only Ollama service owned by
Volna. It starts and prewarms at app startup, not on the audio callback. No model
is downloaded while dictating. Correction is bounded by a 5 second HTTP deadline;
a missing, busy or failed model preserves the transcript with exact-case dictionary
fixes. The local model chooses among phonetic candidate spans. Every accepted
edit must match an original whole-word span and a known term. It cannot provide
arbitrary replacement prose or edit digits. Raw dictation bypasses correction.
Existing configured Output transformations run afterward as before. OCR text is
not added to an external-provider prompt by this feature.

Candidate terms persist in `app-vocabulary.json`. Since Volna 0.1.1, each retained
dictation also keeps its exact screenshot as an owner-only JPEG under
`history-screens/<id>.jpg`, OCR excerpt, before/after text, available terms,
application ID, timings and correction/fallback status. History Off stops saving
new evidence; retention, Delete, Clear and text-cap eviction remove the attached
JPEG. A separate 128 MiB cap evicts oldest screenshots first while keeping text.
The History detail pane previews the screenshot and opens its full-size local
file. Old entries have no retroactive evidence. OCR that missed the processing
deadline is not presented as if correction used it. Option entries explicitly
say that no screenshot was requested and list their app dictionary terms.
Dictionaries are keyed by bundle ID,
with 64 apps and 256 terms per app. Each term expires 30 days after it was last seen;
expired terms are filtered immediately and removed on subsequent processing/startup.
Writes use owner-only temporary files and atomic replacement. Forgetting an app
invalidates prior OCR learning tickets; accepted dictation jobs may still hold the
vocabulary snapshot they started with. The General pane can disable screen
context, disable learning, disable local correction, inspect terms, and forget an
app after confirmation.

Sources: `src/screen_context{.rs,.swift}`, `src/screen_context/`,
`src/recognition.rs`, `src/dictation/processor.rs`, `src/rune_ui/context.rs`.
The new bundle is `com.ruslan.volna`, with its own `volna` Application Support
folder and `~/.config/volna` config root. Upstream auto-updates are disabled.

Verification:

- Tests exercise candidate boundaries, phonetic matching, numeric preservation,
  exact spelling, session identity, dictionary isolation, per-term expiration and
  private file permissions. See `scripts/check-volna.sh` for the executed suites.
- The `context ocr` command exercises Vision on a supplied image without taking a
  screenshot or learning. The `context rewrite` command exercises the real local
  model on supplied text.
- Headless UI snapshots exercise fixtures, not physical shortcuts, microphone
  input, target-app paste, or a new app’s macOS permission grants.
- Full native acceptance needs Volna’s three permissions and a physical
  Right Command dictation followed by Option in the same target app.
