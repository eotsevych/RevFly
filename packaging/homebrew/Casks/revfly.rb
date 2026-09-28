# Homebrew cask for RevFly, published through the tap repo github.com/eotsevych/homebrew-tap.
# Users install with:  brew install --cask eotsevych/tap/revfly
#
# After each release, run scripts/update_homebrew_cask.sh <version> to refresh version and sha256,
# then copy this file to Casks/revfly.rb in the tap repo (see DEPLOYMENT.md).
cask "revfly" do
  version "0.1.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/eotsevych/RevFly/releases/download/v#{version}/RevFly_Universal.dmg"
  name "RevFly"
  desc "Local-first voice dictation and translation"
  homepage "https://eotsevych.github.io/RevFly/"

  livecheck do
    url :url
    strategy :github_latest
  end

  # The app updates itself in place; brew should not fight it.
  auto_updates true
  depends_on macos: ">= :big_sur"

  app "RevFly.app"

  # RevFly is not notarized (no paid Apple Developer ID), so drop the quarantine flag
  # Homebrew adds; otherwise Gatekeeper blocks the first launch.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/RevFly.app"]
  end

  uninstall quit: "com.revfly.desktop"

  zap trash: [
    "~/Library/Application Support/revfly",
    "~/Library/Application Support/com.revfly.desktop",
    "~/Library/Caches/com.revfly.desktop",
    "~/Library/Preferences/com.revfly.desktop.plist",
    "~/Library/Saved Application State/com.revfly.desktop.savedState",
    "~/Library/WebKit/com.revfly.desktop",
  ]
end
