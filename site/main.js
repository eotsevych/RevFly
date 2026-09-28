const REPO = "eotsevych/RevFly";
const RELEASES_URL = `https://github.com/${REPO}/releases/latest`;

const INSTALL_COMMANDS = {
  curl: `curl -fsSL https://raw.githubusercontent.com/${REPO}/main/scripts/install_mac.sh | bash`,
  brew: "brew install --cask eotsevych/tap/revfly",
};

// ── Copy buttons ─────────────────────────────────────────────────────────────
document.querySelectorAll(".copy").forEach((button) => {
  button.addEventListener("click", async () => {
    const target = document.getElementById(button.dataset.copyTarget);
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.textContent.trim());
      button.textContent = "Copied";
    } catch {
      button.textContent = "Select & copy";
    }
    setTimeout(() => (button.textContent = "Copy"), 1600);
  });
});

// ── Install tabs (Terminal / Homebrew) ───────────────────────────────────────
const installCode = document.getElementById("install-code");
document.querySelectorAll(".install-tabs [role=tab]").forEach((tab) => {
  tab.addEventListener("click", () => {
    document
      .querySelectorAll(".install-tabs [role=tab]")
      .forEach((t) => t.setAttribute("aria-selected", String(t === tab)));
    installCode.textContent = INSTALL_COMMANDS[tab.dataset.cmd];
  });
});

// ── Platform detection ───────────────────────────────────────────────────────
function detectOS() {
  const platform = (navigator.userAgentData?.platform || navigator.platform || "").toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  if (/iphone|ipad|android/.test(ua)) return null;
  if (platform.includes("mac") || ua.includes("mac os")) return "mac";
  if (platform.includes("win") || ua.includes("windows")) return "windows";
  if (platform.includes("linux") || ua.includes("linux")) return "linux";
  return null;
}

const OS_NAMES = { mac: "macOS", windows: "Windows", linux: "Linux" };
const os = detectOS();
if (os) {
  document.getElementById("hero-download-label").textContent = `Download for ${OS_NAMES[os]}`;
  document.querySelector(`.dl[data-os="${os}"]`)?.classList.add("is-current");
}

// ── Latest release assets ────────────────────────────────────────────────────
const ASSET_PATTERNS = {
  "mac-dmg": /^RevFly_Universal\.dmg$/,
  "win-exe": /_x64-setup\.exe$/,
  "win-msi": /_x64_en-US\.msi$/,
  "linux-appimage": /_amd64\.AppImage$/,
  "linux-deb": /_amd64\.deb$/,
  "linux-rpm": /\.x86_64\.rpm$/,
};
const HERO_ASSET = { mac: "mac-dmg", windows: "win-exe", linux: "linux-appimage" };

async function loadRelease() {
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return;
    const release = await res.json();
    const urls = {};
    for (const [key, pattern] of Object.entries(ASSET_PATTERNS)) {
      const asset = release.assets?.find((a) => pattern.test(a.name));
      if (asset) urls[key] = asset.browser_download_url;
    }
    document.querySelectorAll("[data-asset]").forEach((link) => {
      const url = urls[link.dataset.asset];
      if (url) link.href = url;
    });
    const version = document.getElementById("release-version");
    if (release.tag_name) version.textContent = `Latest: ${release.tag_name}`;
    if (os && urls[HERO_ASSET[os]]) {
      document.getElementById("hero-download").href = urls[HERO_ASSET[os]];
    }
  } catch {
    // Offline or rate-limited: links keep pointing at the releases page.
  }
}

document.querySelectorAll("[data-asset]").forEach((link) => (link.href = RELEASES_URL));
loadRelease();

// ── Hero demo: speak in Ukrainian, paste in English ──────────────────────────
const pill = document.getElementById("demo-pill");
const pillLabel = document.getElementById("pill-label");
const pillSub = document.getElementById("pill-sub");
const spoken = document.getElementById("demo-spoken");
const typed = document.getElementById("demo-typed");
const placeholder = document.getElementById("demo-placeholder");
const reply = document.getElementById("demo-reply");
const replyText = document.getElementById("demo-reply-text");

const RESULT = "We can show the demo on Thursday at 3 pm, okay?";
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function setPill(state, label, sub) {
  pill.dataset.state = state;
  pillLabel.textContent = label;
  pillSub.textContent = sub;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function typeText(text) {
  placeholder.hidden = true;
  for (let i = 1; i <= text.length; i++) {
    typed.textContent = text.slice(0, i);
    await sleep(22);
  }
}

function showFinalFrame() {
  spoken.classList.add("is-hidden");
  setPill("done", "Done", "Pasted into Slack");
  placeholder.hidden = true;
  reply.hidden = false;
  replyText.textContent = RESULT;
}

async function runDemo() {
  for (;;) {
    reply.hidden = true;
    typed.textContent = "";
    placeholder.hidden = false;
    spoken.classList.remove("is-hidden");
    setPill("listening", "Listening", "Hold ⌥ to talk");
    await sleep(2600);

    spoken.classList.add("is-hidden");
    setPill("transcribing", "Transcribing", "Whisper · on-device");
    await sleep(1300);

    setPill("translating", "Translating", "Ukrainian → English");
    await sleep(1300);

    setPill("done", "Done", "Pasted");
    await typeText(RESULT);
    await sleep(700);

    // "Send" the message, then start over.
    typed.textContent = "";
    reply.hidden = false;
    replyText.textContent = RESULT;
    placeholder.hidden = false;
    await sleep(3200);
  }
}

if (reducedMotion) {
  showFinalFrame();
} else {
  runDemo();
}
