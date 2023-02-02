// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::str::FromStr;

use once_cell::sync::OnceCell;
use std::path::PathBuf;
use strum::Display;
use strum::EnumString;
use zbus::Connection;
use zvariant::{ObjectPath, Str, Value};

use crate::error::Error;
use crate::macros::regex_once;
use crate::proxy::udisks::ManagerProxy;
use crate::proxy::udisks::{BlockProxy, FilesystemProxy, PartitionProxy};

/// An enum of compatible filesystems
///
/// GPT based partition types are found in:
/// https://en.wikipedia.org/wiki/GUID_Partition_Table
/// MBR based partition type identifiers are found in:
/// https://en.wikipedia.org/wiki/Partition_type
#[derive(Debug, Display, EnumString, PartialEq)]
#[non_exhaustive]
enum Filesystem {
    #[strum(
        ascii_case_insensitive,
        to_string = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7"
    )]
    GptMicrosoftBasicData,
    #[strum(
        ascii_case_insensitive,
        to_string = "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
    )]
    GptLinuxFilesystemData,
    #[strum(ascii_case_insensitive, to_string = "0X06")]
    MbrFat16,
    #[strum(ascii_case_insensitive, to_string = "0X0E")]
    MbrFat16Lba,
    #[strum(ascii_case_insensitive, to_string = "0X0B")]
    MbrFat32,
    #[strum(ascii_case_insensitive, to_string = "0X0C")]
    MbrFat32Lba,
    #[strum(ascii_case_insensitive, to_string = "0X17")]
    MbrNtfs,
    #[strum(ascii_case_insensitive, to_string = "0X83")]
    MbrLinuxFilesystem,
}

pub struct UdisksInfo {
    version: String,
}

impl UdisksInfo {
    /// Create a new UdisksInfo
    pub async fn new(connection: &Connection) -> Result<Self, Error> {
        let manager_proxy = ManagerProxy::new(connection).await?;
        match manager_proxy.version().await {
            Ok(version) => Ok(UdisksInfo { version }),
            Err(error) => Err(Error::Dbus(error)),
        }
    }

    /// Return a reference to version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get block devices available to Udisks2
    pub async fn get_block_devices(
        connection: &Connection,
        device_regex: &str,
    ) -> Result<Vec<Device>, Error> {
        let manager_proxy = ManagerProxy::new(connection).await?;
        let options = HashMap::from([("auth.no_user_interaction", Value::Bool(false))]);
        let path_list = manager_proxy.get_block_devices(options).await?;

        Ok(path_list
            .iter()
            .filter_map(|x| {
                regex_once!(device_regex)
                    .is_match(x.as_str())
                    .then_some(Device::new(x.to_string()).unwrap())
            })
            .collect())
    }
}

impl Display for UdisksInfo {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(fmt, "Udisks2 ({})", self.version(),)
    }
}

/// A block device
#[derive(Clone, Debug)]
pub struct Device {
    objectpath: String,
    mountpoint: OnceCell<PathBuf>,
    /// indication whether the mountpoint should be unmounted
    unmountable: OnceCell<bool>,
    /// locations of potential UpdateBundles found at the top-level of a mountpoint
    bundles: Vec<PathBuf>,
    /// locations of potential  UpdateBundles found in override locations of a mountpoint
    override_bundles: Vec<PathBuf>,
}

impl Device {
    /// Create a new Device
    pub fn new(objectpath: String) -> Result<Self, Error> {
        if ObjectPath::try_from(objectpath.as_str()).is_err() {
            Err(Error::InvalidDevicePath(
                objectpath.replace("/org/freedesktop/UDisks2/block_devices", "/dev"),
            ))
        } else {
            Ok(Device {
                objectpath,
                mountpoint: OnceCell::new(),
                unmountable: OnceCell::new(),
                bundles: vec![],
                override_bundles: vec![],
            })
        }
    }

    /// Return whether the Device is mounted
    pub fn is_mounted(&self) -> bool {
        self.mountpoint.get().is_some()
    }

    /// Return a reference to the objectpath
    pub fn objectpath(&self) -> &str {
        &self.objectpath
    }

    /// Return a reference to the objectpath
    pub fn device_path(&self) -> String {
        self.objectpath
            .replace("/org/freedesktop/UDisks2/block_devices", "/dev")
    }

    /// Return vec of PathBufs of potential bundle locations in an Option
    pub fn bundles(&self) -> Option<Vec<PathBuf>> {
        if !self.bundles.is_empty() {
            Some(self.bundles.to_vec())
        } else {
            None
        }
    }

