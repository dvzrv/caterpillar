// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::convert::From;
use std::io;
use std::path::PathBuf;
use std::string::FromUtf8Error;

use config::ConfigError;

/// An error that could occur when caterpillar runs
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A device is already mounted at a mountpoint
    #[error("Device {0} is already mounted at mountpoint {1}")]
    AlreadyMounted(String, String),
    /// A device is not yet mounted
    #[error("Device {0} is not yet mounted")]
    DeviceNotMounted(String),
    /// A block device is a base device (partition number 0)
    #[error("Device {0} is a base device without a partition")]
    IsBaseDevice(String),
    /// A block device is not compatible (not a filesystem)
    #[error("Device {0} does not have a filesystem")]
    IncompatibleBlockDevice(String),
    /// A filesystem is not compatible (not one of CompatibleFilesystem)
    #[error("Device {0} does not have a compatible filesystem")]
    IncompatibleFilesystem(String),
    /// A device path is invalid
    #[error("Device path {0} is not valid")]
    InvalidDevicePath(String),
    /// A problem with dbus
    #[error("A problem occurred while communicating over dbus: {0}")]
    Dbus(zbus::Error),
    /// A file issue
    #[error("An error occurred reading or writing a file: {0}")]
    File(io::Error),
    /// Failed retrieving information on a RAUC update bundle
    #[error("Unable to get information on a RAUC update bundle {0}")]
    BundleInfo(String, String),
    /// A bundle path is invalid
    #[error("RAUC update bundle path {0} is invalid")]
    BundlePath(PathBuf),
    /// A bundle version is invalid
    #[error("Version ({0}) of RAUC update bundle {1} is invalid: {2}")]
    BundleVersion(String, String, String),
    /// A slot version is invalid
    #[error("Version ({0}) of slot {1} is invalid: {2}")]
    SlotVersion(String, String, String),
    #[error("An error occurred reading configuration: {0}")]
    Config(ConfigError),
    /// No compatible update bundle is found
    #[error("No compatible RAUC update bundle found")]
    NoUpdateBundle,
    /// String conversion issues
    #[error("An error occurred trying to convert a string: {0}")]
    String(FromUtf8Error),
    /// There is more than one override bundle
    #[error("There is more than one override update bundle")]
    TooManyOverrides(Vec<PathBuf>),
    /// Unmounting a filesystem failed
    #[error("Unmounting mountpoint {0} failed")]
    UnmountFailed(String),
    /// Installing an update bundle failed
    #[error("Update failed: {0}")]
    UpdateFailed(String),
}

impl From<FromUtf8Error> for Error {
    fn from(value: FromUtf8Error) -> Self {
        Error::String(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::File(value)
    }
}

impl From<zbus::Error> for Error {
    fn from(err: zbus::Error) -> Error {
        Error::Dbus(err)
    }
}

impl From<ConfigError> for Error {
    fn from(err: ConfigError) -> Error {
        Error::Config(err)
    }
}
