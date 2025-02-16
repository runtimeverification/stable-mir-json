use std::env;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();

    let (repo_dir, maybe_user_provided_dir): (PathBuf, Option<PathBuf>) = match &args[..] {
        [_, repo_dir] => (repo_dir.into(), None),
        [_, repo_dir, user_provided_dir] => (repo_dir.into(), Some(user_provided_dir.into())),
        _ => bail!("Usage: cargo run --bin cargo_stable_mir_json -- <PATH-TO-STABLE-MIR-JSON-REPO> [OPTIONAL-PATH-TO-CREATE-BUILD-DIR]"),
    };

    if !repo_dir.is_dir() {
        bail!(
            "Provided should be path to stable_mir_json repo, but {} is not a dir",
            repo_dir.display()
        );
    }

    if let Some(ref user_provided_dir) = maybe_user_provided_dir {
        if !user_provided_dir.is_dir() {
            bail!(
                "Provided should be path to create the .stable_mir_json dir, but {} is not a dir",
                user_provided_dir.display()
            );
        }
    }

    setup(repo_dir, maybe_user_provided_dir)
}

fn setup(repo_dir: PathBuf, maybe_user_provided_dir: Option<PathBuf>) -> Result<()> {
    let smir_json_dir = smir_json_dir(maybe_user_provided_dir)?;
    println!("Creating {} directory", smir_json_dir.display());
    std::fs::create_dir(&smir_json_dir)?; // This errors is the directory already exists

    let ld_library_path = record_ld_library_path(&smir_json_dir)?;
    copy_artefacts(&repo_dir, &smir_json_dir, &ld_library_path)

}

fn smir_json_dir(maybe_user_provided_dir: Option<PathBuf>) -> Result<PathBuf> {
    let user_provided_dir = match maybe_user_provided_dir {
        Some(user_provided_dir) => user_provided_dir,
        None => home::home_dir().expect("couldn't find home directory"),
    };
    if !user_provided_dir.is_dir() {
        bail!(
            // We know this is home because main checked user_provided_dir already
            "got home directory `{}` which isn't a directory",
            user_provided_dir.display()
        );
    }
    let smir_json_dir = user_provided_dir.join(".stable_mir_json");
    Ok(smir_json_dir)
}

fn copy_artefacts(repo_dir: &Path, smir_json_dir: &Path, ld_library_path: &Path) -> Result<()> {
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
        cp_artefacts_from_profile(smir_json_dir, Profile::Dev(repo_dir))?;
        add_run_script(&smir_json_dir, &ld_library_path, Profile::Dev(repo_dir))?;
    }

    // Release
    if release_rlib.exists() {
        cp_artefacts_from_profile(smir_json_dir, Profile::Release(repo_dir))?;
        add_run_script(&smir_json_dir, &ld_library_path, Profile::Release(repo_dir))?;
    }

    Ok(())
}

enum Profile<'a> {
    Dev(&'a Path),
    Release(&'a Path),
}

impl Profile<'_> {
    fn name(&self) -> String {
        match self {
            Profile::Dev(_) => "debug".into(),
            Profile::Release(_) => "release".into(),
        }
    }

    fn profile_dir(&self) -> PathBuf {
        match self {
            Profile::Dev(repo_dir) => repo_dir.join("target/debug/"),
            Profile::Release(repo_dir) => repo_dir.join("target/release/"),
        }
    }
}

fn cp_artefacts_from_profile(smir_json_dir: &Path, profile: Profile) -> Result<()> {
    let rlib = profile.profile_dir().join("libstable_mir_json.rlib");
    let bin = profile.profile_dir().join("stable_mir_json");

    // Stable MIR JSON bin and rlib
    let smir_json_profile_dir = smir_json_dir.join(format!("{}/", profile.name()));
    std::fs::create_dir(&smir_json_profile_dir)?;

    let smir_json_profile_rlib = smir_json_profile_dir.join("libstable_mir_json.rlib");
    println!(
        "Copying {} to {}",
        rlib.display(),
        smir_json_profile_rlib.display()
    );
    std::fs::copy(rlib, smir_json_profile_rlib)?;

    let smir_json_profile_bin = smir_json_profile_dir.join("stable_mir_json");
    println!(
        "Copying {} to {}",
        bin.display(),
        smir_json_profile_bin.display()
    );
    std::fs::copy(bin, smir_json_profile_bin)?;

    Ok(())
}

fn add_run_script(smir_json_dir: &Path, ld_library_path: &Path, profile: Profile) -> Result<()> {
    let run_script_path = smir_json_dir.join(format!("{}.sh", profile.name()));
    let mut run_script = std::fs::File::create(&run_script_path)?;
    writeln!(run_script, "#!/bin/bash")?;
    writeln!(run_script, "set -eu")?;
    writeln!(run_script)?;
    writeln!(
        run_script,
        "export LD_LIBRARY_PATH={}",
        ld_library_path.display(),
    )?;
    writeln!(
        run_script,
        "exec \"{}/{}/stable_mir_json\" \"$@\"",
        smir_json_dir.display(),
        profile.name()
    )?;

    // Set the script permissions to -rwxr-xr-x
    std::fs::set_permissions(run_script_path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn record_ld_library_path(smir_json_dir: &Path) -> Result<PathBuf> {
    const LOADER_PATH: &str = "LD_LIBRARY_PATH";
    if let Some(paths) = env::var_os(LOADER_PATH) {
        // Note: kani filters the LD_LIBRARY_PATH, not sure why as it is working locally as is
        let mut ld_library_file = std::fs::File::create(smir_json_dir.join("ld_library_path"))?;

        match paths.to_str() {
            Some(ld_library_path) => {
                writeln!(ld_library_file, "{}", ld_library_path)?;
                Ok(ld_library_path.into())
            }
            None => bail!("Couldn't cast LD_LIBRARY_PATH to str"),
        }
    } else {
        bail!("Couldn't read LD_LIBRARY_PATH from env"); // This should be unreachable
    }
}
