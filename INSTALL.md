# Installation Guide for RevFly

This guide explains how to install RevFly on macOS, Windows, and Linux.

---

## 1. macOS (Apple Silicon M-series & Intel)

RevFly is free and open source, so it is not notarized by Apple (that requires a paid Apple Developer account).
Options A and B install it without any "unidentified developer" warning. Option C works too, with one extra step.

### Option A: Quick 1-Line Terminal Install (Recommended)

This command downloads the latest app, installs it to Applications, and clears the macOS quarantine flag automatically.

You will see **zero unknown developer warnings**.

Open Terminal and run:
```bash
curl -fsSL https://raw.githubusercontent.com/eotsevych/RevFly/main/scripts/install_mac.sh | bash
```

---

### Option B: Homebrew

If you use [Homebrew](https://brew.sh) (a popular Mac package manager):
```bash
brew install --cask eotsevych/tap/revfly
```

To uninstall later: `brew uninstall --cask revfly` (add `--zap` to also remove settings and models).

---

### Option C: Manual DMG Download

1. Download **`RevFly_Universal.dmg`** (works on M1–M4 Apple Silicon and Intel) from [GitHub Releases](https://github.com/eotsevych/RevFly/releases).
2. Double-click the `.dmg` file to open it.
3. Drag **RevFly** into your **Applications** folder.

#### First Launch: Allow RevFly to Open

When downloaded with a browser, macOS blocks apps that are not notarized by Apple. You need to allow RevFly once:

**macOS 15 Sequoia and newer:**
1. Open **RevFly** from Applications. macOS says it cannot verify the app. Click **Done**.
2. Open **System Settings** → **Privacy & Security**.
3. Scroll down to the **Security** section. Next to *"RevFly" was blocked*, click **Open Anyway**.
4. Confirm with your password or Touch ID, then click **Open Anyway** again.

**macOS 11 Big Sur to 14 Sonoma:**
1. Open your **Applications** folder in Finder.
2. **Right-click** (or hold `Control` and click) **RevFly.app** and choose **Open**.
3. Click **Open** in the dialog.

You only need to do this once. After this, open the app normally.

*Alternative (Terminal command, any macOS version)*:
```bash
xattr -cr "/Applications/RevFly.app"
```

---

### Updates

RevFly updates itself. It checks once a day, and when a new version is out you get a notification. Install it in either place:
- **Menu bar icon** → **Install Update & Restart**
- **Settings** → **General** → **Updates** → **Install & Restart** (with a download progress bar)

You can also check at any time with **Check for Updates** in either place.

Updates installed this way do not show the Gatekeeper warning again, and RevFly keeps its Microphone and Accessibility permissions.

---

## 2. Windows 10 & 11

### Download
Download `RevFly_<version>_x64-setup.exe` (installer) or `RevFly_<version>_x64_en-US.msi` from [GitHub Releases](https://github.com/eotsevych/RevFly/releases).

### Installation Steps
1. Double-click the downloaded `.exe` file.
2. Follow the setup steps on screen.

### How to Pass the "SmartScreen" Warning
Microsoft SmartScreen (a Windows security scanner) flags new open-source software:
1. When the blue "Windows protected your PC" window appears, click **More info**.
2. Click **Run anyway**.
3. You only need to do this once.

### Updates
RevFly checks for updates once a day and shows a notification when a new version is out. Install it from the tray icon (**Install Update & Restart**) or **Settings** → **General** → **Updates**. No need to download new installers by hand.

---

## 3. Linux (Ubuntu, Debian, Fedora, Arch)

Linux does not show developer warnings. An X11 session is recommended: Wayland restricts global hotkeys and simulated paste.

Requires a recent distribution (glibc 2.39 or newer): Ubuntu 24.04+, Debian 13+, Fedora 40+ or a current rolling release such as Arch.

### Option A: AppImage (All Linux distributions)
1. Download `RevFly_<version>_amd64.AppImage`.
2. Make it runnable:
   ```bash
   chmod +x RevFly_*_amd64.AppImage
   ./RevFly_*_amd64.AppImage
   ```

### Option B: Debian / Ubuntu package (.deb)
1. Download `RevFly_<version>_amd64.deb`.
2. Install with your package manager:
   ```bash
   sudo apt install ./RevFly_*_amd64.deb
   ```

### Option C: Fedora / RHEL package (.rpm)
1. Download `RevFly-<version>-1.x86_64.rpm`.
2. Install it:
   ```bash
   sudo dnf install ./RevFly-*.x86_64.rpm
   ```

### Updates
The AppImage updates itself (tray icon or **Settings** → **General** → **Updates**). For `.deb` and `.rpm`, install the new package from GitHub Releases.

---

## 4. Permissions Setup

1. **Microphone** (all platforms): Required to capture your voice.
2. **Accessibility** (macOS only): Required to read the global hotkey and paste text.

On first launch, your operating system will ask you to approve these permissions.
On macOS, permissions carry over to future updates, so you grant them only once.

### Hotkeys
- **macOS**: Single modifier keys (`Right Option`, `Right Control`) or key combos such as `⌘+Shift+Space`.
- **Windows & Linux**: Key combos only. The default is `Ctrl+Shift+Space`.
