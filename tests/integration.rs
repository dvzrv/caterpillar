// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use rstest::rstest;
use std::path::PathBuf;
use testresult::TestResult;

mod common;
use common::ab_image;
use common::bundle_disks;
use common::cmd_guestmount;
use common::cmd_guestunmount;
use common::cmd_qemu_img;
use common::cmd_qemu_system;
use common::input_path_ovmf_code;
use common::ovmf_vars;
use common::rauc_bundles;
use common::run_test;
use common::Cmd;
use common::DiskType;
use common::FileSystem;
use common::RaucBundle;
use common::TestError;
use common::TestImage;
use common::UpdateImage;

use serial_test::file_serial;

#[rstest]
#[case(FileSystem::Btrfs)]
#[case(FileSystem::Ext4)]
#[case(FileSystem::Vfat)]
#[file_serial]
fn integration_success_single(
    cmd_qemu_img: Result<Cmd, which::Error>,
    cmd_qemu_system: Result<Cmd, which::Error>,
    cmd_guestmount: Result<Cmd, which::Error>,
    cmd_guestunmount: Result<Cmd, which::Error>,
    input_path_ovmf_code: Result<PathBuf, TestError>,
    ab_image: Result<TestImage, TestError>,
    ovmf_vars: Result<PathBuf, TestError>,
    bundle_disks: Result<Vec<UpdateImage>, TestError>,
    rauc_bundles: Result<Vec<RaucBundle>, TestError>,
    #[case] filesystem: FileSystem,
) -> TestResult {
    let name = "success_single";
    let disk_type = DiskType::Single;

    let qemu_img = cmd_qemu_img?;
    let qemu_system = cmd_qemu_system?;
    let guestmount = cmd_guestmount?;
    let guestunmount = cmd_guestunmount?;
    let ovmf_vars = ovmf_vars?;
    let update_bundles = rauc_bundles?;
    let bundle_disk = match bundle_disks?
        .iter()
        .find(|x| x.filesystem().eq(&filesystem) && x.disk_type().eq(&disk_type))
    {
        Some(bundle) => bundle.clone(),
        None => return Err(testresult::TestError::from("foo")),
    };
    let test_image = ab_image?;
    println!("Built ab_image: {:?}", &test_image);
    test_image.prepare_for_test(&qemu_img, &guestmount, &guestunmount)?;

    bundle_disk.prepare_test(
        &qemu_img,
        &guestmount,
        &guestunmount,
        vec![(update_bundles[0].clone(), PathBuf::from("update.raucb"))],
    )?;

    println!("Created OVMF vars: {}", ovmf_vars.display());
    println!("Using bundle disk: {:?}", bundle_disk.path().display());
    println!("Created RAUC bundles: {:?}", update_bundles);

    run_test(
        &qemu_system,
        &qemu_img,
        input_path_ovmf_code?,
        ovmf_vars,
        test_image,
        bundle_disk,
        name,
    )?;

    Ok(())
}

#[rstest]
#[case(FileSystem::Btrfs)]
#[case(FileSystem::Ext4)]
#[case(FileSystem::Vfat)]
#[file_serial]
fn integration_success_multiple(
    cmd_qemu_img: Result<Cmd, which::Error>,
    cmd_qemu_system: Result<Cmd, which::Error>,
    cmd_guestmount: Result<Cmd, which::Error>,
    cmd_guestunmount: Result<Cmd, which::Error>,
    input_path_ovmf_code: Result<PathBuf, TestError>,
    ab_image: Result<TestImage, TestError>,
    ovmf_vars: Result<PathBuf, TestError>,
    bundle_disks: Result<Vec<UpdateImage>, TestError>,
    rauc_bundles: Result<Vec<RaucBundle>, TestError>,
    #[case] filesystem: FileSystem,
) -> TestResult {
    let name = "success_multiple";
    let disk_type = DiskType::Multiple;

    let qemu_img = cmd_qemu_img?;
    let qemu_system = cmd_qemu_system?;
    let guestmount = cmd_guestmount?;
    let guestunmount = cmd_guestunmount?;
    let ovmf_vars = ovmf_vars?;
    let update_bundles = rauc_bundles?;
    let bundle_disk = match bundle_disks?
        .iter()
        .find(|x| x.filesystem().eq(&filesystem) && x.disk_type().eq(&disk_type))
    {
        Some(bundle) => bundle.clone(),
        None => return Err(testresult::TestError::from("foo")),
    };

    let test_image = ab_image?;
    println!("Built ab_image: {:?}", &test_image);
    test_image.prepare_for_test(&qemu_img, &guestmount, &guestunmount)?;

    bundle_disk.prepare_test(
        &qemu_img,
        &guestmount,
        &guestunmount,
        vec![
            (update_bundles[0].clone(), PathBuf::from("update.raucb")),
            (update_bundles[1].clone(), PathBuf::from("update2.raucb")),
        ],
    )?;

    println!("Created OVMF vars: {}", ovmf_vars.display());
    println!("Using bundle disk: {:?}", bundle_disk.path().display());
    println!("Created RAUC bundles: {:?}", update_bundles);

    run_test(
        &qemu_system,
        &qemu_img,
        input_path_ovmf_code?,
        ovmf_vars,
        test_image,
        bundle_disk,
        name,
    )?;

    Ok(())
}

#[rstest]
#[case(FileSystem::Btrfs)]
#[case(FileSystem::Ext4)]
#[case(FileSystem::Vfat)]
#[file_serial]
fn integration_success_override(
    cmd_qemu_img: Result<Cmd, which::Error>,
    cmd_qemu_system: Result<Cmd, which::Error>,
    cmd_guestmount: Result<Cmd, which::Error>,
    cmd_guestunmount: Result<Cmd, which::Error>,
    input_path_ovmf_code: Result<PathBuf, TestError>,
    ab_image: Result<TestImage, TestError>,
    ovmf_vars: Result<PathBuf, TestError>,
    bundle_disks: Result<Vec<UpdateImage>, TestError>,
    rauc_bundles: Result<Vec<RaucBundle>, TestError>,
    #[case] filesystem: FileSystem,
) -> TestResult {
    let name = "success_override";
    let disk_type = DiskType::Multiple;

    let qemu_img = cmd_qemu_img?;
    let qemu_system = cmd_qemu_system?;
    let guestmount = cmd_guestmount?;
    let guestunmount = cmd_guestunmount?;
    let ovmf_vars = ovmf_vars?;
    let update_bundles = rauc_bundles?;
    let bundle_disk = match bundle_disks?
        .iter()
        .find(|x| x.filesystem().eq(&filesystem) && x.disk_type().eq(&disk_type))
    {
        Some(bundle) => bundle.clone(),
        None => return Err(testresult::TestError::from("foo")),
    };

    let test_image = ab_image?;
    println!("Built ab_image: {:?}", &test_image);
    test_image.prepare_for_test(&qemu_img, &guestmount, &guestunmount)?;

    bundle_disk.prepare_test(
        &qemu_img,
        &guestmount,
        &guestunmount,
        vec![
            (
                update_bundles[0].clone(),
                PathBuf::from("override/update.raucb"),
            ),
            (update_bundles[1].clone(), PathBuf::from("update2.raucb")),
        ],
    )?;

    println!("Created OVMF vars: {}", ovmf_vars.display());
    println!("Using bundle disk: {:?}", bundle_disk.path().display());
    println!("Created RAUC bundles: {:?}", update_bundles);

    run_test(
        &qemu_system,
        &qemu_img,
        input_path_ovmf_code?,
        ovmf_vars,
        test_image,
        bundle_disk,
        name,
    )?;

    Ok(())
}