    /// Return vec of PathBufs of potential override bundle locations in an Option
    pub fn override_bundles(&self) -> Option<Vec<PathBuf>> {
        if !self.override_bundles.is_empty() {
            Some(self.override_bundles.to_vec())
        } else {
            None
        }
    }

    /// Mount a filesystem identified by the ObjectPath of the Device
    pub async fn mount_filesystem(&self, connection: &Connection) -> Result<String, Error> {
        println!("Checking block device {}...", &self.device_path());
        let objectpath = ObjectPath::try_from(self.objectpath.as_str()).unwrap();
        let block_proxy = BlockProxy::builder(connection)
            .cache_properties(zbus::CacheProperties::No)
            .path(&objectpath)?
            .build()
            .await?;
        let id_usage = block_proxy.id_usage().await?;

        if id_usage != "filesystem" {
            return Err(Error::IncompatibleBlockDevice(self.device_path()));
        }

        let partition_proxy = PartitionProxy::builder(connection)
            .cache_properties(zbus::CacheProperties::No)
            .path(&objectpath)?
            .build()
            .await?;
        let partition_number = partition_proxy.number().await?;

        if partition_number == 0 {
            return Err(Error::IsBaseDevice(self.device_path()));
        }

        let partition_type = partition_proxy.type_().await?;
        if let Ok(partition_type_ok) = Filesystem::from_str(&partition_type) {
            println!("Compatible partition type {:?} found!", &partition_type_ok);

            let filesystem_proxy = FilesystemProxy::builder(connection)
                .cache_properties(zbus::CacheProperties::No)
                .path(&objectpath)?
                .build()
                .await?;
            let mountpoints = filesystem_proxy.mount_points().await?;
            let mountpoint = if mountpoints.is_empty() {
                // NOTE: mount read-writable by default
                let mount_options = HashMap::from([("options", Value::Str(Str::from("rw")))]);
                let mountpoint = filesystem_proxy.mount(mount_options).await?;
                println!("Mounted {} to {}.", &self.device_path(), &mountpoint);
                self.unmountable.set(true).unwrap();
                mountpoint
            } else {
                // NOTE: removing NUL byte from response
                let mountpoint =
                    String::from_utf8(mountpoints[0][0..mountpoints[0].len() - 1].to_owned())?;
                println!(
                    "Found {} already mounted to {}",
                    &self.device_path(),
                    &mountpoint
                );
                self.unmountable.set(false).unwrap();
                mountpoint
            };

            if let Err(mountpoint) = self.mountpoint.set(Path::new(mountpoint.as_str()).into()) {
                return Err(Error::AlreadyMounted(
                    self.device_path(),
                    mountpoint.to_string_lossy().into(),
                ));
            } else {
                Ok(mountpoint)
            }
        } else {
            Err(Error::IncompatibleFilesystem(self.device_path()))
        }
    }

    /// Unmount a filesystem identified by an ObjectPath.
    pub async fn unmount_filesystem(&mut self, connection: &Connection) -> Result<(), Error> {
        if self.unmountable.get().is_some_and(|x| x == &false) {
            println!(
                "Skipping unmount of {} as it was not mounted via udisks.",
                self.device_path()
            );
            return Ok(());
        }
        let objectpath = ObjectPath::try_from(self.objectpath.as_str()).unwrap();
        let filesystem_proxy = FilesystemProxy::builder(connection)
            .cache_properties(zbus::CacheProperties::No)
            .path(objectpath)?
            .build()
            .await?;
        if filesystem_proxy
            .unmount(HashMap::from([("force", zvariant::Value::Bool(true))]))
            .await
            .is_ok()
        {
            println!("Successfully unmounted {}!", &self.device_path());
            self.mountpoint.take();
            Ok(())
        } else {
            eprintln!("Failed unmounting {}!", &self);
            let mountpoint: String = if let Some(mountpoint) = self.mountpoint.get() {
                mountpoint
                    .clone()
                    .into_os_string()
                    .to_string_lossy()
                    .into_owned()
            } else {
                "unknown".to_owned()
            };
            Err(Error::UnmountFailed(mountpoint))
        }
    }

    /// Find RAUC update bundles below the mountpoint
    pub async fn find_bundles(&mut self, bundle_extension: &str) -> Result<(), Error> {
        if let Some(mountpoint) = self.mountpoint.get() {
            println!(
                "Searching for RAUC update bundles with file extension '{}' in {:?}...",
                bundle_extension,
                mountpoint.as_os_str()
            );
            for entry in (mountpoint.read_dir()?).flatten() {
                let path = entry.path();
                let bundle = match path.extension() {
                    Some(extension) => match extension.to_str() {
                        Some(extension) => extension == bundle_extension,
                        None => false,
                    },
                    None => false,
                };

                if bundle {
                    println!("Detected potential update bundle: {:?}", path);
                }

                if path.exists() && path.is_file() && bundle {
                    self.bundles.push(path)
                }
            }
            Ok(())
        } else {
            Err(Error::DeviceNotMounted(self.objectpath.to_string()))
        }
    }

