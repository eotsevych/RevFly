use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
  build_apple_translate();
  tauri_build::build()
}

/// Builds revfly-translate, the helper for the "Apple" translation provider, which
/// tauri.macos.conf.json bundles as a sidecar (`binaries/revfly-translate-<target triple>`). The
/// helper is universal, so one build serves every macOS target.
///
/// It needs the macOS 26 SDK. Without it the build goes on with a stand-in that answers every
/// request with an error, and the Apple provider fails with that message; set
/// REVFLY_REQUIRE_APPLE_TRANSLATE=1 (release builds do) to fail the build instead.
fn build_apple_translate() {
  let target = std::env::var("TARGET").unwrap_or_default();
  if !target.ends_with("apple-darwin") {
    return;
  }
  let source = Path::new("apple-translate/revfly-translate.swift");
  let script = Path::new("../scripts/build_apple_translate.sh");
  println!("cargo:rerun-if-changed={}", source.display());
  println!("cargo:rerun-if-changed={}", script.display());
  println!("cargo:rerun-if-env-changed=REVFLY_REQUIRE_APPLE_TRANSLATE");

  let universal = PathBuf::from("binaries/revfly-translate-universal-apple-darwin");
  let sidecar = PathBuf::from(format!("binaries/revfly-translate-{}", target));
  if !is_newer(&universal, &[source, script]) || is_stand_in(&universal) {
    let built = Command::new("bash").arg(script).arg(&universal).status().map(|s| s.success()).unwrap_or(false);
    if !built {
      if std::env::var_os("REVFLY_REQUIRE_APPLE_TRANSLATE").is_some() {
        panic!("Couldn't build the Apple Translation helper (scripts/build_apple_translate.sh); it needs Xcode with the macOS 26 SDK");
      }
      println!("cargo:warning=Couldn't build the Apple Translation helper (needs Xcode with the macOS 26 SDK); the Apple translation provider won't work in this build.");
      write_stand_in(&universal);
    }
  }
  if sidecar != universal {
    std::fs::copy(&universal, &sidecar).expect("copy the Apple Translation helper");
  }
}

const STAND_IN_MARK: &str = "revfly-translate stand-in";

/// Answers every request (and a download) with an error saying why the helper is missing.
fn write_stand_in(path: &Path) {
  let message = "This RevFly build has no Apple Translation helper (it was built without the macOS 26 SDK)";
  let script = format!(
    "#!/bin/sh\n# {STAND_IN_MARK}\nanswer='{{\"ok\":false,\"error\":\"internal\",\"message\":\"{message}\"}}'\nif [ \"$1\" = download ]; then echo \"$answer\"; exit 1; fi\nwhile read -r _; do echo \"$answer\"; done\n"
  );
  std::fs::create_dir_all(path.parent().unwrap()).expect("create src-tauri/binaries");
  std::fs::write(path, script).expect("write the Apple Translation helper stand-in");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
  }
}

fn is_stand_in(path: &Path) -> bool {
  std::fs::read(path).map(|b| b.starts_with(b"#!") && String::from_utf8_lossy(&b).contains(STAND_IN_MARK)).unwrap_or(false)
}

/// True when `path` exists and is newer than every input.
fn is_newer(path: &Path, inputs: &[&Path]) -> bool {
  let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
  let Some(built) = modified(path) else { return false };
  inputs.iter().all(|input| modified(input).map_or(true, |m| m <= built))
}
