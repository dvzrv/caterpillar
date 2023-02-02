// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use async_std::stream::StreamExt;
use futures::try_join;
use once_cell::sync::OnceCell;
use semver::Version;
use zbus::Connection;
use zvariant::OwnedValue;

use crate::error::Error;
use crate::proxy::rauc::InstallerProxy;

/// RAUC update bundle
///
/// RAUC update bundles are exposed by their `path`, the `variant` they are compatible with and their `version`.
/// The information apart from the location is obtained from an `InstallerProxy`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateBundle {
    path: PathBuf,
    compatible: String,
    version: Version,
    is_override: bool,
}

impl UpdateBundle {
    /// Create a new UpdateBundle
    pub async fn new(
        path: &Path,
        is_override: bool,
        connection: &Connection,
    ) -> Result<UpdateBundle, Error> {
        let path_str = if let Some(path_str) = path.to_str() {
            path_str
        } else {
            return Err(Error::BundlePath(path.into()));
        };
        let installer_proxy = InstallerProxy::new(connection).await?;

        match &installer_proxy.info(path_str).await {
            Ok(bundle_info) => match Version::parse(bundle_info.1.as_str()) {
                Ok(version) => Ok(UpdateBundle {
                    path: path.into(),
                    compatible: bundle_info.0.to_owned(),
                    version,
                    is_override,
                }),
                Err(error) => Err(Error::BundleVersion(
                    path_str.to_string(),
                    bundle_info.1.to_owned(),
                    error.to_string(),
                )),
            },
            Err(error) => Err(Error::BundleInfo(path_str.to_string(), error.to_string())),
        }
    }

    /// Get the compatible of the bundle
    pub fn compatible(&self) -> &str {
        self.compatible.as_str()
    }

    /// Get the path of the bundle
    pub fn path(&self) -> String {
        self.path.display().to_string()
    }

    /// Install the update bundle
    pub async fn install(&self, connection: &Connection) -> Result<(), Error> {
        println!("Installing update bundle {}", self.path());
        let installer_proxy = InstallerProxy::new(connection).await?;
        let mut completed = installer_proxy.receive_completed().await?;
        let mut failed = false;
        installer_proxy
            .install_bundle(self.path.to_str().unwrap(), HashMap::new())
            .await?;

        while let Some(signal) = completed.next().await {
            if let Ok(args) = signal.args() {
                if args.result().is_positive() {
                    failed = true;
                }
                break;
            }
        }

        if failed {
            let error_message = installer_proxy.last_error().await?;
            eprintln!("RAUC error: {}", &error_message);
            Err(Error::UpdateFailed(error_message))
        } else {
            Ok(())
        }
    }

    /// Return a reference to the UpdateBundle's Version
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Return a bool specifying whether the UpdateBundle is an override
    pub fn is_override(&self) -> bool {
        self.is_override
    }
}

impl Display for UpdateBundle {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(
            fmt,
            "{} (variant: {}; version: {})",
            self.path.to_str().unwrap(),
            self.compatible,
            self.version
        )
    }
}

impl Ord for UpdateBundle {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.compatible.cmp(&other.compatible) {
            Ordering::Equal => self.version.cmp(&other.version),
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
        }
    }
}

impl PartialOrd for UpdateBundle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Information on a slot on a system
#[derive(Debug)]
pub struct Slot {
    /// whether this slot is the primary
    primary: bool,
    /// whether this slot was booted from
    booted: bool,
    /// the name of the slot
    name: String,
    /// The version of the slot (if any)
    version: Option<Version>,
    /// the raw slot status
    status: Option<HashMap<String, String>>,
}

impl Slot {
    /// Create a new Slot
    pub fn new(
        primary: bool,
        booted: bool,
        name: &str,
        version: Option<Version>,
        status: Option<HashMap<String, String>>,
    ) -> Self {
        Slot {
            primary,
            booted,
            name: name.to_string(),
            version,
            status,
        }
    }