    /// Find RAUC update bundles below the override directory of the mountpoint
    pub async fn find_override_bundles(
        &mut self,
        bundle_extension: &str,
        override_dir: &Path,
    ) -> Result<(), Error> {
        if let Some(mountpoint) = self.mountpoint.get() {
            let path = mountpoint.join(override_dir);
            if !path.exists() {
                eprintln!(
                    "Skipping search in override location {:?} as it does not exist.",
                    path.as_os_str()
                );
                return Ok(());
            }
            if path.exists() && !path.is_dir() {
                eprintln!(
                    "Skipping search in override location {:?} as it is not a directory.",
                    path.as_os_str()
                );
                return Ok(());
            }

            println!(
                "Searching for RAUC update bundles in override location {:?}...",
                path.as_os_str()
            );

            for entry in (path.read_dir()?).flatten() {
                let path = entry.path();
                let bundle = match path.extension() {
                    Some(extension) => extension == bundle_extension,
                    None => false,
                };

                if path.exists() && path.is_file() && bundle {
                    self.override_bundles.push(path)
                }
            }
            Ok(())
        } else {
            Err(Error::DeviceNotMounted(self.objectpath.to_string()))
        }
    }
}

impl Display for Device {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(
            fmt,
            "{} ({})",
            self.objectpath(),
            if let Some(mountpoint) = self.mountpoint.get() {
                format!("mounted at {}", mountpoint.display())
            } else {
                "not mounted".to_string()
            }
        )
    }
}

#[cfg(test)]
mod test {
    use crate::config::DEVICE_REGEX;

    use super::*;
    use dbus_launch::{BusType, Daemon, Launcher};
    use rstest::{fixture, rstest};
    use testresult::TestResult;
    use zbus::{dbus_interface, ConnectionBuilder};

    struct Manager;

    #[dbus_interface(name = "org.freedesktop.UDisks2.Manager")]
    impl Manager {
        /// GetBlockDevices method
        #[dbus_interface(name = "GetBlockDevices")]
        fn get_block_devices(
            &self,
            options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
        ) -> zbus::fdo::Result<Vec<zbus::zvariant::OwnedObjectPath>> {
            tracing::debug!("GetBlockDevices called with options: {:?}", options);
            Ok(vec![zbus::zvariant::OwnedObjectPath::from(
                ObjectPath::from_str_unchecked("/org/freedesktop/UDisks2/block_devices/sda1"),
            )])
        }

        /// Version property
        #[dbus_interface(property, name = "Version")]
        fn version(&self) -> zbus::fdo::Result<String> {
            Ok("1.0.0".to_string())
        }
    }

    /// Create a dbus system bus and return it in a Result
    #[fixture]
    fn dbus_daemon() -> Daemon {
        Launcher::daemon()
            .bus_type(BusType::System)
            .launch()
            .unwrap()
    }

    #[fixture]
    async fn connection_daemon(dbus_daemon: Daemon) -> (Connection, Daemon) {
        let connection = ConnectionBuilder::address(dbus_daemon.address())
            .unwrap()
            .name("org.freedesktop.UDisks2")
            .unwrap()
            .serve_at("/org/freedesktop/UDisks2/Manager", Manager)
            .unwrap()
            .build()
            .await
            .unwrap();

        (connection, dbus_daemon)
    }

    #[rstest]
    async fn test_udisksinfo_new(#[future] connection_daemon: (Connection, Daemon)) -> TestResult {
        let (connection, daemon) = connection_daemon.await;
        assert_eq!(
            "1.0.0",
            UdisksInfo::new(&connection).await.unwrap().version()
        );
        drop(daemon);
        Ok(())
    }

    #[rstest]
    async fn test_udisksinfo_get_block_devices(
        #[future] connection_daemon: (Connection, Daemon),
    ) -> TestResult {
        let (connection, daemon) = connection_daemon.await;
        assert_eq!(
            "/org/freedesktop/UDisks2/block_devices/sda1".to_string(),
            UdisksInfo::get_block_devices(&connection, DEVICE_REGEX)
                .await
                .unwrap()[0]
                .objectpath()
        );
        drop(daemon);
        Ok(())
    }
}
