#!/usr/bin/env python3
"""Original short glass-ripple feedback tones, 48 kHz mono PCM."""
from pathlib import Path
import math
import struct
import wave

out = Path(__file__).resolve().parents[1] / 'resources/audio/sounds/volna'
out.mkdir(parents=True, exist_ok=True)
for name, frequencies, duration in [
    ('start', [659.25, 987.77], .12), ('stop', [987.77, 659.25], .16),
    ('done', [1318.51], .10), ('cancel', [554.37, 440], .16),
    ('error', [392, 369.99], .20), ('success', [659.25, 830.61, 987.77], .28),
]:
    samples = []
    rate = 48000
    for i in range(int(rate * duration)):
        t, value = i / rate, 0.0
        for j, frequency in enumerate(frequencies):
            u = t - j * duration * .22
            if u >= 0:
                envelope = (1 - math.exp(-u * 900)) * math.exp(-u * 35) * min(1, (duration - t) * 500)
                value += .15 * envelope * (math.sin(2 * math.pi * frequency * u) + .16 * math.sin(2 * math.pi * frequency * 2.003 * u))
        samples.append(int(max(-1, min(1, value)) * 32767))
    with wave.open(str(out / (name + '.wav')), 'wb') as file:
        file.setnchannels(1)
        file.setsampwidth(2)
        file.setframerate(rate)
        file.writeframes(struct.pack('<' + 'h' * len(samples), *samples))
