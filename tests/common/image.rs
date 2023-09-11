// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::fs::copy;
use std::fs::create_dir_all;
use std::fs::remove_dir;
use std::path::Path;
use std::path::PathBuf;

use assert_cmd::Command;
use rstest::fixture;
use strum_macros::EnumString;
use testdir::testdir;
use testresult::TestResult;

use super::cmd::cmd_mkosi;
use super::cmd_qemu_img;
use super::cmd_qemu_system;
use super::input_path_ovmf_code;
use super::input_path_ovmf_vars;
use super::mkosi_dir_ab_image;
use super::mkosi_dir_base_image;
use super::mkosi_dir_bundle_image;
use super::mkosi_dir_single_image;
use super::output_dir;
use super::output_dir_ab_override;
use super::output_path_ab_image;
use super::output_path_base_image;
use super::output_path_ovmf_vars;
use super::output_path_single_image;
use super::public_key_infrastructure;
use super::remove_files;
use super::Cmd;
use super::RaucBundle;
use super::TestError;

#[derive(Clone, Copy, Debug, strum::Display, EnumString, PartialEq)]
#[non_exhaustive]
/// A type of a disk image
pub enum DiskType {
    #[strum(to_string = "empty")]
    Empty,
    #[strum(to_string = "multiple")]
    Multiple,
    #[strum(to_string = "single")]
    Single,
}

#[derive(Clone, Copy, Debug, strum::Display, EnumString, PartialEq)]
#[non_exhaustive]
/// A filesystem of a disk image
pub enum FileSystem {
    #[strum(to_string = "btrfs")]
    Btrfs,
    #[strum(to_string = "ext4")]
    Ext4,
    #[strum(to_string = "vfat")]
    Vfat,
}

#[derive(Clone, Debug)]
/// An update image, containing zero or more updates
pub struct UpdateImage {
    path: PathBuf,
    filesystem: FileSystem,
    disk_type: DiskType,
}

