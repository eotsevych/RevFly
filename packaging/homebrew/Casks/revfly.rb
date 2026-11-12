# Homebrew cask for RevFly, published through the tap repo github.com/eotsevych/homebrew-tap.
# Users install with:  brew install --cask eotsevych/tap/revfly
#
# After each release, run scripts/update_homebrew_cask.sh <version> to refresh version and sha256,
# then copy this file to Casks/revfly.rb in the tap repo (see DEPLOYMENT.md).
cask "revfly" do
  version "1.0.13"
  sha256 "31a47eea6b7ca74120414ee07d29bdfc9bd6bf1253ad6978a2bed4f57ba89dcf"

  url "https://github.com/eotsevych/RevFly/releases/download/v#{version}/RevFly_Universal.dmg"
  name "RevFly"
  desc "Local-first voice dictation and translation"
  homepage "https://eotsevych.github.io/RevFly/"

  # RevFly is now Bifaria; this cask gets no more updates.
  deprecate! date: "2026-11-12", because: "has been renamed to Bifaria", replacement_cask: "bifaria"

  livecheck do
    url :url
    strategy :github_latest
  end

  # The app updates itself in place; brew should not fight it.
  auto_updates true
  depends_on :macos

  app "RevFly.app"

  uninstall quit: "com.revfly.desktop"

  zap trash: [
    "~/Library/Application Support/com.revfly.desktop",
    "~/Library/Application Support/revfly",
    "~/Library/Caches/com.revfly.desktop",
    "~/Library/LaunchAgents/RevFly.plist",
    "~/Library/Preferences/com.revfly.desktop.plist",
    "~/Library/Saved Application State/com.revfly.desktop.savedState",
    "~/Library/WebKit/com.revfly.desktop",
  ]
end
