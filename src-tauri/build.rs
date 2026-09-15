use std::{env, fs, path::PathBuf};

/// Copies WinDivert.dll + WinDivert64.sys next to the built executable so that
/// `cargo tauri dev` finds the driver without any manual step.
fn copy_windivert() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("windivert");
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out  ->  target/<profile>
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out.ancestors().nth(3).unwrap().to_path_buf();
    for f in ["WinDivert.dll", "WinDivert64.sys"] {
        let from = src.join(f);
        let to = target_dir.join(f);
        if from.exists() {
            let _ = fs::copy(&from, &to);
        }
        println!("cargo:rerun-if-changed={}", from.display());
    }
}

fn main() {
    copy_windivert();

    let mut windows = tauri_build::WindowsAttributes::new();
    // Release builds require elevation through the manifest (like NetLimiter).
    // Debug builds self-elevate at runtime instead so `cargo tauri dev` works
    // from a normal terminal (cargo cannot spawn an exe that demands UAC).
    if env::var("PROFILE").map(|p| p == "release").unwrap_or(false) {
        windows = windows.app_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>"#,
        );
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri build script");
}
