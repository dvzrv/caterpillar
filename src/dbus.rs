// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use async_std::fs::rename;
use async_std::sync::RwLock;
use config::Config;
use event_listener::Event;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::Duration;
use zbus::names::BusName;
use zbus::names::InterfaceName;
use zbus::Connection;
use zbus::SignalContext;
use zbus_macros::dbus_interface;
use zvariant::ObjectPath;
use zvariant::Type;

use crate::config::read_config;
use crate::device::Device;
use crate::device::UdisksInfo;
use crate::error::Error;
use crate::proxy::login1::ManagerProxy;
use crate::rauc::RaucInfo;
use crate::rauc::UpdateBundle;

/// State of the application
#[derive(Clone, Debug, strum::Display, strum::EnumString, PartialEq)]
#[non_exhaustive]
pub enum State {
    #[strum(to_string = "done")]
    Done(bool, usize),
    #[strum(to_string = "idle")]
    Idle(bool, usize),
    #[strum(to_string = "init")]
    Init,
    #[strum(to_string = "mounted")]
    Mounted(bool, usize),
    #[strum(to_string = "mounting")]
    Mounting(bool, usize),
    #[strum(to_string = "noupdatefound")]
    NoUpdateFound(bool, usize),
    #[strum(to_string = "searching")]
    Searching(bool, usize),
    #[strum(to_string = "skip")]
    Skip(bool, usize),
    #[strum(to_string = "unmounted")]
    Unmounted(bool, usize, bool),
    #[strum(to_string = "unmounting")]
    Unmounting(bool, usize, bool),
    #[strum(to_string = "updated")]
    Updated(bool, usize, bool),
    #[strum(to_string = "updatefound")]
    UpdateFound(bool, usize),
    #[strum(to_string = "updating")]
    Updating(bool, usize),
}

impl State {
    /// Return whether the system has been updated successfully
    ///
    /// Since users may choose not to reboot right after update, this indicator helps in distinguishing whether to allow another update attempt.
    pub fn get_updated(&self) -> bool {
        match self {
            State::Init => false,
            State::Done(updated, _)
            | State::UpdateFound(updated, _)
            | State::Idle(updated, _)
            | State::Mounting(updated, _)
            | State::Mounted(updated, _)
            | State::NoUpdateFound(updated, _)
            | State::Searching(updated, _)
            | State::Skip(updated, _)
            | State::Unmounting(updated, _, _)
            | State::Unmounted(updated, _, _)
            | State::Updating(updated, _)
            | State::Updated(updated, _, _) => updated.to_owned(),
        }
    }

    /// Return the iteration the program is currently in
    ///
    /// An iteration is defined by how often [`State::Unmounted`] has been reached
    pub fn get_iteration(&self) -> usize {
        match self {
            State::Init => 0,
            State::Done(_, iteration)
            | State::UpdateFound(_, iteration)
            | State::Idle(_, iteration)
            | State::Mounting(_, iteration)
            | State::Mounted(_, iteration)
            | State::NoUpdateFound(_, iteration)
            | State::Searching(_, iteration)
            | State::Skip(_, iteration)
            | State::Unmounting(_, iteration, _)
            | State::Unmounted(_, iteration, _)
            | State::Updating(_, iteration)
            | State::Updated(_, iteration, _) => iteration.to_owned(),
        }
    }

    /// Return whether the system is marked for reboot
    ///
    /// A system is marked for reboot, if reboot has been selected as action after successful update
    pub fn get_marked_for_reboot(&self) -> bool {
        match self {
            State::Init
            | State::Done(_, _)
            | State::UpdateFound(_, _)
            | State::Idle(_, _)
            | State::Mounting(_, _)
            | State::Mounted(_, _)
            | State::NoUpdateFound(_, _)
            | State::Searching(_, _)
            | State::Updating(_, _)
            | State::Skip(_, _) => false,
            State::Unmounting(_, _, reboot)
            | State::Unmounted(_, _, reboot)
            | State::Updated(_, _, reboot) => reboot.to_owned(),
        }
    }
}

/// An Update as it is presented over D-BUS
///
/// An update is represented by the (file) name, current (old) version of the system, the (new) version of the update
/// and whether the update is forced.
#[derive(Debug, Deserialize, PartialEq, Serialize, Type)]
struct Update {
    name: String,
    old_version: String,
    new_version: String,
    force: bool,
}

impl Update {
    /// Create an Update from an UpdateBundle and the current system version
    pub fn from_bundle(bundle: &UpdateBundle, current_version: &Version) -> Self {
        Self {
            name: bundle.path(),
            old_version: current_version.to_string(),
            new_version: bundle.version().to_string(),
            force: bundle.is_override(),
        }
    }
}

