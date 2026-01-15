//! Build script for openvox-webui
//!
//! This script automatically builds the frontend when OPENVOX_SERVE_FRONTEND=true
//! is set in the environment or .env file.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Tell Cargo to rerun this script if these files change
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/src/");

    // Load .env file if it exists
    let env_path = Path::new(".env");
    if env_path.exists() {
        if let Ok(contents) = fs::read_to_string(env_path) {
            for line in contents.lines() {
                let line = line.trim();
                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                // Parse KEY=VALUE
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    // Only set if not already in environment
                    if env::var(key).is_err() {
                        env::set_var(key, value);
                    }
                }
            }
        }
    }

    // Check if we should build the frontend
    let serve_frontend = env::var("OPENVOX_SERVE_FRONTEND")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if !serve_frontend {
        println!("cargo:warning=Skipping frontend build (OPENVOX_SERVE_FRONTEND != true)");
        return;
    }

    let frontend_dir = Path::new("frontend");
    let dist_dir = frontend_dir.join("dist");

    // Check if frontend directory exists
    if !frontend_dir.exists() {
        println!("cargo:warning=Frontend directory not found, skipping frontend build");
        return;
    }

    // Check if node_modules exists, run npm install if not
    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        println!("cargo:warning=Installing frontend dependencies...");
        let status = Command::new("npm")
            .arg("install")
            .current_dir(frontend_dir)
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("cargo:warning=Frontend dependencies installed successfully");
            }
            Ok(s) => {
                println!(
                    "cargo:warning=npm install failed with exit code: {:?}",
                    s.code()
                );
                return;
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm install: {}", e);
                println!("cargo:warning=Make sure npm is installed and in your PATH");
                return;
            }
        }
    }

    // Check if we need to rebuild
    // We rebuild if dist doesn't exist or if any source file is newer than dist
    let needs_rebuild = if !dist_dir.exists() {
        true
    } else {
        // Check if any source files are newer than dist/index.html
        let dist_index = dist_dir.join("index.html");
        if !dist_index.exists() {
            true
        } else {
            let dist_time = fs::metadata(&dist_index)
                .and_then(|m| m.modified())
                .ok();

            if let Some(dist_time) = dist_time {
                // Check if any file in frontend/src is newer
                check_dir_newer_than(&frontend_dir.join("src"), dist_time)
                    || check_file_newer_than(&frontend_dir.join("package.json"), dist_time)
                    || check_file_newer_than(&frontend_dir.join("index.html"), dist_time)
                    || check_file_newer_than(&frontend_dir.join("vite.config.ts"), dist_time)
                    || check_file_newer_than(&frontend_dir.join("tailwind.config.js"), dist_time)
            } else {
                true
            }
        }
    };

    if !needs_rebuild {
        println!("cargo:warning=Frontend is up to date, skipping build");
        return;
    }

    println!("cargo:warning=Building frontend...");

    let status = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(frontend_dir)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=Frontend built successfully");
        }
        Ok(s) => {
            // Don't fail the entire build, just warn
            println!(
                "cargo:warning=Frontend build failed with exit code: {:?}",
                s.code()
            );
            println!("cargo:warning=You may need to build the frontend manually: cd frontend && npm run build");
        }
        Err(e) => {
            println!("cargo:warning=Failed to run npm build: {}", e);
            println!("cargo:warning=Make sure npm is installed and in your PATH");
        }
    }
}

/// Check if any file in a directory is newer than the given time
fn check_dir_newer_than(dir: &Path, time: std::time::SystemTime) -> bool {
    if !dir.exists() {
        return false;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if check_dir_newer_than(&path, time) {
                    return true;
                }
            } else if check_file_newer_than(&path, time) {
                return true;
            }
        }
    }
    false
}

/// Check if a file is newer than the given time
fn check_file_newer_than(path: &Path, time: std::time::SystemTime) -> bool {
    if !path.exists() {
        return false;
    }

    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|file_time| file_time > time)
        .unwrap_or(false)
}