    /// Return the optional Version as String
    pub fn version_string(&self) -> String {
        match self.version.as_ref() {
            Some(version) => version.to_string(),
            None => "".to_string(),
        }
    }

    /// Return the raw slot status as optional HashMap
    pub fn status(&self) -> Option<&HashMap<String, String>> {
        self.status.as_ref()
    }
}

impl Display for Slot {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(
            fmt,
            "Slot \"{}\" (primary: {}; booted: {}; version: {}) status: {:?}",
            self.name,
            if self.primary { "✔️" } else { "❌" },
            if self.booted { "✔️" } else { "❌" },
            self.version_string(),
            self.status(),
        )
    }
}

/// Information about a RAUC instance
pub struct RaucInfo {
    /// operational state of RAUC
    operation: OnceCell<String>,
    /// The compatible the RAUC instance supports
    compatible: String,
    /// The variant the RAUC instance supports
    variant: String,
    /// The current boot slot of the RAUC instance
    boot_slot: String,
    /// The version of the primary slot (if any)
    version: Option<Version>,
    /// The slots the RAUC instance tracks
    slots: Vec<Slot>,
}

impl RaucInfo {
    /// Create a new RaucInfo and return it in a Result
    pub async fn new(connection: &Connection) -> Result<Self, Error> {
        let installer_proxy = InstallerProxy::new(connection).await?;
        match try_join!(
            installer_proxy.operation(),
            installer_proxy.compatible(),
            installer_proxy.variant(),
            installer_proxy.boot_slot(),
            installer_proxy.get_primary(),
            installer_proxy.get_slot_status(),
        ) {
            Ok((operation, compatible, variant, boot_slot, primary, slot_status)) => {
                let mut slots = vec![];
                let mut system_version = None;

                for slot_name in get_slot_names(&slot_status) {
                    let raw_slot_status = unwrap_slot_status(&slot_name, &slot_status);
                    let slot_booted = raw_slot_status.as_ref().is_some_and(|x| {
                        x.get("state")
                            .is_some_and(|x| x == "booted" || x == "active")
                    });
                    let slot_version = match raw_slot_status.as_ref() {
                        Some(map) => match map.get("bundle.version") {
                            Some(map_version) => match Version::parse(map_version) {
                                Ok(version) => Some(version),
                                Err(error) => {
                                    return Err(Error::SlotVersion(
                                        map_version.to_owned(),
                                        slot_name,
                                        error.to_string(),
                                    ))
                                }
                            },
                            None => None,
                        },
                        None => None,
                    };
                    let slot_primary = slot_name == primary;

                    // if this slot is the primary and has a version, expose it as the system version
                    if slot_primary && slot_version.is_some() {
                        system_version = slot_version.clone();
                    }

                    slots.push(Slot::new(
                        slot_primary,
                        slot_booted.to_owned(),
                        slot_name.as_str(),
                        slot_version,
                        raw_slot_status,
                    ));
                }

                Ok(RaucInfo {
                    operation: OnceCell::from(operation),
                    compatible,
                    variant,
                    boot_slot,
                    version: system_version,
                    slots,
                })
            }
            Err(error) => {
                eprintln!(
                    "An error occurred trying to communicate with RAUC via dbus: {}",
                    error
                );
                Err(Error::Dbus(error))
            }
        }
    }

    /// Get the operation status of the RAUC instance
    pub fn operation(&self) -> Option<&str> {
        if let Some(operation) = self.operation.get() {
            Some(operation.as_str())
        } else {
            None
        }
    }

    /// Get the compatible of the RAUC instance
    pub fn compatible(&self) -> &str {
        &self.compatible
    }

    /// Get the variant of the RAUC instance
    pub fn variant(&self) -> &str {
        &self.variant
    }

    /// Get reference to the optional system Version
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Return the optional Version as String
    pub fn version_string(&self) -> String {
        match self.version.as_ref() {
            Some(version) => version.to_string(),
            None => "".to_string(),
        }
    }

    /// Get the boot slot of the RAUC instance
    pub fn boot_slot(&self) -> &str {
        &self.boot_slot
    }

