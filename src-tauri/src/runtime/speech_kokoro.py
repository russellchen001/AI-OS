"""AI-OS Kokoro runner. Text arrives through stdin, never via shell."""

import sys
import numpy as np
import soundfile as sf
from mlx_audio.tts.utils import load_model


def main():
    if len(sys.argv) != 3:
        raise ValueError("Expected voice and output arguments")

    voice = sys.argv[1]
    output = sys.argv[2]

    allowed_voices = {
        "zf_xiaobei",
        "zm_yunxi",
        "af_heart",
    }

    if voice not in allowed_voices:
        raise ValueError("Unsupported Kokoro voice")

    text = sys.stdin.read()

    if not text.strip() or len(text) > 2000:
        raise ValueError("Invalid speech text")

    language = "a" if voice.startswith("af_") else "z"

    model = load_model("mlx-community/Kokoro-82M-bf16")

    chunks = []
    sample_rate = None

    for result in model.generate(
        text=text,
        voice=voice,
        speed=1.0,
        lang_code=language,
    ):
        chunks.append(np.asarray(result.audio))
        sample_rate = result.sample_rate

    if not chunks or not sample_rate:
        raise RuntimeError("Kokoro returned no audio")

    audio = np.concatenate(chunks)

    sf.write(output, audio, sample_rate)


if __name__ == "__main__":
    main()