/// The state of the application
pub struct StateHandle {
    state: Arc<RwLock<State>>,
    done: Arc<Event>,
    sender: Option<Sender<State>>,
    thread: Option<JoinHandle<Result<(), Error>>>,
}

impl StateHandle {
    pub fn new(done: Event) -> Self {
        Self {
            state: Arc::new(RwLock::new(State::Init)),
            done: Arc::new(done),
            sender: None,
            thread: None,
        }
    }

    /// Clone the state Sender
    pub async fn sender_clone(&self) -> Result<Sender<State>, Error> {
        if let Some(sender) = self.sender.as_ref() {
            Ok(sender.clone())
        } else {
            Err(Error::Default("Unable to clone state Sender.".to_string()))
        }
    }

    pub async fn read_state(&self) -> State {
        self.state.read_arc().await.clone()
    }
}

/// The main application and D-Bus interface
///
/// The struct unifies the configuration, connection to other D-BUS proxies, central state, found devices and updates.
pub struct Caterpillar {
    config: Config,
    devices: Arc<RwLock<Vec<Device>>>,
    updates: Arc<RwLock<Vec<UpdateBundle>>>,
    state_handle: StateHandle,
}

impl Caterpillar {
    /// Create a new Caterpillar instance
    pub async fn new(done: Event) -> Result<Self, Error> {
        println!("Initializing Caterpillar");
        let mut caterpillar = Self {
            config: read_config().await?,
            devices: Arc::new(RwLock::new(vec![])),
            updates: Arc::new(RwLock::new(vec![])),
            state_handle: StateHandle::new(done),
        };
        caterpillar.init().await?;
        Ok(caterpillar)
    }

