# Homebrew cask for RevFly, published through the tap repo github.com/eotsevych/homebrew-tap.
# Users install with:  brew install --cask eotsevych/tap/revfly
#
# After each release, run scripts/update_homebrew_cask.sh <version> to refresh version and sha256,
# then copy this file to Casks/revfly.rb in the tap repo (see DEPLOYMENT.md).
cask "revfly" do
  version "1.0.7"
  sha256 "9145c148946966cb08da222f734823c1dc041a813d23d018ac9977d8d0a92958"

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
