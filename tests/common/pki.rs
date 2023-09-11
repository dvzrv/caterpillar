// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::fs::create_dir_all;
use std::path::PathBuf;

use assert_cmd::Command;
use rstest::fixture;

use super::cmd::cmd_openssl;
use super::path::output_dir;
use super::path::output_dir_ab_override;
use super::Cmd;
use super::TestError;

#[fixture]
/// A fixture to define the output paths of the PKI
fn output_paths_pki(output_dir: PathBuf, output_dir_ab_override: PathBuf) -> (PathBuf, PathBuf) {
    (
        PathBuf::from(&output_dir).join("system.key"),
        PathBuf::from(&output_dir_ab_override)
            .join("etc")
            .join("rauc")
            .join("system.pem"),
    )
}

#[fixture]
/// A fixture to create and provide the public key infrastructure
pub fn public_key_infrastructure(
    output_paths_pki: (PathBuf, PathBuf),
    cmd_openssl: Result<Cmd, which::Error>,
) -> Result<(PathBuf, PathBuf), TestError> {
    if !(output_paths_pki.0.exists() && output_paths_pki.1.exists()) {
        println!(
            "{} and {} do not exist yet. Generating...",
            &output_paths_pki.0.display(),
            &output_paths_pki.1.display()
        );

        create_dir_all(output_paths_pki.1.parent().unwrap())?;

        Command::new(format!("{}", cmd_openssl?))
            .arg("req")
            .arg("-x509")
            .arg("-newkey")
            .arg("rsa:4096")
            .arg("-nodes")
            .arg("-keyout")
            .arg(format!("{}", &output_paths_pki.0.display()))
            .arg("-out")
            .arg(format!("{}", &output_paths_pki.1.display()))
            .arg("-subj")
            .arg("/O=Test/CN=systems-device")
            .assert()
            .try_success()?;

        assert!(output_paths_pki.0.exists());
        assert!(output_paths_pki.1.exists());
    }
    Ok(output_paths_pki)
}
