fn main() {
    ensure_sidecar_launcher();
    tauri_build::build();
}

fn ensure_sidecar_launcher() {
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
    );
    let sidecar_dir = manifest_dir.join("sidecar");
    let _ = std::fs::create_dir_all(&sidecar_dir);

    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let output = sidecar_dir.join(format!("main-{}{}", target, exe_suffix));
    if output.exists() {
        return;
    }

    let source = sidecar_dir.join("main_launcher.rs");
    let source_code = r#"
use std::{env, path::PathBuf, process::Command};

fn main() {
    let script = find_script().unwrap_or_else(|| PathBuf::from("sidecar").join("main.py"));
    let status = Command::new("python")
        .arg(&script)
        .status()
        .or_else(|_| Command::new("py").arg(&script).status())
        .expect("failed to start Python sidecar");
    std::process::exit(status.code().unwrap_or(1));
}

fn find_script() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("main.py"));
            candidates.push(dir.join("..").join("sidecar").join("main.py"));
            candidates.push(dir.join("..").join("..").join("sidecar").join("main.py"));
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("sidecar").join("main.py"));
        candidates.push(cwd.join("..").join("sidecar").join("main.py"));
    }
    candidates.into_iter().find(|path| path.exists())
}
"#;

    if std::fs::write(&source, source_code).is_err() {
        return;
    }

    let _ = std::process::Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .status();
}
