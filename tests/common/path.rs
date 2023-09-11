// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use super::TestError;
use rstest::fixture;
use std::fs::remove_file;
use std::path::PathBuf;

/// Remove files in a list of files
pub fn remove_files(files: &[&str]) -> Result<(), std::io::Error> {
    for file in files {
        if let Err(error) = remove_file(file) {
            eprintln!("Unable to remove {}: {}", file, error);
        };
    }
    Ok(())
}

#[fixture]
/// A fixture to provide the first matching location of OVMF code files
pub fn input_path_ovmf_code() -> Result<PathBuf, TestError> {
    let candidates = [PathBuf::from("/usr/share/edk2/x64/OVMF_CODE.4m.fd")];

    match candidates.iter().find(|&candidate| candidate.exists()) {
        Some(candidate) => Ok(candidate.clone()),
        None => return Err(TestError::Missing("OVMF code".to_string())),
    }
}

#[fixture]
/// A fixture to provide the first matching location of OVMF variable files
pub fn input_path_ovmf_vars() -> Result<PathBuf, TestError> {
    let candidates = [PathBuf::from("/usr/share/edk2/x64/OVMF_VARS.4m.fd")];

    match candidates.iter().find(|&candidate| candidate.exists()) {
        Some(candidate) => Ok(candidate.clone()),
        None => return Err(TestError::Missing("OVMF code".to_string())),
    }
}

#[fixture]
/// A fixture to provide the output dir for integration tests
pub fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

#[fixture]
/// A fixture to provide the output path for the base image
pub fn output_path_base_image(output_dir: PathBuf) -> PathBuf {
    PathBuf::from(&output_dir).join("base")
}

#[fixture]
/// A fixture to provide the output path for the single image
pub fn output_path_single_image(output_dir: PathBuf) -> PathBuf {
    PathBuf::from(&output_dir).join("single")
}

#[fixture]
/// A fixture to provide the output path for the A/B image
pub fn output_path_ab_image(output_dir: PathBuf) -> PathBuf {
    PathBuf::from(&output_dir).join("ab")
}

#[fixture]
/// A fixture to provide the output path for the override directory of the A/B image
pub fn output_dir_ab_override(output_dir: PathBuf) -> PathBuf {
    PathBuf::from(&output_dir).join("override_for_ab")
}

#[fixture]
/// A fixture to provide the output path for modified OVMF vars file
///
/// This location is used for the OVMF vars that contain EFI bootloader entries for the A/B image
pub fn output_path_ovmf_vars(output_dir: PathBuf) -> PathBuf {
    PathBuf::from(&output_dir).join("ovmf_vars.fd")
}

#[fixture]
/// A fixture to provide the integration tests directory of the project
fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[fixture]
/// A fixture to provide the directory of the mkosi base image setup
pub fn mkosi_dir_base_image(tests_dir: PathBuf) -> PathBuf {
    PathBuf::from(tests_dir).join("mkosi").join("base_image")
}

#[fixture]
/// A fixture to provide the directory of the mkosi single image setup
pub fn mkosi_dir_single_image(tests_dir: PathBuf) -> PathBuf {
    PathBuf::from(tests_dir).join("mkosi").join("single_image")
}

#[fixture]
/// A fixture to provide the directory of the mkosi A/B image setup
pub fn mkosi_dir_ab_image(tests_dir: PathBuf) -> PathBuf {
    PathBuf::from(tests_dir).join("mkosi").join("ab_image")
}

#[fixture]
/// A fixture to provide the directory of the mkosi update image setup
pub fn mkosi_dir_bundle_image(tests_dir: PathBuf) -> PathBuf {
    PathBuf::from(tests_dir).join("mkosi").join("bundle_image")
}
