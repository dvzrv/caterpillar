// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use device::Device;
use device::UdisksInfo;
use std::collections::HashMap;
use std::fs::rename;
use std::path::Path;
use std::path::PathBuf;
use zbus::Connection;

mod config;
use crate::config::read_config;

mod device;

mod rauc;
use rauc::RaucInfo;
use rauc::UpdateBundle;

mod error;
use error::Error;

mod macros;

mod proxy;
use crate::proxy::login1::ManagerProxy;

/// State of the updater
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum State {
    Mounting,
    Searching,
    Updating,
    Updated,
}

/// Return UdisksInfo, RaucInfo and ManagerProxy instances in a Result
async fn get_connections(
    connection: &Connection,
) -> Result<(UdisksInfo, RaucInfo, ManagerProxy), Error> {
    println!("Connecting to logind over dbus...");
    let login_proxy = match ManagerProxy::new(connection).await {
        Ok(proxy) => proxy,
        Err(error) => return Err(Error::Dbus(error)),
    };

    println!("Connecting to Udisks2 over dbus...");
    let udisks_proxy = match UdisksInfo::new(connection).await {
        Ok(proxy) => {
            println!("Communicating with Udisks2 {}", proxy.version());
            proxy
        }
        Err(error) => return Err(error),
    };

    println!("Connecting to RAUC over dbus...");
    let rauc_proxy = match RaucInfo::new(connection).await {
        Ok(proxy) => {
            println!("{}", proxy);
            println!("RAUC slot info:");
            for slot in proxy.slots() {
                println!("{}", slot);
            }
            proxy
        }
        Err(error) => return Err(error),
    };

    Ok((udisks_proxy, rauc_proxy, login_proxy))
}

/// Return a list of matching and mounted Device instances that have been searched for UpdateBundles in a Result
async fn mount_and_search_devices(
    connection: &Connection,
    device_regex: &str,
    bundle_extension: &str,
    override_dir: &str,
) -> Result<Vec<Device>, Error> {
    println!("Searching for compatible block devices...");
    let mut devices = UdisksInfo::get_block_devices(connection, device_regex).await?;

    for device in &mut devices[..] {
        match device.mount_filesystem(connection).await {
            Ok(_path) => {
                // gather PathBufs of update bundles
                if let Err(error) = device.find_bundles(bundle_extension).await {
                    eprintln!("{}", error)
                }

                // gather PathBufs of override update bundles
                if let Err(error) = device
                    .find_override_bundles(bundle_extension, Path::new(&override_dir))
                    .await
                {
                    eprintln!("{}", error)
                }
            }
            Err(error) => eprintln!("{}", error),
        }
    }
    Ok(devices)
}

/// Get an optional UpdateBundle to update to in a Result
async fn get_update_bundle(
    connection: &Connection,
    rauc_info: &RaucInfo,
    devices: &[Device],
) -> Result<Option<UpdateBundle>, Error> {
    println!("Search for compatible RAUC update bundle...");
    // get paths to all override bundles
    let override_bundle_paths: Vec<PathBuf> = devices
        .iter()
        .filter_map(|x| x.override_bundles())
        .flatten()
        .collect();

    match override_bundle_paths.len() {
        0 => {}
        // install override bundle
        1 => match UpdateBundle::new(&override_bundle_paths[0], true, connection).await {
            Ok(bundle) => {
                if bundle.compatible() == rauc_info.compatible() {
                    return Ok(Some(bundle));
                } else {
                    eprintln!(
                        "Update bundle {} is not compatible with this system!",
                        bundle.path()
                    )
                }
            }
            Err(error) => eprintln!("{}", error),
        },
        // error if there is more than one override bundle
        _ => return Err(Error::TooManyOverrides(override_bundle_paths)),
    }

    // get paths to all top-level bundles
    let bundle_paths: Vec<PathBuf> = devices
        .iter()
        .filter_map(|x| x.bundles())
        .flatten()
        .collect();

    if !bundle_paths.is_empty() {
        let mut bundles = vec![];
        for path in bundle_paths {
            match UpdateBundle::new(&path, false, connection).await {
                Ok(bundle) => {
                    println!("Found update bundle: {}", bundle.path());
                    // add bundle only if it is compatible and if its version is higher than the current
                    if bundle.compatible() == rauc_info.compatible() {
                        if rauc_info.version().is_none()
                            || rauc_info.version().is_some_and(|x| bundle.version().gt(x))
                        {
                            println!(
                                "Adding update bundle {} to list of compatible bundles...",
                                bundle.path()
                            );
                            bundles.push(bundle);
                        } else {
                            eprintln!("Update bundle {} is compatible, but its version ({}) is lower or equal to the current ({})", bundle.path(), bundle.version(), rauc_info.version_string());
                        }
                    } else {
                        eprintln!("Update bundle {} is not compatible!", bundle.path());
                    }
                }
                Err(error) => eprintln!("{}", error),
            }
        }

        if bundles.is_empty() {
            Ok(None)
        } else {
            // sort by version
            bundles.sort();
            bundles.reverse();
            println!("Selecting update bundle {}...", bundles[0].path());
            Ok(Some(bundles[0].clone()))
        }
    } else {
        Ok(None)
    }
}

/// Unmount any previously mounted filesystems
async fn unmount_filesystems(connection: &Connection, devices: Vec<Device>) -> Result<(), Error> {
    for mut device in devices {
        if device.is_mounted() {
            device.unmount_filesystem(connection).await?;
        }
    }
    Ok(())
}

#[tokio::main]
pub async fn main() -> Result<(), Error> {
    println!(
        "Starting {} {}.",
        env!("CARGO_BIN_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let mut state = State::Searching;
    let connection = Connection::system().await?;
    let config = read_config().await?;
    println!(
        "{:?}",
        &config
            .clone()
            .try_deserialize::<HashMap<String, String>>()
            .unwrap()
    );
    let (_, rauc_info, login_proxy) = get_connections(&connection).await?;

    let devices = mount_and_search_devices(
        &connection,
        &config.get_string("device_regex")?,
        &config.get_string("bundle_extension")?,
        &config.get_string("override_dir")?,
    )
    .await?;

    let result = match get_update_bundle(&connection, &rauc_info, &devices).await {
        Ok(Some(bundle)) => {
            println!(
                "Found {}update {}",
                if bundle.is_override() {
                    "override "
                } else {
                    " "
                },
                bundle.path()
            );
            state = State::Updating;
            bundle.install(&connection).await?;
            Ok((bundle.path(), bundle.is_override()))
        }
        Ok(None) => Err(Error::NoUpdateBundle),
        Err(error) => Err(error),
    };

    match result {
        Ok((bundle_path, is_override)) => {
            state = State::Updated;
            println!("Update successful!");

            // rename override bundle, so that it will not be installed again
            if is_override {
                println!("Disabling override bundle {}", bundle_path);
                rename(&bundle_path, format!("{}.installed", &bundle_path))?;
            }
        }
        Err(error) => eprintln!("{}", error),
    }

    unmount_filesystems(&connection, devices).await?;

    if state == State::Updated && config.get_bool("reboot")? {
        println!("Rebooting...");
        login_proxy.reboot(false).await?;
    }

    if state == State::Searching {
        println!("No compatible updates found. Exiting...")
    }

    Ok(())
}