impl UpdateImage {
    pub fn new(path: PathBuf, filesystem: FileSystem, size: DiskType) -> Self {
        Self {
            path,
            filesystem,
            disk_type: size,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn filesystem(&self) -> &FileSystem {
        &self.filesystem
    }

    pub fn disk_type(&self) -> &DiskType {
        &self.disk_type
    }

    /// prepare bundle disk for a named test by resetting the disk and copying update bundles into it
    pub fn prepare_test(
        &self,
        qemu_img: &Cmd,
        guestmount: &Cmd,
        guestunmount: &Cmd,
        update_bundles: Vec<(RaucBundle, PathBuf)>,
    ) -> TestResult {
        reset_image(&qemu_img, self.path())?;

        if !update_bundles.is_empty() {
            let mount_dir = testdir!().join("bundle_disk_write_mount");
            create_dir_all(&mount_dir)?;

            // mount the disk
            Command::new(&guestmount.path())
                .arg("-a")
                .arg(format!("{}", self.path().display()))
                .arg("-m")
                .arg("/dev/sda1")
                .arg("--rw")
                .arg(format!("{}", &mount_dir.display()))
                .assert()
                .try_success()?;

            // copy update bundles to disk
            for (bundle, target) in update_bundles {
                if let Some(target_parent) = target.parent() {
                    create_dir_all(&mount_dir.join(target_parent))?;
                }
                copy(&bundle.path(), &mount_dir.join(target))?;
            }

            // unmount persistence partition
            Command::new(&guestunmount.path())
                .arg(format!("{}", &mount_dir.display()))
                .assert()
                .try_success()?;

            remove_dir(mount_dir)?;
        }

        Ok(())
    }
}

/// A disk image to test with
#[derive(Debug)]
pub struct TestImage {
    path: PathBuf,
    efi: PathBuf,
    rootfs: PathBuf,
}

impl TestImage {
    pub fn new(path: PathBuf, efi: PathBuf, rootfs: PathBuf) -> Self {
        TestImage { path, efi, rootfs }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn efi(&self) -> &Path {
        &self.efi
    }

    pub fn rootfs(&self) -> &Path {
        &self.rootfs
    }

    /// Prepare an image for a test by resetting it, deploying a payload and creating another snapshot
    pub fn prepare_for_test(
        &self,
        qemu_img: &Cmd,
        guestmount: &Cmd,
        guestunmount: &Cmd,
    ) -> TestResult {
        reset_image(&qemu_img, self.path())?;

        let payload = PathBuf::from(env!("CARGO_BIN_EXE_caterpillar"));
        let mount_dir = testdir!().join("write_mount");
        create_dir_all(&mount_dir)?;

        // mount the first partition (persistence partition)
        Command::new(&guestmount.path())
            .arg("-a")
            .arg(format!("{}", self.path().display()))
            .arg("-m")
            .arg("/dev/sda1")
            .arg("--rw")
            .arg(format!("{}", &mount_dir.display()))
            .assert()
            .try_success()?;

        // copy payload to persistence partition
        copy(&payload, &mount_dir.join(payload.file_name().unwrap()))?;

        // unmount persistence partition
        Command::new(&guestunmount.path())
            .arg(format!("{}", &mount_dir.display()))
            .assert()
            .try_success()?;

        remove_dir(mount_dir)?;
        Ok(())
    }
}

/// Convert a virtual machine image from raw to qcow2
pub fn convert_image(qemu_img: &Cmd, input: &str, output: &str) -> Result<(), TestError> {
    Command::new(qemu_img.path())
        .arg("convert")
        .arg("-c")
        .arg("-f")
        .arg("raw")
        .arg("-O")
        .arg("qcow2")
        .arg("--")
        .arg(input)
        .arg(output)
        .assert()
        .try_success()?;
    Ok(())
}

/// Create a snapshot of a virtual machine image
///
/// If `name` is `None` the snapshot is called `"base"`.
pub fn snapshot_image(qemu_img: &Cmd, path: &Path, name: Option<&str>) -> Result<(), TestError> {
    Command::new(qemu_img.path())
        .arg("snapshot")
        .arg("-c")
        .arg(name.unwrap_or("base"))
        .arg(format!("{}", path.display()))
        .assert()
        .try_success()?;
    Ok(())
}

/// Reset a virtual machine image to its "base" snapshot
pub fn reset_image(qemu_img: &Cmd, path: &Path) -> Result<(), TestError> {
    Command::new(qemu_img.path())
        .arg("snapshot")
        .arg("-a")
        .arg("base")
        .arg(format!("{}", path.display()))
        .assert()
        .try_success()?;
    Ok(())
}

#[fixture]
/// A basic virtual machine image (tar file)
///
/// This tar file is used to create `single_image` and `ab_image` from.
pub fn base_image(
    cmd_mkosi: Result<Cmd, which::Error>,
    mkosi_dir_base_image: PathBuf,
    output_path_base_image: PathBuf,
    output_dir: PathBuf,
) -> Result<PathBuf, TestError> {
    if !output_path_base_image.exists() {
        println!(
            "{} does not exist yet. Building...",
            output_path_base_image.display()
        );

        Command::new(format!("{}", cmd_mkosi?))
            .arg("--output-dir")
            .arg(format!("{}", &output_dir.display()))
            .arg("--force")
            .arg("build")
            .current_dir(mkosi_dir_base_image)
            .assert()
            .try_success()?;

        // remove unnecessary files to save space
        remove_files(&[
            &format!("{}.efi", &output_path_base_image.display()),
            &format!("{}.initrd", &output_path_base_image.display()),
            &format!("{}.vmlinuz", &output_path_base_image.display()),
            &format!("{}-initrd", &output_path_base_image.display()),
            &format!("{}-initrd.cpio.zst", &output_path_base_image.display()),
        ])?;
    }

    Ok(output_path_base_image)
}

#[fixture]
/// An intermediate single system virtual machine image
///
/// This image's main purpose is to be used in the context of generating EFI boot loader entries for `ab_image`.
pub fn single_image(
    cmd_mkosi: Result<Cmd, which::Error>,
    cmd_qemu_img: Result<Cmd, which::Error>,
    base_image: Result<PathBuf, TestError>,
    mkosi_dir_single_image: PathBuf,
    output_path_single_image: PathBuf,
    output_dir: PathBuf,
) -> Result<PathBuf, TestError> {
    let output_path = PathBuf::from(format!("{}.qcow2", output_path_single_image.display()));

    if !output_path.exists() {
        println!(
            "{} does not exist yet. Building...",
            output_path_single_image.display()
        );

        let qemu_img = cmd_qemu_img?;

        Command::new(cmd_mkosi?.path())
            .arg("--base-tree")
            .arg(format!("{}", &base_image?.display()))
            .arg("--output-dir")
            .arg(format!("{}", &output_dir.display()))
            .arg("--force")
            .arg("build")
            .current_dir(mkosi_dir_single_image)
            .assert()
            .try_success()?;

        convert_image(
            &qemu_img,
            &format!("{}.raw", &output_path_single_image.display()),
            &format!("{}", &output_path.display()),
        )?;

        snapshot_image(&qemu_img, &output_path, None)?;

        // remove unnecessary files to save space
        remove_files(&[
            &format!("{}", &output_path_single_image.display()),
            &format!("{}.efi", &output_path_single_image.display()),
            &format!("{}.initrd", &output_path_single_image.display()),
            &format!("{}.raw", &output_path_single_image.display()),
            &format!("{}.vmlinuz", &output_path_single_image.display()),
            &format!("{}-initrd", &output_path_single_image.display()),
            &format!("{}-initrd.cpio.zst", &output_path_single_image.display()),
        ])?;
    }

    Ok(output_path)
}

#[fixture]
/// An A/B test image
///
/// The test image is a .qcow2 virtual machine image with five partitions:
/// - the first serves as writable location for persistence
/// - the second and third partition are ESPs for two different target root filesystems
/// - the fourth and fifth partition are root filesystems that are tied to their respective ESPs
pub fn ab_image(
    cmd_mkosi: Result<Cmd, which::Error>,
    cmd_qemu_img: Result<Cmd, which::Error>,
    public_key_infrastructure: Result<(PathBuf, PathBuf), TestError>,
    base_image: Result<PathBuf, TestError>,
    output_dir_ab_override: PathBuf,
    mkosi_dir_ab_image: PathBuf,
    output_path_ab_image: PathBuf,
    output_dir: PathBuf,
) -> Result<TestImage, TestError> {
    let output = TestImage::new(
        PathBuf::from(format!("{}.qcow2", output_path_ab_image.display())),
        PathBuf::from(format!("{}.esp_a.raw", output_path_ab_image.display())),
        PathBuf::from(format!(
            "{}.root-x86-64_a.raw",
            output_path_ab_image.display()
        )),
    );

    if !output.path().exists() {
        println!(
            "{} does not exist yet. Building...",
            output.path().display()
        );

        if let Err(error) = &public_key_infrastructure {
            eprintln!("error creating PKI: {:?}", error);
            assert!(false);
        }

        let qemu_img = cmd_qemu_img?;

        Command::new(cmd_mkosi?.path())
            .arg("--base-tree")
            .arg(format!("{}", &base_image?.display(),))
            .arg("--extra-tree")
            .arg(format!("{}", &output_dir_ab_override.display()))
            .arg("--output-dir")
            .arg(format!("{}", &output_dir.display()))
            .arg("--force")
            .arg("build")
            .current_dir(&mkosi_dir_ab_image)
            .assert()
            .try_success()?;

        convert_image(
            &qemu_img,
            &format!("{}.raw", &output_path_ab_image.display()),
            &format!("{}", &output.path().display()),
        )?;
        snapshot_image(&qemu_img, &output.path(), None)?;

        // remove unnecessary files to save space
        remove_files(&[
            &format!("{}", &output_path_ab_image.display()),
            &format!("{}.efi", &output_path_ab_image.display()),
            &format!("{}.raw", &output_path_ab_image.display()),
            &format!("{}.esp_b.raw", &output_path_ab_image.display()),
            &format!("{}.initrd", &output_path_ab_image.display()),
            &format!("{}.linux-generic.raw", &output_path_ab_image.display()),
            &format!("{}.root-x86-64_b.raw", &output_path_ab_image.display()),
            &format!("{}.vmlinuz", &output_path_ab_image.display()),
            &format!("{}-initrd", &output_path_ab_image.display()),
            &format!("{}-initrd.cpio.zst", &output_path_ab_image.display()),
        ])?;
    }

    Ok(output)
}

#[fixture]
/// A fixture for providing OVMF vars prepared for a test setup
///
/// The OVMF vars contain EFI bootloader entries for EFI partitions of the test setup.
pub fn ovmf_vars(
    cmd_qemu_system: Result<Cmd, which::Error>,
    input_path_ovmf_code: Result<PathBuf, TestError>,
    input_path_ovmf_vars: Result<PathBuf, TestError>,
    output_path_ovmf_vars: PathBuf,
    single_image: Result<PathBuf, TestError>,
    ab_image: Result<TestImage, TestError>,
) -> Result<PathBuf, TestError> {
    if !output_path_ovmf_vars.exists() {
        println!(
            "{} does not exist yet. Creating...",
            &output_path_ovmf_vars.display()
        );

        let test_dir = testdir!();
        let tmp_file = test_dir.join(&output_path_ovmf_vars.file_name().unwrap());
        println!("Copy template OVMF vars to temporary file...");
        copy(&input_path_ovmf_vars?, &tmp_file)?;

        Command::new(format!("{}", cmd_qemu_system?))
            .arg("-boot")
            .arg("order=d,menu=on,reboot-timeout=5000")
            .arg("-m")
            .arg("size=3072")
            .arg("-machine")
            .arg("type=q35,smm=on,accel=kvm,usb=on")
            .arg("-smbios")
            .arg("type=11,value=io.systemd.credential:set_efi_boot_entries=yes")
            .arg("-drive")
            .arg(format!(
                "if=pflash,format=raw,unit=0,file={},read-only=on",
                input_path_ovmf_code?.display()
            ))
            .arg("-drive")
            .arg(format!(
                "file={},format=raw,if=pflash,readonly=off,unit=1",
                tmp_file.display()
            ))
            .arg("-drive")
            .arg(format!("format=qcow2,file={}", &single_image?.display()))
            .arg("-drive")
            .arg(format!("format=qcow2,file={}", &ab_image?.path().display()))
            .arg("-nographic")
            .arg("-nodefaults")
            .arg("-chardev")
            .arg("stdio,mux=on,id=console,signal=off")
            .arg("-serial")
            .arg("chardev:console")
            .arg("-mon")
            .arg("console")
            .assert()
            .try_success()?;

        println!("Copy template OVMF vars to output...");
        copy(&tmp_file, &output_path_ovmf_vars)?;
    }
    Ok(output_path_ovmf_vars)
}

#[fixture]
/// A fixture to provide empty update images used in test setups
///
/// The created `UpdateImage`s are of varying type (`DiskType`) and filesystem (`FileSystem`)
pub fn bundle_disks(
    cmd_mkosi: Result<Cmd, which::Error>,
    cmd_qemu_img: Result<Cmd, which::Error>,
    mkosi_dir_bundle_image: PathBuf,
    output_dir: PathBuf,
) -> Result<Vec<UpdateImage>, TestError> {
    let mut paths = vec![];
    let disk_types = [DiskType::Empty, DiskType::Multiple, DiskType::Single];
    let filesystems = [FileSystem::Btrfs, FileSystem::Ext4, FileSystem::Vfat];
    let mkosi = cmd_mkosi?;
    let qemu_img = cmd_qemu_img?;

    for filesystem in filesystems {
        for disk_type in disk_types {
            let path: PathBuf = [
                format!("{}", output_dir.display()),
                format!("{}_{}.qcow2", filesystem, disk_type),
            ]
            .iter()
            .collect();

            if !path.exists() {
                println!("{} does not exist yet. Generating...", path.display());
                Command::new(mkosi.path())
                    .arg("--output")
                    .arg(format!("{}_{}", filesystem, disk_type))
                    .arg("--output-dir")
                    .arg(format!("{}", &output_dir.display()))
                    .arg("--repart-dir")
                    .arg(format!("repart/{}/{}", filesystem, disk_type))
                    .arg("--force")
                    .arg("build")
                    .current_dir(&mkosi_dir_bundle_image)
                    .assert()
                    .try_success()?;

                convert_image(
                    &qemu_img,
                    &format!(
                        "{}",
                        &path
                            .with_file_name(format!("{}_{}.raw", filesystem, disk_type))
                            .display()
                    ),
                    &format!("{}", &path.display()),
                )?;

                snapshot_image(&qemu_img, &path, None)?;

                // remove unnecessary files to save space
                remove_files(&[
                    &format!(
                        "{}",
                        &path
                            .with_file_name(format!("{}_{}.raw", filesystem, disk_type))
                            .display()
                    ),
                    &format!(
                        "{}",
                        &path
                            .with_file_name(format!("{}_{}", filesystem, disk_type))
                            .display()
                    ),
                ])?;
            }

            paths.push(UpdateImage::new(path, filesystem, disk_type));
        }
    }

    Ok(paths)
}
