const REPO = "eotsevych/RevFly";
const RELEASES_URL = `https://github.com/${REPO}/releases/latest`;

const INSTALL_COMMANDS = {
  curl: `curl -fsSL https://raw.githubusercontent.com/${REPO}/main/scripts/install_mac.sh | bash`,
  brew: "brew install --cask eotsevych/tap/revfly",
};

// ── Language (English is in the HTML; others come from i18n.js) ─────────────
const DICTS = window.REVFLY_I18N || {};
const englishText = new Map(); // element → original innerHTML
const englishAttrs = new Map(); // element → { attr: original value }
const englishMeta = {
  title: document.title,
  description: document.querySelector('meta[name="description"]')?.content || "",
};

// English fallbacks for strings that only exist in JS.
const EN = {
  "ui.copy": "Copy",
  "ui.copied": "Copied",
  "ui.selectCopy": "Select & copy",
  "hero.download": "Download RevFly",
  "hero.downloadFor": "Download for {os}",
  "dl.latestTag": "Latest: {tag}",
  "demo.listening": "Listening",
  "demo.toEnglish": "Translating to English",
  "demo.transcribing": "Transcribing",
  "demo.onDevice": "Parakeet · on-device",
  "demo.translating": "Translating",
  "demo.direction": "Ukrainian → English",
  "demo.done": "Done",
  "demo.pastedIn": "Pasted in English",
};

let lang = document.documentElement.dataset.lang === "uk" ? "uk" : "en";

function t(key, vars = {}) {
  const text = (lang !== "en" && DICTS[lang]?.[key]) || EN[key] || key;
  return text.replace(/\{(\w+)\}/g, (_, name) => vars[name] ?? "");
}

function applyLanguage(next) {
  lang = DICTS[next] ? next : "en";
  const dict = lang === "en" ? null : DICTS[lang];
  const root = document.documentElement;
  root.lang = lang;
  root.dataset.lang = lang;

  document.querySelectorAll("[data-i18n]").forEach((el) => {
    if (!englishText.has(el)) englishText.set(el, el.innerHTML);
    const value = dict?.[el.dataset.i18n];
    el.innerHTML = value ?? englishText.get(el);
  });

  document.querySelectorAll("[data-i18n-attr]").forEach((el) => {
    if (!englishAttrs.has(el)) englishAttrs.set(el, {});
    const saved = englishAttrs.get(el);
    el.dataset.i18nAttr.split(";").forEach((pair) => {
      const [attr, key] = pair.split(":");
      if (!(attr in saved)) saved[attr] = el.getAttribute(attr);
      el.setAttribute(attr, dict?.[key] ?? saved[attr]);
    });
  });

  document.title = dict?.["meta.title"] ?? englishMeta.title;
  document
    .querySelector('meta[name="description"]')
    ?.setAttribute("content", dict?.["meta.description"] ?? englishMeta.description);

  document.querySelectorAll(".lang-switch [data-lang]").forEach((b) => {
    b.setAttribute("aria-pressed", String(b.dataset.lang === lang));
  });

  updateDynamicText();
  root.classList.remove("i18n-pending");
}

document.querySelectorAll(".lang-switch [data-lang]").forEach((button) => {
  button.addEventListener("click", () => {
    applyLanguage(button.dataset.lang);
    try {
      localStorage.setItem("revfly_lang", lang);
    } catch {
      // Storage blocked: the choice lasts for this visit only.
    }
    const url = new URL(location.href);
    url.searchParams.set("lang", lang);
    history.replaceState(null, "", url);
  });
});

// ── Copy buttons ─────────────────────────────────────────────────────────────
document.querySelectorAll(".copy").forEach((button) => {
  button.addEventListener("click", async () => {
    const target = document.getElementById(button.dataset.copyTarget);
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.textContent.trim());
      button.textContent = t("ui.copied");
    } catch {
      button.textContent = t("ui.selectCopy");
    }
    setTimeout(() => (button.textContent = t("ui.copy")), 1600);
  });
});

// ── Install tabs (Terminal / Homebrew) ───────────────────────────────────────
const installCode = document.getElementById("install-code");
document.querySelectorAll(".install-tabs [role=tab]").forEach((tab) => {
  tab.addEventListener("click", () => {
    document
      .querySelectorAll(".install-tabs [role=tab]")
      .forEach((x) => x.setAttribute("aria-selected", String(x === tab)));
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
if (os) document.querySelector(`.dl[data-os="${os}"]`)?.classList.add("is-current");
let releaseTag = null;

// Text that depends on runtime state (OS, release) as well as the language.
function updateDynamicText() {
  const heroLabel = document.getElementById("hero-download-label");
  heroLabel.textContent = os ? t("hero.downloadFor", { os: OS_NAMES[os] }) : t("hero.download");
  if (releaseTag) {
    document.getElementById("release-version").textContent = t("dl.latestTag", { tag: releaseTag });
  }
  if (demoState) setPill(...demoState);
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
    if (release.tag_name) {
      releaseTag = release.tag_name;
      updateDynamicText();
    }
    if (os && urls[HERO_ASSET[os]]) {
      document.getElementById("hero-download").href = urls[HERO_ASSET[os]];
    }
  } catch {
    // Offline or rate-limited: links keep pointing at the releases page.
  }
}

document.querySelectorAll("[data-asset]").forEach((link) => (link.href = RELEASES_URL));

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
let demoState = null; // [state, labelKey, subKey], re-rendered on language change

function setPill(state, labelKey, subKey) {
  demoState = [state, labelKey, subKey];
  pill.dataset.state = state;
  pillLabel.textContent = t(labelKey);
  pillSub.textContent = t(subKey);
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
  setPill("done", "demo.done", "demo.pastedIn");
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
    setPill("listening", "demo.listening", "demo.toEnglish");
    await sleep(2600);

    spoken.classList.add("is-hidden");
    setPill("transcribing", "demo.transcribing", "demo.onDevice");
    await sleep(1300);

    setPill("translating", "demo.translating", "demo.direction");
    await sleep(1300);

    setPill("done", "demo.done", "demo.pastedIn");
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

applyLanguage(lang);
loadRelease();

if (reducedMotion) {
  showFinalFrame();
} else {
  runDemo();
}
