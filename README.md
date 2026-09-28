# RevFly

<div align="center">

**The fast, private, local-first desktop voice assistant with instant AI translation.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-brightgreen.svg)](INSTALL.md)
[![Tauri](https://img.shields.io/badge/Built%20With-Tauri%20v2-orange.svg)](https://v2.tauri.app)
[![React](https://img.shields.io/badge/Frontend-React%2019%20%2B%20Tailwind-blueviolet.svg)](https://react.dev)

[Website Demo](https://eotsevych.github.io/RevFly/) • [Quick Install](INSTALL.md) • [Deployment Guide](DEPLOYMENT.md) • [Features](#features)

</div>

---

## Overview

**RevFly** is a lightweight desktop assistant that records your speech, transcribes it locally on your computer in RAM (Random Access Memory), translates foreign languages with AI, and automatically pastes the result directly into your active window.

No cloud audio upload. No browser required. Just tap your hotkey and speak.

---

## Key Features

### 1. Instant Global Hotkey & Hold-to-Talk
- **Single-Tap Modifier Keys**: Use `Right Option` (⌥) or `Right Control` (⌃) directly on macOS.
- **Key Combos Everywhere**: Use any shortcut such as `Ctrl+Shift+Space` on Windows and Linux.
- **Hold-to-Talk Gesture**: Hold the key to speak, release to stop and paste.
- **Toggle Mode**: Tap once to begin recording, tap again to finish.
- **Global Cancel**: Tap `Escape` at any time to instantly stop recording or cancel translation.
- **Micro-Sounds**: Subtly chimes on recording start and completion.

### 2. Local-First In-RAM Speech Recognition
- **Zero Cloud Audio**: Audio records directly into computer memory (RAM). Zero audio files are saved to disk during speech.
- **Parakeet TDT 0.6B**: Runs on Apple Neural Engine (ANE) for ultra-fast, near-zero latency English recognition.
- **Whisper Models**: Bundled with `whisper.cpp` (Medium `q5_0`, Small, Base) for high accuracy multilingual recognition offline.
- **Smart RAM Management**: Automatically unloads models from memory when idle to conserve system memory.

### 3. Voice Activity Detection (VAD)
- **Silero VAD**: Analyzes speech locally via ONNX Runtime.
- **Smart Silence Trimming**: Strips out background silence and pauses.
- **Speech Protection Margins**: Maintains a 400 millisecond safety buffer before and after speech to protect whispers and soft consonants.

### 4. 0.0001s Instant Skip Engine
- **Instant Language Check**: Checks the recognized language locally before calling the network.
- **Zero-Latency Bypass**: If you speak your target language (e.g. English) or any excluded language, translation is bypassed in **0.0001 seconds** with zero API calls.

### 5. Multi-Provider AI Translation
- **Google Gemini**: Fast, fluent translation powered by `gemini-3.6-flash`.
- **Local LLMs**: Connects to local AI models via Ollama or LM Studio (e.g. `llama3.2`, `mistral`).
- **Custom Endpoints**: Compatible with any OpenAI-style REST API endpoint with custom system prompt templates.

### 6. Automatic Text Normalization
- **Cleaner Transcripts**: Removes verbal stuttering, duplicate words, and spoken filler phrases ("um", "uh", "you know", "like").
- **Smart Formatter**: Automatically converts spoken numbers, currency ($100), dates, times, and web addresses (URLs) into standard written text.
- **False-Start Cleanup**: Corrects spoken corrections automatically (e.g., *"I think, sorry, I know"* becomes *"I know"*).

### 7. Confidentiality Guard & Data Masking
- **Sensitive Data Shield**: Masks personal names, project codenames, credit card numbers, phone numbers, and email addresses before sending text to external translation APIs.
- **Custom Replacement Tokens**: Replaces sensitive data with tags like `[CONFIDENTIAL]` or `***`.

### 8. Floating Animated Voice Pill
- **Always-on-Top Minimal HUD**: Small floating indicator that stays above other windows with transparent glass design.
- **4 Visual States**:
  - **Listening**: Real-time equalizer bars reacting to your microphone audio volume.
  - **Transcribing**: Spinning ring while Whisper or Parakeet runs locally in RAM.
  - **Translating**: Pulsing violet glow displaying language direction (`Auto → English`).
  - **Done**: Quick emerald green checkmark as text pastes.
- **Draggable**: Drag the pill anywhere on your screen.

### 9. Automatic Text Paste
- **Direct App Insertion**: Copies translated text to system clipboard and simulates keyboard paste (`Cmd+V` on macOS, `Ctrl+V` on Windows/Linux) into your active cursor position.
- Works in any application: Slack, Telegram, WhatsApp, VS Code, Google Docs, Chrome, Terminal, and more.

### 10. Privacy & History Storage Controls
- **3 History Modes**:
  - *Text Only (Default)*: Stores only text transcripts in local SQLite database.
  - *Text + Audio*: Saves high-efficiency Opus voice recordings alongside text.
  - *Private Mode*: Stores everything in RAM only. Zero history is written to disk.
- **Auto-Pruning**: Automatic 500 MB storage cap (FIFO auto-cleanup) and 30-day retention schedule.

---

## Architecture

```
[ Microphone Stream ]
         │
         ▼ (16 kHz mono float32 into RAM)
[ Silero VAD ] ── (Trims silence, preserves 0.4s margins)
         │
         ▼
[ Local Speech Recognition ] ── (Parakeet TDT / Whisper Medium q5_0 in RAM)
         │
         ▼
[ Language & Skip Check ] ── (0.0001s: if already target language) ──► [ Direct Auto-Paste ]
         │ (If foreign speech)
         ▼
[ Confidentiality Masker ] ── (Hides emails, cards, and custom secrets)
         │
         ▼
[ AI Translation Engine ] ── (Google Gemini / Local Ollama / Custom API)
         │
         ▼
[ Auto-Paste (Cmd+V / Ctrl+V) ] + [ Local SQLite History ]
```

---

## Installation

See **[INSTALL.md](INSTALL.md)** for detailed installation steps.

### macOS Quick Install (1-Line Terminal Command)

Open Terminal and run this command to download, install, and clear gatekeeper quarantine automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/eotsevych/RevFly/main/scripts/install_mac.sh | bash
```

Or with [Homebrew](https://brew.sh):

```bash
brew install --cask eotsevych/tap/revfly
```

RevFly updates itself: you get a notification when a new version is out, and install it from the menu bar or Settings.

### Manual Downloads

| Platform | Recommended Installer | System Architecture |
|---|---|---|
| **macOS** | `RevFly_Universal.dmg` | Apple Silicon (M1–M4) & Intel (`x86_64`) |
| **Windows** | `RevFly_<version>_x64-setup.exe` or `.msi` | 64-bit Windows 10 & 11 |
| **Linux** | `RevFly_<version>_amd64.AppImage`, `.deb` or `.rpm` | 64-bit Linux (Ubuntu, Debian, Fedora) |

All installers are published on [GitHub Releases](https://github.com/eotsevych/RevFly/releases).

---

## Development & Build

### Prerequisites
- Node.js or [Bun](https://bun.sh)
- Rust & Cargo (1.77.2+)
- CMake and Clang (for the Whisper build)
- Linux only: the system packages listed in `scripts/build_linux.sh`

### Run in Development
```bash
# Install frontend packages
bun install

# Start desktop app in dev mode
bun run tauri dev
```

### Build Production Packages
```bash
# macOS: Universal, Apple Silicon and Intel DMGs (output: installers/)
bash scripts/build_installers.sh

# Windows: .exe and .msi installers
scripts\build_windows.bat

# Linux: .AppImage, .deb and .rpm packages
bash scripts/build_linux.sh
```

Each platform is built on its own OS. GitHub Actions builds all three on every push; see [DEPLOYMENT.md](DEPLOYMENT.md) for releases.

---

## License

This project is licensed under the [MIT License](LICENSE).
