// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use assert_cmd::Command;
use rstest::fixture;
use semver::Version;
use std::fs::copy;
use std::fs::create_dir_all;
use std::fs::remove_dir_all;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use testdir::testdir;

use super::ab_image;
use super::cmd_rauc;
use super::output_dir;
use super::public_key_infrastructure;
use super::Cmd;
use super::TestError;
use super::TestImage;

#[derive(Clone, Debug)]
/// A RAUC update bundle
pub struct RaucBundle {
    path: PathBuf,
    version: Version,
    compatible: String,
}

impl RaucBundle {
    pub fn new(path: PathBuf, version: Version, compatible: String) -> Self {
        RaucBundle {
            path,
            version,
            compatible,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn compatible(&self) -> &str {
        &self.compatible
    }
}

#[fixture]
/// A fixture to describe which RAUC update bundles to create
fn output_rauc_bundles(output_dir: PathBuf) -> Vec<RaucBundle> {
    vec![
        RaucBundle::new(
            output_dir.join("update.raucb"),
            Version::new(1, 0, 0),
            "system".to_string(),
        ),
        RaucBundle::new(
            output_dir.join("update2.raucb"),
            Version::new(2, 0, 0),
            "system".to_string(),
        ),
    ]
}

#[fixture]
/// A fixture to provide RAUC update bundles
pub fn rauc_bundles(
    cmd_rauc: Result<Cmd, which::Error>,
    output_rauc_bundles: Vec<RaucBundle>,
    ab_image: Result<TestImage, TestError>,
    public_key_infrastructure: Result<(PathBuf, PathBuf), TestError>,
) -> Result<Vec<RaucBundle>, TestError> {
    let image_data = ab_image?;
    let (private_key, public_key) = public_key_infrastructure?;
    let rauc = format!("{}", cmd_rauc?);
    let names = ("esp.vfat", "root.img");
    for bundle in output_rauc_bundles.iter() {
        if !bundle.path().exists() {
            eprintln!(
                "RAUC update bundle {} does not exist yet. Creating...",
                bundle.path().display()
            );

            let bundle_dir = testdir!().join("rauc_bundle_dir");
            create_dir_all(&bundle_dir)?;

            {
                let mut f = BufWriter::new(File::create(bundle_dir.join("manifest.raucm"))?);

                writeln!(f, "[update]")?;
                writeln!(f, "compatible={}", bundle.compatible())?;
                writeln!(f, "version={}", bundle.version().to_string())?;
                writeln!(f, "[bundle]")?;
                writeln!(f, "format=verity")?;
                writeln!(f, "[image.efi]")?;
                writeln!(f, "filename={}", names.0)?;
                writeln!(f, "[image.rootfs]")?;
                writeln!(f, "filename={}", names.1)?;
            }

            println!(
                "Copy efi ({}) and rootfs ({}) to bundle dir...",
                image_data.efi().display(),
                image_data.rootfs().display()
            );
            copy(image_data.efi(), bundle_dir.join(names.0))?;
            copy(image_data.rootfs(), bundle_dir.join(names.1))?;
            Command::new(&rauc)
                .arg("bundle")
                .arg("--key")
                .arg(format!("{}", &private_key.display()))
                .arg("--cert")
                .arg(format!("{}", &public_key.display()))
                .arg(format!("{}", bundle_dir.display()))
                .arg(format!("{}", bundle.path().display()))
                .assert()
                .try_success()?;
            remove_dir_all(bundle_dir)?;
        }
    }
    Ok(output_rauc_bundles)
}
