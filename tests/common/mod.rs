// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use assert_cmd::Command;
use std::fs::copy;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use testdir::testdir;
use testresult::TestResult;

mod cmd;
pub use cmd::cmd_guestmount;
pub use cmd::cmd_guestunmount;
pub use cmd::cmd_qemu_img;
pub use cmd::cmd_qemu_system;
pub use cmd::cmd_rauc;
pub use cmd::Cmd;

mod error;
pub use error::TestError;

mod image;
pub use image::ab_image;
pub use image::bundle_disks;
pub use image::ovmf_vars;
use image::reset_image;
pub use image::DiskType;
pub use image::FileSystem;
pub use image::TestImage;
pub use image::UpdateImage;

mod rauc;
pub use rauc::rauc_bundles;
pub use rauc::RaucBundle;

mod path;
pub use path::input_path_ovmf_code;
use path::input_path_ovmf_vars;
use path::mkosi_dir_ab_image;
use path::mkosi_dir_base_image;
use path::mkosi_dir_bundle_image;
use path::mkosi_dir_single_image;
use path::output_dir;
use path::output_dir_ab_override;
use path::output_path_ab_image;
use path::output_path_base_image;
use path::output_path_ovmf_vars;
use path::output_path_single_image;
use path::remove_files;

mod pki;
pub use pki::public_key_infrastructure;

/// Run a test using QEMU
///
/// A prepared A/B image (containing the caterpillar payload) is booted into, using a pre-configured EFI bootloader, while an image containing zero or more RAUC update bundles is attached
pub fn run_test(
    qemu_system: &Cmd,
    qemu_img: &Cmd,
    ovmf_code: PathBuf,
    ovmf_vars: PathBuf,
    ab_image: TestImage,
    bundle_disk: UpdateImage,
    name: &str,
) -> TestResult {
    let duration = Duration::from_secs(1);
    // NOTE: we need to wait for the images to settle
    sleep(duration);

    let tmp_ovmf_vars = testdir!().join(&ovmf_vars.file_name().unwrap());
    copy(&ovmf_vars, &tmp_ovmf_vars)?;

    Command::new(&qemu_system.path())
        .arg("-boot")
        .arg("order=d,menu=on,reboot-timeout=5000")
        .arg("-m")
        .arg("size=3072")
        .arg("-machine")
        .arg("type=q35,smm=on,accel=kvm,usb=on")
        .arg("-smbios")
        .arg(format!(
            "type=11,value=io.systemd.credential:test_environment={}",
            name
        ))
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,unit=0,file={},read-only=on",
            &ovmf_code.display()
        ))
        .arg("-drive")
        .arg(format!(
            "file={},format=raw,if=pflash,readonly=off,unit=1",
            &tmp_ovmf_vars.display()
        ))
        .arg("-drive")
        .arg(format!("format=qcow2,file={}", ab_image.path().display()))
        .arg("-drive")
        .arg(format!(
            "format=qcow2,file={}",
            bundle_disk.path().display()
        ))
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

    reset_image(&qemu_img, &bundle_disk.path())?;
    reset_image(&qemu_img, &ab_image.path())?;

    // NOTE: we need to wait for the images to settle
    sleep(duration);

    Ok(())
}
