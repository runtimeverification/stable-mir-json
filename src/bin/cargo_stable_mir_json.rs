use std::env;

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 2 {
        bail!("Usage: cargo run --bin cargo_stable_mir_json -- <PATH-TO-STABLE-MIR-JSON-REPO>")
    }
    let repo_path: PathBuf = args[1].clone().into();
    if !repo_path.is_dir() {
        bail!(
            "Provided should be path to stable_mir_json repo, but {} is not a dir",
            repo_path.display()
        );
    }
    setup(repo_path)
}

fn smir_json_dir() -> Result<PathBuf> {
    let home_dir = home::home_dir().expect("couldn't find home directory");
    if !home_dir.is_dir() {
        bail!(
            "got home directory `{}` which isn't a directory",
            home_dir.display()
        );
    }
    let smir_json_dir = home_dir.join(".stable_mir_json");
    Ok(smir_json_dir)
}

fn setup(repo_dir: PathBuf) -> Result<()> {
    let smir_json_dir = smir_json_dir()?;
    println!("Creating {} directory", smir_json_dir.display());
    std::fs::create_dir(&smir_json_dir)?;

    copy_libs(repo_dir, smir_json_dir)?;

    Ok(())
}

fn copy_libs(repo_dir: PathBuf, smir_json_dir: PathBuf) -> Result<()> {
    let dev_dir = repo_dir.join("target/debug/");
    let dev_rlib = dev_dir.join("libstable_mir_json.rlib");

    let release_dir = repo_dir.join("target/release/");
    let release_rlib = release_dir.join("libstable_mir_json.rlib");

    if !dev_rlib.exists() && !release_rlib.exists() {
        bail!(
            "Neither dev rlib `{}`, nor release rlib `{}` exists",
            dev_dir.display(),
            release_dir.display()
        );
    }

    // Debug
    if dev_rlib.exists() {
        cp_rlibs_from_profile(dev_dir, &smir_json_dir, Profile::Dev)?;
    }

    // Release
    if release_rlib.exists() {
        cp_rlibs_from_profile(release_dir, &smir_json_dir, Profile::Release)?;
    }

    Ok(())
}

enum Profile {
    Dev,
    Release,
}

impl Profile {
    fn folder_as_string(&self) -> String {
        match self {
            Profile::Dev => "debug/".into(),
            Profile::Release => "release/".into(),
        }
    }
}

fn cp_rlibs_from_profile(
    profile_dir: PathBuf,
    smir_json_dir: &Path,
    profile: Profile,
) -> Result<()> {
    let rlib = profile_dir.join("libstable_mir_json.rlib");
    // Stable MIR JSON rlib
    let smir_json_profile_dir = smir_json_dir.join(profile.folder_as_string());
    std::fs::create_dir(&smir_json_profile_dir)?;

    let smir_json_profile_rlib = smir_json_profile_dir.join("libstable_mir_json.rlib");
    println!(
        "Copying {} to {}",
        rlib.display(),
        smir_json_profile_rlib.display()
    );
    std::fs::copy(rlib, smir_json_profile_rlib)?;

    // Deps
    let smir_json_profile_deps_dir = smir_json_profile_dir.join("deps/");
    std::fs::create_dir(&smir_json_profile_deps_dir)?;

    let profile_deps_dir = profile_dir.join("deps/");
    if let Ok(entries) = std::fs::read_dir(profile_deps_dir) {
        for file in entries.flatten() {
            let file_path = file.path();

            if !file_path.is_file() {
                continue;
            }

            if let Some(ext) = file_path.extension() {
                if ext == "rlib" {
                    let smir_json_profile_deps_rlib =
                        smir_json_profile_deps_dir.join(file_path.file_name().unwrap());
                    println!(
                        "Copying {} to {}",
                        file_path.display(),
                        smir_json_profile_deps_rlib.display()
                    );
                    std::fs::copy(file_path, smir_json_profile_deps_rlib)?;
                }
            }
        }
    }

    Ok(())
}
