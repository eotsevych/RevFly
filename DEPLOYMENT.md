# Deployment & Release Guide for Maintainers

This guide explains how to build, package, and publish RevFly for public distribution.

---

## 1. Where to Store Installers (Git vs. GitHub Releases)

### Do Not Store Installers Directly in Git

Do not commit `.dmg`, `.exe`, or `.deb` files into Git repository commits.

Here is why:
1. **File size limits**: GitHub blocks any file over 100 MB and warns at 50 MB.
2. **Repository bloat**: Git keeps every past copy of binary files forever. A 200 MB release makes the repository large, slow to clone, and difficult to manage.

### The Open-Source Solution: GitHub Releases

Use **GitHub Releases** (a GitHub service to host application download packages).

- **Free storage**: Unlimited download traffic.
- **Fast speed**: Delivered via GitHub CDN (Content Delivery Network - fast global file distribution).
- **Direct links**: Users download straight from your repository "Releases" page.

---

## 2. Developer Warnings (Gatekeeper & SmartScreen)

### Why Do Warnings Happen?

Operating systems (OS - the main computer software) show security warnings on downloaded files:
- **macOS Gatekeeper**: Warns if an app lacks a paid Apple Developer certificate ($99 per year) and Apple Notarization (Apple automated security review).
- **Windows SmartScreen**: Warns if an app lacks a paid Microsoft code signing certificate ($200 to $400 per year).

Being open-source does not stop these warnings automatically. The OS checks for a cryptographic certificate.

---

### Solutions for Open-Source Maintainers

#### Solution 1: Zero Cost (Document User Bypass - Standard for Open Source)
Most open-source tools (like Blender, Audacity, and Ollama when they started) use this method.

1. macOS ad-hoc signing:
   The build script runs:
   ```bash
   codesign --force --deep --sign - --identifier "com.revfly.desktop" "RevFly.app"
   ```
2. Inform users in [INSTALL.md](INSTALL.md) how to open the app:
   - **macOS**: Right-click the app -> Click **Open**, or run `xattr -cr "/Applications/RevFly.app"`.
   - **Windows**: Click **More info** -> Click **Run anyway**.

---

#### Solution 2: Free Certificate for Open Source (Windows)
For Windows, you can get free code signing through **SignPath.io**:
- SignPath provides free code signing certificates to approved open-source GitHub projects.
- It integrates with GitHub Actions CI/CD (Continuous Integration and Continuous Delivery).
- Sign up at: `https://signpath.io/about/open-source`

---

#### Solution 3: Official Paid Signing (Commercial Grade)
If you want zero dialogs and zero user warnings:

1. **For macOS**:
   - Buy an Apple Developer account ($99/year).
   - Create a "Developer ID Application" certificate in Xcode or Apple Developer Portal.
   - Sign the app with your certificate:
     ```bash
     codesign --sign "Developer ID Application: Your Name (ID)" "RevFly.app"
     ```
   - Send the DMG to Apple for Notarization:
     ```bash
     xcrun notarytool submit "RevFly_Universal.dmg" --keychain-profile "AC_PASSWORD" --wait
     xcrun stapler staple "RevFly_Universal.dmg"
     ```

2. **For Windows**:
   - Buy a standard code signing certificate (e.g. Sectigo or DigiCert) or use Azure Trusted Signing.
   - Sign the `.exe` and `.msi` in your build script:
     ```cmd
     signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 /f YourCert.pfx /p YourPassword installer.exe
     ```

---

## 3. How to Build Installers Locally

### On macOS (Universal, Apple Silicon & Intel)
Run the automated installer script:
```bash
bash scripts/build_installers.sh
```

This creates:
- `installers/RevFly_Universal.dmg` (Works on all Macs: M1-M4 and Intel)
- `installers/RevFly_Apple_Silicon_arm64.dmg` (Apple Silicon only)
- `installers/RevFly_Intel_x86_64.dmg` (Intel only)

### On Windows
Run the Windows build batch script:
```cmd
scripts\build_windows.bat
```
Output files:
- `src-tauri\target\release\bundle\nsis\RevFly_Setup.exe`
- `src-tauri\target\release\bundle\msi\RevFly.msi`

### On Linux
Run the Linux build script:
```bash
bash scripts/build_linux.sh
```
Output files:
- `src-tauri/target/release/bundle/appimage/RevFly.AppImage`
- `src-tauri/target/release/bundle/deb/revfly.deb`

---

## 4. How to Publish a Release (Step-by-Step)

### Option A: Manual First Release (Fastest - You Already Have the DMG)

Do not commit the `.dmg` into Git. Follow these exact steps:

#### Step 1: Push Your Code to GitHub
Run this in Terminal:
```bash
# Add your GitHub remote repository (if not added yet)
git remote add origin https://github.com/eugeneotsevich/RevFly.git

# Push the main code branch
git push -u origin main
```

#### Step 2: Locate Your Ready DMG File
You already have the signed universal DMG on your Desktop:
```
~/Desktop/RevFly_Universal.dmg
```
*(If you ever need to rebuild it, run: `bash scripts/build_installers.sh`)*.

#### Step 3: Create Release on GitHub
1. Open your repository on GitHub in a web browser.
2. Click **Releases** on the right side.
3. Click **Draft a new release**.
4. In **Choose a tag**, type: `v0.1.0` and click **Create new tag**.
5. Set the release title: `RevFly v0.1.0`.
6. Drag `RevFly_Universal.dmg` into the **Attach binaries** box.
7. Click **Publish release**.

Both your manual DMG download and the 1-line install command will now work immediately.

---

### Option B: Automated Cloud Release (GitHub Actions)

When you want GitHub servers to build Windows `.exe`, Linux, and macOS packages automatically:

#### Step 1: Push a Version Tag
```bash
git tag v0.1.0
git push origin v0.1.0
```

#### Step 2: Automated Cloud Compilation
The workflow in `.github/workflows/build-all-platforms.yml` starts automatically.

It compiles:
- macOS Universal DMG
- Windows `.exe` and `.msi`
- Linux `.AppImage` and `.deb`

#### Step 3: Review and Publish
1. Open GitHub -> **Releases**.
2. Click the new draft release created by GitHub Actions.
3. Review the attached installers.
4. Click **Publish release**.
