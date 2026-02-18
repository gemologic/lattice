use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/package.json");
    println!("cargo:rerun-if-changed=ui/bun.lock");
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");

    if env::var("LATTICE_SKIP_UI_BUILD").ok().as_deref() == Some("1") {
        create_placeholder_dist();
        return;
    }

    if !Path::new("ui/package.json").exists() {
        create_placeholder_dist();
        return;
    }

    run_bun_install();
    run_bun_build();
}

fn run_bun_install() {
    let lock_file_exists = Path::new("ui/bun.lock").exists();

    let status = if lock_file_exists {
        Command::new("bun")
            .arg("install")
            .arg("--frozen-lockfile")
            .current_dir("ui")
            .status()
    } else {
        Command::new("bun")
            .arg("install")
            .current_dir("ui")
            .status()
    };

    match status {
        Ok(exit_status) if exit_status.success() => {}
        Ok(exit_status) => {
            panic!("bun install failed with status: {exit_status}");
        }
        Err(error) => {
            panic!("failed to run bun install: {error}");
        }
    }
}

fn run_bun_build() {
    let status = Command::new("bun")
        .arg("run")
        .arg("build")
        .current_dir("ui")
        .status();

    match status {
        Ok(exit_status) if exit_status.success() => {}
        Ok(exit_status) => {
            panic!("bun build failed with status: {exit_status}");
        }
        Err(error) => {
            panic!("failed to run bun build: {error}");
        }
    }
}

fn create_placeholder_dist() {
    let dist_dir = Path::new("ui/dist");

    if let Err(error) = fs::create_dir_all(dist_dir) {
        panic!("failed to create placeholder dist directory: {error}");
    }

    let placeholder = r#"<!doctype html>
<html>
  <head>
    <meta charset=\"utf-8\" />
    <title>lattice</title>
  </head>
  <body>
    <main>
      <h1>Lattice</h1>
      <p>UI bundle has not been built yet.</p>
    </main>
  </body>
</html>
"#;

    if let Err(error) = fs::write(dist_dir.join("index.html"), placeholder) {
        panic!("failed to write placeholder index.html: {error}");
    }
}
