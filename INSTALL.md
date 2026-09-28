# Installation Guide for RevFly

This guide explains how to install RevFly on macOS, Windows, and Linux.

---

## 1. macOS (Apple Silicon M-series & Intel)

You have two easy ways to install RevFly on macOS.

### Option A: Quick 1-Line Terminal Install (Recommended)

This command downloads the latest app, installs it to Applications, and clears the macOS quarantine flag automatically. 

You will see **zero unknown developer warnings**.

Open Terminal and run:
```bash
curl -fsSL https://raw.githubusercontent.com/eugeneotsevich/RevFly/main/scripts/install_mac.sh | bash
```

---

### Option B: Manual DMG Download

If you prefer downloading manually:

1. Download **`RevFly_Universal.dmg`** (works on M1–M4 Apple Silicon and Intel) from [GitHub Releases](https://github.com/eugeneotsevich/RevFly/releases).
2. Double-click the `.dmg` file to open it.
3. Drag **RevFly** into your **Applications** folder.

#### How to Open Without "Unknown Developer" Warning

When downloaded with a browser, macOS marks the file with a quarantine tag.

To open the app:
1. Open your **Applications** folder in Finder.
2. **Right-click** (or hold `Control` and click) **RevFly.app**.
3. Click **Open** from the menu.
4. Click the **Open** button in the dialog.
5. You only need to do this once. After this, open the app normally.

*Alternative (Terminal command)*:
```bash
xattr -cr "/Applications/RevFly.app"
```

---

## 2. Windows 10 & 11

### Download
Download `RevFly_Setup.exe` (installer executable) from [GitHub Releases](https://github.com/eugeneotsevich/RevFly/releases).

### Installation Steps
1. Double-click the downloaded `.exe` file.
2. Follow the setup steps on screen.

### How to Pass the "SmartScreen" Warning
Microsoft SmartScreen (a Windows security scanner) flags new open-source software:
1. When the blue "Windows protected your PC" window appears, click **More info**.
2. Click **Run anyway**.
3. You only need to do this once.

---

## 3. Linux (Ubuntu, Debian, Fedora, Arch)

Linux does not show developer warnings.

### Option A: AppImage (All Linux distributions)
1. Download `RevFly.AppImage`.
2. Make it runnable:
   ```bash
   chmod +x RevFly.AppImage
   ./RevFly.AppImage
   ```

### Option B: Debian / Ubuntu package (.deb)
1. Download `revfly.deb`.
2. Install with your package manager:
   ```bash
   sudo dpkg -i revfly.deb
   sudo apt-get install -f
   ```

---

## 4. Permissions Setup

RevFly needs two permissions:
1. **Microphone**: Required to capture your voice.
2. **Accessibility**: Required to read the global hotkey and paste text.

On first launch, your operating system will ask you to approve these permissions.