    /// Get the slots of the RAUC instance
    pub fn slots(&self) -> &Vec<Slot> {
        self.slots.as_ref()
    }
}

impl Display for RaucInfo {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(
            fmt,
            "RAUC ({}) for compatible \"{}\" (variant: \"{}\") on boot slot \"{}\" in version \"{}\"",
            if let Some(operation) = self.operation() {
                operation
            } else {
                ""
            },
            self.compatible(),
            self.variant(),
            self.boot_slot(),
            self.version_string(),
        )
    }
}

/// Get the unwrapped status of a specific slot
///
/// Unpacks the zbus variants to native owned types and returns them as a HashMap of Strings.
fn unwrap_slot_status(
    key: &str,
    status: &[(String, HashMap<String, OwnedValue>)],
) -> Option<HashMap<String, String>> {
    status
        .iter()
        .filter_map(|(slot, map)| {
            if slot.eq(key) {
                Some(
                    map.iter()
                        .map(|(status_key, value)| {
                            (
                                status_key.clone(),
                                value.clone().try_into().unwrap_or_default(),
                            )
                        })
                        .collect(),
                )
            } else {
                None
            }
        })
        .last()
}

/// Get the names of all slots from the slot status
fn get_slot_names(status: &[(String, HashMap<String, OwnedValue>)]) -> Vec<String> {
    status.iter().map(|x| x.0.clone()).collect()
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use super::*;
    use dbus_launch::BusType;
    use dbus_launch::Daemon;
    use dbus_launch::Launcher;
    use rstest::fixture;
    use rstest::rstest;
    use testdir::testdir;
    use testresult::TestResult;
    use zbus::dbus_interface;
    use zbus::Connection;
    use zbus::ConnectionBuilder;
    use zbus::SignalContext;
    use zvariant::Value;

    struct Installer {
        pub completed_return: i32,
    }

    #[dbus_interface(name = "de.pengutronix.rauc.Installer")]
    impl Installer {
        #[dbus_interface(name = "Info")]
        fn info(&self, bundle: &str) -> zbus::fdo::Result<(String, String)> {
            tracing::debug!("Info called");
            match bundle {
                _ if bundle.ends_with("foo.raucb") => {
                    Ok(("foo_variant".to_string(), "0.1.0".to_string()))
                }
                _ if bundle.ends_with("foo1.raucb") => {
                    Ok(("foo_variant".to_string(), "1.0.0".to_string()))
                }
                _ if bundle.ends_with("foo2.raucb") => {
                    Ok(("foo_variant".to_string(), "2.0.0".to_string()))
                }
                _ => Err(zbus::fdo::Error::Failed("not found".to_string())),
            }
        }

        #[dbus_interface(property, name = "Operation")]
        fn operation(&self) -> zbus::fdo::Result<String> {
            Ok("ok".to_string())
        }

        #[dbus_interface(property, name = "Compatible")]
        fn compatible(&self) -> zbus::fdo::Result<String> {
            Ok("compatible_system".to_string())
        }

        /// InstallBundle method
        #[dbus_interface(name = "InstallBundle")]
        async fn install_bundle(
            &self,
            _source: &str,
            _args: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
            #[zbus(signal_context)] ctxt: SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            Installer::completed(&ctxt, self.completed_return).await?;
            Ok(())
        }

        #[dbus_interface(property, name = "Variant")]
        fn variant(&self) -> zbus::fdo::Result<String> {
            Ok("foo".to_string())
        }

        #[dbus_interface(property, name = "BootSlot")]
        fn boot_slot(&self) -> zbus::fdo::Result<String> {
            Ok("A".to_string())
        }

        #[dbus_interface(name = "GetPrimary")]
        fn get_primary(&self) -> zbus::fdo::Result<String> {
            Ok("A".to_string())
        }

        #[dbus_interface(name = "GetSlotStatus")]
        fn get_slot_status(
            &self,
        ) -> zbus::fdo::Result<
            Vec<(
                String,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            )>,
        > {
            Ok(vec![(
                "A".to_string(),
                HashMap::from([(
                    "A".to_string(),
                    zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from("foo")),
                )]),
            )])
        }

        #[dbus_interface(property, name = "LastError")]
        fn last_error(&self) -> zbus::fdo::Result<String> {
            Ok("error".to_string())
        }

        /// Completed signal
        #[dbus_interface(signal)]
        async fn completed(ctxt: &SignalContext<'_>, result: i32) -> zbus::Result<()>;
    }

    /// Create a Path for a fake update bundle
    #[fixture]
    fn bundle_path() -> PathBuf {
        let bundle = testdir!().join("foo.raucb");
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(&bundle)
            .unwrap();
        bundle
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
            .name("de.pengutronix.rauc")
            .unwrap()
            .serve_at(
                "/",
                Installer {
                    completed_return: 0,
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();

        (connection, dbus_daemon)
    }

    #[fixture]
    async fn connection_daemon_installer_fail(dbus_daemon: Daemon) -> (Connection, Daemon) {
        let connection = ConnectionBuilder::address(dbus_daemon.address())
            .unwrap()
            .name("de.pengutronix.rauc")
            .unwrap()
            .serve_at(
                "/",
                Installer {
                    completed_return: 1,
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();

        (connection, dbus_daemon)
    }

    #[rstest]
    #[case(
        &[(String::from("A"), HashMap::from([(String::from("foo"), Value::Str("value".into()).into())]))],
        vec![String::from("A")],
    )]
    fn test_get_slot_names(
        #[case] status: &[(String, HashMap<String, OwnedValue>)],
        #[case] slots: Vec<String>,
    ) {
        assert_eq!(slots, get_slot_names(status));
    }

    #[rstest]
    #[case(
        "A",
        &[(String::from("A"), HashMap::from([(String::from("foo"), Value::Str("value".into()).into())]))],
        Some(HashMap::from([(String::from("foo"), String::from("value"))])),
    )]
    fn test_unwrap_slot_status(
        #[case] key: &str,
        #[case] status: &[(String, HashMap<String, OwnedValue>)],
        #[case] return_value: Option<HashMap<String, String>>,
    ) {
        assert_eq!(return_value, unwrap_slot_status(key, status));
    }

    #[rstest]
    async fn test_updatebundle_new(
        #[future] connection_daemon: (Connection, Daemon),
        bundle_path: PathBuf,
    ) -> TestResult {
        let (connection, daemon) = connection_daemon.await;
        UpdateBundle::new(&bundle_path, false, &connection).await?;
        drop(daemon);
        Ok(())
    }

    #[rstest]
    async fn test_updatebundle_install(
        #[future] connection_daemon: (Connection, Daemon),
        bundle_path: PathBuf,
    ) -> TestResult {
        let (connection, daemon) = connection_daemon.await;
        let bundle = UpdateBundle::new(&bundle_path, false, &connection).await?;
        bundle.install(&connection).await?;
        drop(daemon);
        Ok(())
    }

    #[rstest]
    async fn test_updatebundle_install_fail(
        #[future] connection_daemon_installer_fail: (Connection, Daemon),
        bundle_path: PathBuf,
    ) -> TestResult {
        let (connection, daemon) = connection_daemon_installer_fail.await;
        let bundle = UpdateBundle::new(&bundle_path, false, &connection).await?;
        let update_result = bundle.install(&connection).await;
        assert!(update_result
            .is_err_and(|x| format!("{:?}", x) == "UpdateFailed(\"error\")".to_string()));
        drop(daemon);
        Ok(())
    }

    #[rstest]
    async fn test_raucinfo_new(#[future] connection_daemon: (Connection, Daemon)) -> TestResult {
        let (connection, daemon) = connection_daemon.await;
        let raucinfo = RaucInfo::new(&connection).await?;
        assert_eq!(raucinfo.variant(), "foo");
        assert_eq!(raucinfo.operation(), Some("ok"));
        drop(daemon);
        Ok(())
    }
}
