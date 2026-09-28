import numpy as np
import wave
import os

SAMPLE_RATE = 44100

def apply_envelope(samples, attack_ms, decay_rate=14.0):
    attack_samples = int(SAMPLE_RATE * (attack_ms / 1000.0))
    n = len(samples)
    env = np.ones(n, dtype=np.float32)
    if attack_samples > 0:
        env[:attack_samples] = 0.5 * (1.0 - np.cos(np.pi * np.arange(attack_samples) / attack_samples))
    t = np.linspace(0, 1, n)
    decay = np.exp(-decay_rate * t)
    return samples * (env * decay)

def synth_tone(freq, duration_sec, harmonics=[(1.0, 1.0), (2.0, 0.22), (3.0, 0.06)], attack_ms=4, decay_rate=16.0):
    num_samples = int(SAMPLE_RATE * duration_sec)
    t = np.arange(num_samples) / SAMPLE_RATE
    sig = np.zeros(num_samples, dtype=np.float32)
    for mult, weight in harmonics:
        sig += weight * np.sin(2.0 * np.pi * freq * mult * t)
    return apply_envelope(sig, attack_ms, decay_rate)

def save_wav(path, samples, peak=0.50):
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    max_val = np.max(np.abs(samples))
    if max_val > 0:
        samples = (samples / max_val) * peak
    int16_samples = np.int16(np.clip(samples, -1.0, 1.0) * 32767)
    with wave.open(path, 'wb') as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(SAMPLE_RATE)
        wf.writeframes(int16_samples.tobytes())
    print(f"Saved: {path} ({len(samples)/SAMPLE_RATE:.3f}s)")

# --- Option 1: Crisp Resolve (The musical answer to Start) ---
# Start was C5 (523 Hz) -> G5 (784 Hz).
# Option 1 resolves upward: G5 (784 Hz) -> C6 (1046 Hz).
# Crisp, minimalist, identical timbre and length to start sound (~190ms).
def generate_option1():
    n1 = synth_tone(783.99, 0.07, harmonics=[(1.0, 1.0), (2.0, 0.22), (3.0, 0.06)], attack_ms=3, decay_rate=18.0)
    n2 = synth_tone(1046.50, 0.14, harmonics=[(1.0, 1.0), (2.0, 0.20), (3.0, 0.05)], attack_ms=3, decay_rate=13.0)
    overlap = int(SAMPLE_RATE * 0.015)
    total_len = len(n1) + len(n2) - overlap
    out = np.zeros(total_len, dtype=np.float32)
    out[:len(n1)] += n1 * 0.75
    out[len(n1)-overlap : len(n1)-overlap+len(n2)] += n2 * 0.90
    return out

# --- Option 2: Soft Ripple (Gentle 3-note micro-arpeggio) ---
# E5 (659 Hz) -> G5 (784 Hz) -> C6 (1046 Hz).
# Very soft, bubbly, high-end Apple UI feel (~220ms).
def generate_option2():
    n1 = synth_tone(659.25, 0.05, harmonics=[(1.0, 1.0), (2.0, 0.18)], attack_ms=3, decay_rate=22.0)
    n2 = synth_tone(783.99, 0.06, harmonics=[(1.0, 1.0), (2.0, 0.20)], attack_ms=3, decay_rate=20.0)
    n3 = synth_tone(1046.50, 0.14, harmonics=[(1.0, 1.0), (2.0, 0.22), (3.0, 0.04)], attack_ms=3, decay_rate=13.0)
    step = int(SAMPLE_RATE * 0.038)
    total_len = step * 2 + len(n3)
    out = np.zeros(total_len, dtype=np.float32)
    out[:len(n1)] += n1 * 0.60
    out[step : step+len(n2)] += n2 * 0.75
    out[step*2 : step*2+len(n3)] += n3 * 0.90
    return out

# --- Option 3: Tactile Tap & Settle (High tick + warm base lock) ---
# High crisp tap (1174 Hz) immediately settling into warm G5 (784 Hz) with a subtle wooden thud.
# Like a mechanical lock snapping shut (~160ms).
def generate_option3():
    # Crisp upper tick
    tick = synth_tone(1174.66, 0.04, harmonics=[(1.0, 1.0), (2.0, 0.3), (3.0, 0.1)], attack_ms=2, decay_rate=35.0)
    # Warm body note
    body = synth_tone(783.99, 0.14, harmonics=[(1.0, 1.0), (2.0, 0.25), (3.0, 0.05)], attack_ms=4, decay_rate=16.0)
    # Subtle sub tap
    thud = synth_tone(261.63, 0.08, harmonics=[(1.0, 1.0), (2.0, 0.1)], attack_ms=3, decay_rate=25.0)
    
    total_len = max(len(tick) + int(SAMPLE_RATE * 0.02), len(body) + int(SAMPLE_RATE * 0.015))
    out = np.zeros(total_len, dtype=np.float32)
    out[:len(tick)] += tick * 0.65
    offset = int(SAMPLE_RATE * 0.015)
    out[offset : offset+len(body)] += body * 0.85
    out[offset : offset+len(thud)] += thud * 0.35
    return out

if __name__ == "__main__":
    script_dir = os.path.dirname(os.path.abspath(__file__))
    out_dir = os.path.join(script_dir, "dist_sounds")
    
    op1 = generate_option1()
    op2 = generate_option2()
    op3 = generate_option3()
    
    p1 = os.path.join(out_dir, "complete_option1_crisp_resolve.wav")
    p2 = os.path.join(out_dir, "complete_option2_soft_ripple.wav")
    p3 = os.path.join(out_dir, "complete_option3_tactile_lock.wav")
    
    save_wav(p1, op1)
    save_wav(p2, op2)
    save_wav(p3, op3)
    print("Done generating completion sound options.")