    /// Initialize the application's state handling
    async fn init(&mut self) -> Result<(), Error> {
        // state
        let (sender, mut receiver): (Sender<State>, Receiver<State>) = channel(2);
        let state_sender = sender.clone();
        let state_lock = self.state_handle.state.clone();
        let done_lock = self.state_handle.done.clone();

        // devices and updates
        let devices_lock = self.devices.clone();
        let updates_lock = self.updates.clone();

        // config data
        let autorun = self.config().get_bool("autorun")?;

        // test connections to other services
        let connection = Connection::system().await?;
        test_connections(&connection).await?;

        // start task that receives state changes, persists and acts on them
        self.state_handle.sender = Some(sender);
        self.state_handle.thread = Some(spawn(async move {
            let mut exit = false;
            state_sender.send(State::Idle(false, 0)).await?;
            while !exit {
                if let Ok(state) = receiver.try_recv() {
                    println!("Entering state: {}", &state);
                    // let previous_state = state_lock.read_arc().await;
                    {
                        // update the state
                        let mut state_write = state_lock.write_arc().await;
                        *state_write = state;
                    }

                    // match against a clone of the state so we do not block
                    let state_read = state_lock.read_arc().await.clone();
                    match state_read {
                        State::Init
                        | State::Mounting(_, _)
                        | State::Mounted(_, _)
                        | State::Searching(_, _)
                        | State::Updating(_, _) => {}
                        State::Done(_, _) => {
                            exit = true;
                            done_lock.notify(1);
                        }
                        State::UpdateFound(_, iteration) => {
                            let updates = updates_lock.read_arc().await;
                            let connection = Connection::system().await?;
                            let rauc_info = RaucInfo::new(&connection).await?;

                            // signal that we have found an update
                            println!("Signal over D-Bus, that an update is found");
                            Caterpillar::update_found(
                                &SignalContext::from_parts(
                                    connection.to_owned(),
                                    ObjectPath::from_str_unchecked("/de/sleepmap/Caterpillar"),
                                ),
                                vec![Update::from_bundle(
                                    &updates[0],
                                    rauc_info.version().unwrap_or(&Version::new(0, 0, 0)),
                                )],
                            )
                            .await?;

                            // if this is the first iteration (i.e. boot) and configured to do so, install update and reboot
                            if iteration == 1 && autorun {
                                println!("Running in non-interactive mode. Install...");
                                connection
                                    .call_method(
                                        Some(
                                            BusName::try_from("de.sleepmap.Caterpillar")
                                                .map_err(|x| Error::Default(x.to_string()))?,
                                        ),
                                        ObjectPath::try_from("/de/sleepmap/Caterpillar")
                                            .map_err(|x| Error::Default(x.to_string()))?,
                                        Some(
                                            InterfaceName::try_from("de.sleepmap.Caterpillar")
                                                .map_err(|x| Error::Default(x.to_string()))?,
                                        ),
                                        "InstallUpdate",
                                        &(true, true),
                                    )
                                    .await?;
                            }
                        }
                        State::NoUpdateFound(updated, iteration) => {
                            state_sender
                                .send(State::Unmounting(updated, iteration, false))
                                .await?;
                        }
                        State::Idle(updated, iteration) => {
                            {
                                // increment our iteration
                                let mut state_write = state_lock.write_arc().await;
                                *state_write = State::Idle(updated, iteration + 1);
                            }
                        }
                        State::Skip(updated, iteration) => {
                            state_sender
                                .send(State::Unmounting(updated, iteration, false))
                                .await?;
                        }
                        State::Unmounting(updated, iteration, reboot) => {
                            let connection = Connection::system().await?;
                            let mut devices = devices_lock.write_arc().await;
                            for device in devices.iter_mut() {
                                if device.is_mounted() {
                                    device.unmount_filesystem(&connection).await?;
                                }
                            }
                            state_sender
                                .send(State::Unmounted(updated, iteration, reboot))
                                .await?;
                        }
                        State::Unmounted(updated, iteration, reboot) => {
                            // if this is the first iteration, successfully updated and configured to do so, reboot
                            if updated && ((iteration == 1 && autorun) || reboot) {
                                let connection = Connection::system().await?;
                                println!("Connecting to logind over dbus...");
                                let login_proxy = ManagerProxy::new(&connection).await?;
                                println!("Rebooting...");
                                login_proxy.reboot(false).await?;
                            // return to idle state if not updated or no reboot is wanted
                            } else {
                                state_sender.send(State::Idle(updated, iteration)).await?;
                            }

                            // reset devices and updates lists
                            {
                                let mut devices_write = devices_lock.write_arc().await;
                                *devices_write = vec![];
                            }
                            {
                                let mut updates_write = updates_lock.write_arc().await;
                                *updates_write = vec![];
                            }
                        }
                        State::Updated(_, iteration, reboot) => {
                            // mark ourselves as updated
                            state_sender
                                .send(State::Unmounting(true, iteration, reboot))
                                .await?;
                        }
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
            Ok(())
        }));
        Ok(())
    }

    /// Return a reference to the application's configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Return a reference to the done Event of the application
    pub fn done(&self) -> &Event {
        &self.state_handle.done
    }

    /// Return the optional UpdateBundle, that the application found
    async fn get_update(&self) -> Option<UpdateBundle> {
        self.updates
            .read()
            .await
            .iter()
            .last()
            .map(|bundle| bundle.to_owned())
    }
}

#[dbus_interface(name = "de.sleepmap.Caterpillar")]
impl Caterpillar {
    /// Trigger the search for an update
    ///
    /// It is advised to subscribe to the `UpdateFound` signal before calling this method.
    pub async fn search_for_update(&self) -> zbus::fdo::Result<()> {
        println!("Search for update...");
        let state = self.state_handle.read_state().await;
        match state {
            State::Idle(updated, iteration) if !updated => {
                let state_sender = self
                    .state_handle
                    .sender_clone()
                    .await
                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                let devices_lock = self.devices.clone();
                let (device_regex, bundle_extension, override_dir) = (
                    self.config
                        .get_string("device_regex")
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?,
                    self.config
                        .get_string("bundle_extension")
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?,
                    self.config
                        .get_string("override_dir")
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?,
                );
                let updates_lock = self.updates.clone();
                let connection = Connection::system().await?;

                // run background task that mounts available devices and searches for compatible updates
                spawn(async move {
                    state_sender
                        .send(State::Mounting(updated, iteration))
                        .await
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                    let mut devices = devices_lock.write_arc().await;
                    // setup the devices (mounts)
                    *devices = mount_and_search_devices(
                        &connection,
                        &device_regex,
                        &bundle_extension,
                        &override_dir,
                    )
                    .await
                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;

                    state_sender
                        .send(State::Mounted(updated, iteration))
                        .await
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;

                    let mut updates = updates_lock.write_arc().await;
                    let rauc_info = RaucInfo::new(&connection)
                        .await
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                    state_sender
                        .send(State::Searching(updated, iteration))
                        .await
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                    // search for a compatible update bundle
                    match get_update_bundle(&connection, &rauc_info, &devices)
                        .await
                        .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?
                    {
                        Some(bundle) => {
                            println!(
                                "Found {}update {}",
                                if bundle.is_override() {
                                    "override "
                                } else {
                                    " "
                                },
                                bundle.path()
                            );
                            updates.push(bundle);
                            state_sender
                                .send(State::UpdateFound(updated, iteration))
                                .await
                                .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                        }
                        None => state_sender
                            .send(State::NoUpdateFound(updated, iteration))
                            .await
                            .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?,
                    }
                    Ok::<(), zbus::fdo::Error>(())
                });
                Ok(())
            }
            _ => Err(zbus::fdo::Error::AccessDenied(format!(
                "Already in state {}",
                state
            ))),
        }
    }

    /// Trigger the installation of an update
    ///
    /// The parameters to this method provide information on whether to update (b) and whether to reboot afterwards (b)
    async fn install_update(&self, update: bool, reboot: bool) -> zbus::fdo::Result<()> {
        let state = self.state_handle.read_state().await;
        match state {
            State::UpdateFound(updated, iteration) if !updated && update => {
                let state_sender = self
                    .state_handle
                    .sender_clone()
                    .await
                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                if let Some(bundle) = self.get_update().await {
                    spawn(async move {
                        println!(
                            "Install update {} and {}reboot",
                            &bundle,
                            if reboot { "" } else { "do not " }
                        );
                        state_sender
                            .send(State::Updating(updated, iteration))
                            .await
                            .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;

                        match bundle
                            .install(&Connection::system().await?)
                            .await
                            .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))
                        {
                            Ok(()) => {
                                if bundle.is_override() {
                                    println!("Disabling override bundle {}", bundle.path());
                                    if let Err(error) = rename(
                                        bundle.path(),
                                        format!("{}.installed", bundle.path()),
                                    )
                                    .await
                                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))
                                    {
                                        eprintln!("{}", error);
                                        return Err(error);
                                    }
                                }
                                state_sender
                                    .send(State::Updated(updated, iteration, reboot))
                                    .await
                                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                            }
                            Err(error) => {
                                eprintln!("{}", error);
                                return Err(error);
                            }
                        }
                        Ok(())
                    });
                } else {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "{}",
                        Error::NoUpdateBundle
                    )));
                }
            }
            State::NoUpdateFound(updated, iteration) | State::UpdateFound(updated, iteration)
                if !update =>
            {
                let state_sender = self
                    .state_handle
                    .sender_clone()
                    .await
                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
                state_sender
                    .send(State::Skip(updated, iteration))
                    .await
                    .map_err(|x| zbus::fdo::Error::Failed(x.to_string()))?;
            }
            _ => {
                if state.get_updated() {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "{}",
                        Error::WrongState(
                            "System is updated already, waiting for reboot".to_string()
                        )
                    )));
                } else {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "{}",
                        Error::WrongState(format!("{}", state))
                    )));
                }
            }
        }
        Ok(())
    }

    /// The internal state of Caterpillar
    ///
    /// One of
    /// - "done"
    /// - "idle"
    /// - "init"
    /// - "mounted"
    /// - "mounting"
    /// - "noupdatefound"
    /// - "searching"
    /// - "skip"
    /// - "unmounted"
    /// - "unmounting"
    /// - "updated"
    /// - "updatefound"
    /// - "updating"
    #[dbus_interface(property)]
    async fn state(&self) -> String {
        format!("{}", self.state_handle.read_state().await)
    }

    /// Whether the system has been successfully updated
    #[dbus_interface(property)]
    async fn updated(&self) -> bool {
        self.state_handle.read_state().await.get_updated()
    }

    /// Whether the system has been marked for reboot when requesting the installation of an update
    #[dbus_interface(property)]
    async fn marked_for_reboot(&self) -> bool {
        self.state_handle.read_state().await.get_marked_for_reboot()
    }

    /// A signal, broadcasting information on found updates
    ///
    /// The update is returned in an array of length one.
    /// The update information consists of the absolute filename (s),
    /// the current version of the system (s),
    /// the new version (s)
    /// and whether the update is an override (b)
    #[dbus_interface(signal)]
    async fn update_found(ctxt: &SignalContext<'_>, update: Vec<Update>) -> zbus::Result<()>;
}

/// Test connections to UdisksInfo, RaucInfo and ManagerProxy instances in a Result
async fn test_connections(connection: &Connection) -> Result<(), Error> {
    println!("Connecting to logind over dbus...");
    if let Err(error) = ManagerProxy::new(connection).await {
        return Err(Error::Dbus(error));
    };

    println!("Connecting to Udisks2 over dbus...");
    match UdisksInfo::new(connection).await {
        Ok(proxy) => {
            println!("Communicating with Udisks2 {}", proxy.version());
        }
        Err(error) => return Err(error),
    }

    println!("Connecting to RAUC over dbus...");
    match RaucInfo::new(connection).await {
        Ok(proxy) => {
            println!("{}", proxy);
            println!("RAUC slot info:");
            for slot in proxy.slots() {
                println!("{}", slot);
            }
        }
        Err(error) => return Err(error),
    }

    Ok(())
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
