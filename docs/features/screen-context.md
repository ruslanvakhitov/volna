# Volna contextual dictation

Implemented in this fork; native verification results are recorded separately
from source behavior below.

```text
Option → audio capture → transcription → this app’s saved text → local correction → paste
Right Command → intentional hold (300 ms) ┬→ audio capture → transcription ─┐
                                         └→ active display → Vision OCR ────┤
                        app bundle ID → remember bounded text → local edits ┴→ paste
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
saved text for that dictation. Browser tabs currently share their browser’s
context; website-specific context is not implemented.

Apple Vision’s accurate recognizer is used because its fast recognizer does not
support Cyrillic on all macOS versions. It runs off the audio and UI threads, in
parallel with recording. Only one OCR worker runs; at most two more requests wait.
Capture has a 12 second bridge deadline, including ScreenCaptureKit cold startup.
Vision runs in a short-lived child process on a JPEG sent through anonymous
pipes, with a separate 5 second deadline. No temporary screenshot file is
created. The child uses the same decoder/recognizer as `context ocr -`.
Live in-process Vision failed during real Telegram dictation at both 3 and
12 second deadlines, while OCR on the saved JPEG took about 0.7 seconds.
The isolated reader avoids running that synchronous ML call on the GUI capture
Task executor. The earlier deadline-only change was not sufficient. Processing waits at most 450 ms
for an outstanding OCR result, then falls back to saved text. OCR can misread
letters, especially Latin I/l; the context is a hint, not a verified transcript.

Qwen3.5-0.8B runs through a cloud-disabled, loopback-only Ollama service owned by
Volna. It starts and prewarms at app startup, not on the audio callback. No model
is downloaded while dictating. Correction is bounded by a 5 second HTTP deadline;
a missing, busy or failed model preserves the transcript. Since 0.1.5 the reference
is ordinary screen text plus the app’s recent saved text, bounded to 8,000
characters. No new ranked keyword list is generated or used by correction. Spelling
proposals are matched to spoken spans against words in that full reference; the
local model selects among those proposals. Every accepted edit must match an
original whole-word span and a spelling present in the reference. It cannot
provide arbitrary replacement prose or edit digits. A unique repeated acronym
with two matching surrounding words in the same reference line can repair a
one-letter speech error such as CLP → SLP, including the first letter. Ambiguous
or already referenced acronyms stay unchanged. Ordinary words do not inherit
capitalization from screen headings. Raw dictation bypasses correction.
Existing configured Output transformations run afterward as before. OCR text is
not added to an external-provider prompt by this feature.

App text persists in the version 2 `app-vocabulary.json` store. It keeps up to four
snapshots per app, at most 6,000 characters per snapshot and 12,000 per app; each
snapshot expires 30 days after it was captured. Heading hash markers are removed
while sentence and line structure is retained. Old keyword-only records remain
readable and preserved, but are not used as prose; the next contextual dictation
populates the new memory. Since Volna 0.1.1, each retained
dictation also keeps its exact screenshot as an owner-only JPEG under
`history-screens/<id>.jpg`, OCR excerpt, before/after text, available terms,
application ID, timings and correction/fallback status. Since 0.1.5 History stores
the exact plain-text reference supplied to correction, including saved context
used by Option, and its text budget includes that field. The old term wall is no
longer displayed. History Off stops saving
new evidence; retention, Delete, Clear and text-cap eviction remove the attached
JPEG. A separate 128 MiB cap evicts oldest screenshots first while keeping text.
The History detail pane previews the screenshot and opens its full-size local
file. Old entries have no retroactive evidence. OCR that missed the processing
deadline is not presented as if correction used it. Option entries explicitly
say that no screenshot was requested and show their saved app text.
Contexts are keyed by bundle ID, with at most 64 apps. Expired snapshots are
filtered immediately and removed on subsequent processing/startup.
Writes use owner-only temporary files and atomic replacement. Forgetting an app
invalidates prior OCR learning tickets; accepted dictation jobs may still hold the
text snapshot they started with. The General pane can disable screen
context, disable learning, disable local correction, inspect text, and forget an
app after confirmation.

Sources: `src/screen_context{.rs,.swift}`, `src/screen_context/`,
`src/recognition.rs`, `src/dictation/processor.rs`, `src/rune_ui/context.rs`.
The new bundle is `com.ruslan.volna`, with its own `volna` Application Support
folder and `~/.config/volna` config root. Upstream auto-updates are disabled.

Verification:

- Tests exercise candidate boundaries, phonetic matching, numeric preservation,
  repeated/ambiguous acronyms, session identity, app isolation, per-snapshot expiration and
  private file permissions. See `scripts/check-volna.sh` for the executed suites.
- The `context ocr` command exercises Vision on a supplied image without taking a
  screenshot or learning. The `context rewrite` command exercises the real local
  model on supplied text.
- Headless UI snapshots exercise fixtures, not physical shortcuts, microphone
  input, target-app paste, or a new app’s macOS permission grants.
- Full native acceptance needs Volna’s three permissions and a physical
  Right Command dictation followed by Option in the same target app.

Diagnostics (macOS): `context status` queries the running app, so its permission
result cannot be confused with the invoking terminal’s grant. `context probe`
uses the real foreground-app session, capture worker and local corrector without
audio, paste or History writes. It does not learn unless `--learn` is supplied;
`--dictionary` tests the Option path, and `--transcript TEXT` tests correction.
Run it with the intended app in front. Both commands use the authenticated
owner-only local API.

Stack sampling of the isolated reader showed waiting in AppleNeuralEngine model
loading. Explicit per-stage GPU selection read the same Telegram image in
450 ms; CPU-only recognition took 3.6 s. The reader now prefers a supported GPU
device, falls back to CPU and uses explicit Russian/English languages rather
than automatic language detection. The subprocess alone did not cure the ANE
loading stall.

Observed on the installed Volna 0.1.4 build on November 3, 2026: an authenticated
native probe of the foreground Telegram app captured an 880,367-byte JPEG and
read 2,210 characters in 3,868 ms (4,000 ms total), learning 96 terms. A subsequent
dictionary-only probe loaded those 96 terms without a screenshot, corrected a
term's spelling and preserved the supplied number 15. A separate local-model
check corrected the public synthetic names “конфидай” and “кодекс” to ConfidAI
and Codex, also preserving 15. The running app reported its microphone,
Accessibility, transcription model and screen permission ready, with its hotkey
listener enabled. These probes exercise the native session, reader and corrector;
they do not establish physical audio capture, paste or History retention on this
build. Earlier physical dictations established audio/paste and retained evidence
but exposed the OCR failures described above. No screenshots or extracted private
text from these checks are stored in the repository.

Validation for 0.1.5: `scripts/check-volna.sh` passed 175 tests across its eight
suites, with four ignored tests. Both dark and light History fixtures were
rendered and inspected; the screenshot image pixels matched across themes.
Replaying a retained Telegram transcript against its original OCR using the new
local corrector repaired both wrong acronym occurrences and preserved ordinary
lowercase wording. This replay did not change the retained History entry. A
separate public synthetic check corrected ConfidAI/Codex in 462 ms while keeping
15 unchanged; two ordinary-phrase checks stayed unchanged. These are correction
and fixture checks, not a new physical microphone/hotkey acceptance run.

A subsequent physical Telegram dictation on 0.1.5 captured its screen successfully
(2,278 OCR characters, 480 ms) and retained its plain reference, but left a fused
acronym/name spelling unchanged. Equal-length acronym repair does not handle
insertion/deletion or joined multiword names such as SLPI / SLP AI. This is an
observed correction gap, not an OCR failure. The old History result remains
unchanged and accurately records the failure.
