# Volna notices

Volna is an independently branded fork of [Hex](https://github.com/anomalyco/hex).
The upstream MIT copyright notice and license remain in `LICENSE`. The
upstream dependency/model/icon notices remain in `THIRD_PARTY_NOTICES.md`.

The original Volna wave mark and water sound set were drawn/synthesized for
this fork. Their editable sources are in `scripts/create-volna-icon.swift`
and `scripts/volna-sounds.py`.

## Local contextual correction

- Qwen3.5-0.8B, by the Qwen team: Apache License 2.0.
  Source: https://huggingface.co/Qwen/Qwen3.5-0.8B
  License: https://huggingface.co/Qwen/Qwen3.5-0.8B/blob/main/LICENSE
  The Ollama distribution `qwen3.5:0.8b` is downloaded separately; Volna
  does not modify its weights. Its manifest and license blobs remain in
  the local model store. The tag can change if explicitly pulled again.
- Ollama runtime: MIT. https://github.com/ollama/ollama/blob/main/LICENSE
  When bundled, its own license and dependency notices are retained alongside
  its binaries in `Contents/Resources/engine`.
- Apple Vision and ScreenCaptureKit are system frameworks, not redistributed
  model weights. OCR runs on this Mac. Pixels are not written to disk or sent
  to an external model provider by the contextual dictation feature.
