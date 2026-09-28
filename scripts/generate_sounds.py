import numpy as np
import wave
import os

SAMPLE_RATE = 44100

def apply_envelope(samples, attack_ms, decay_rate=12.0):
    attack_samples = int(SAMPLE_RATE * (attack_ms / 1000.0))
    n = len(samples)
    env = np.ones(n, dtype=np.float32)
    
    # Attack: smooth half-cosine ramp up
    if attack_samples > 0:
        env[:attack_samples] = 0.5 * (1.0 - np.cos(np.pi * np.arange(attack_samples) / attack_samples))
    
    # Decay: natural exponential decay
    t = np.linspace(0, 1, n)
    decay = np.exp(-decay_rate * t)
    env = env * decay
    return samples * env

def synth_note(freq, duration_sec, harmonics=[(1.0, 1.0), (2.0, 0.25), (3.0, 0.08)], attack_ms=6, decay_rate=14.0):
    num_samples = int(SAMPLE_RATE * duration_sec)
    t = np.arange(num_samples) / SAMPLE_RATE
    signal = np.zeros(num_samples, dtype=np.float32)
    
    for mult, weight in harmonics:
        signal += weight * np.sin(2.0 * np.pi * freq * mult * t)
        
    signal = apply_envelope(signal, attack_ms, decay_rate)
    return signal

def generate_start_sound():
    """
    Subtle, snappy ascending two-tone earcon:
    Note 1: C5 (523 Hz) - 60ms
    Note 2: G5 (784 Hz) - 120ms
    Total duration: ~180ms
    """
    n1 = synth_note(523.25, 0.07, harmonics=[(1.0, 1.0), (2.0, 0.22), (3.0, 0.06)], attack_ms=4, decay_rate=18.0)
    n2 = synth_note(783.99, 0.13, harmonics=[(1.0, 1.0), (2.0, 0.25), (3.0, 0.08)], attack_ms=4, decay_rate=14.0)
    
    # Slight overlap of 15ms for smoothness
    overlap_samples = int(SAMPLE_RATE * 0.015)
    total_len = len(n1) + len(n2) - overlap_samples
    combined = np.zeros(total_len, dtype=np.float32)
    
    combined[:len(n1)] += n1 * 0.75
    combined[len(n1) - overlap_samples:len(n1) - overlap_samples + len(n2)] += n2 * 0.85
    
    # Normalize to -6 dBFS (~0.5 max amplitude)
    max_val = np.max(np.abs(combined))
    if max_val > 0:
        combined = (combined / max_val) * 0.50
        
    return combined

def generate_complete_sound():
    """
    Pleasant, rich success chime indicating transcription complete & copied to clipboard:
    Step 1: E5 (659 Hz) - 70ms
    Step 2: A5 (880 Hz) + C#6 (1108 Hz) major chime chord - 320ms
    Total duration: ~380ms
    """
    # Leading gentle note
    lead = synth_note(659.25, 0.08, harmonics=[(1.0, 1.0), (2.0, 0.20), (3.0, 0.05)], attack_ms=6, decay_rate=16.0)
    
    # Chord resolution (A5 + C#6 + touch of E6)
    c1 = synth_note(880.00, 0.32, harmonics=[(1.0, 1.0), (2.0, 0.28), (3.0, 0.09)], attack_ms=6, decay_rate=9.0)
    c2 = synth_note(1108.73, 0.32, harmonics=[(1.0, 0.85), (2.0, 0.22), (3.0, 0.07)], attack_ms=6, decay_rate=10.0)
    c3 = synth_note(1318.51, 0.32, harmonics=[(1.0, 0.40), (2.0, 0.12)], attack_ms=6, decay_rate=12.0)
    chord = c1 + c2 + c3
    
    overlap_samples = int(SAMPLE_RATE * 0.020)
    total_len = len(lead) + len(chord) - overlap_samples
    combined = np.zeros(total_len, dtype=np.float32)
    
    combined[:len(lead)] += lead * 0.65
    combined[len(lead) - overlap_samples:len(lead) - overlap_samples + len(chord)] += chord * 0.55
    
    # Normalize to -5 dBFS (~0.56 max amplitude)
    max_val = np.max(np.abs(combined))
    if max_val > 0:
        combined = (combined / max_val) * 0.56
        
    return combined

def save_wav(filename, samples):
    os.makedirs(os.path.dirname(os.path.abspath(filename)), exist_ok=True)
    # Convert float32 [-1.0, 1.0] to int16
    int16_samples = np.int16(np.clip(samples, -1.0, 1.0) * 32767)
    with wave.open(filename, 'wb') as wav_file:
        wav_file.setnchannels(1)       # Mono
        wav_file.setsampwidth(2)      # 16-bit
        wav_file.setframerate(SAMPLE_RATE)
        wav_file.writeframes(int16_samples.tobytes())
    print(f"Generated {filename} ({len(samples)/SAMPLE_RATE:.3f}s)")

if __name__ == "__main__":
    start_audio = generate_start_sound()
    complete_audio = generate_complete_sound()
    
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.abspath(os.path.join(script_dir, ".."))
    
    # Save to project source directory
    pkg_sounds_dir = os.path.join(project_root, "src-tauri", "sounds")
    pkg_start = os.path.join(pkg_sounds_dir, "start_recording.wav")
    pkg_complete = os.path.join(pkg_sounds_dir, "transcription_complete.wav")
    
    save_wav(pkg_start, start_audio)
    save_wav(pkg_complete, complete_audio)
    print("Done generating sounds.")
