// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use event_listener::Event;
use zbus::names::BusName;
use zbus::names::InterfaceName;
use zbus::ConnectionBuilder;
use zvariant::ObjectPath;

mod config;
mod dbus;
mod device;
mod error;
mod macros;
mod proxy;
mod rauc;

use dbus::Caterpillar;
use error::Error;

#[tokio::main]
pub async fn main() -> Result<(), Error> {
    println!(
        "Starting {} {}.",
        env!("CARGO_BIN_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let caterpillar = Caterpillar::new(Event::new()).await?;
    let mut listener = caterpillar.done().listen();
    let autorun = caterpillar.config().get_bool("autorun")?;

    println!("Making Caterpillar available on D-Bus");
    let connection = ConnectionBuilder::system()?
        .name("de.sleepmap.Caterpillar")?
        .serve_at("/de/sleepmap/Caterpillar", caterpillar)?
        .build()
        .await?;

    // autorun caterpillar
    if autorun {
        println!("Non-interactive mode on first run");
        connection
            .call_method(
                Some(BusName::try_from("de.sleepmap.Caterpillar").unwrap()),
                ObjectPath::try_from("/de/sleepmap/Caterpillar").unwrap(),
                Some(InterfaceName::try_from("de.sleepmap.Caterpillar").unwrap()),
                "SearchForUpdate",
                &(),
            )
            .await?;
    }

    listener.as_mut().wait();

    Ok(())
}
