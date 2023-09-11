// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use rstest::fixture;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::path::PathBuf;
use which::which;

/// A command available in PATH
pub struct Cmd {
    path: PathBuf,
}

impl Cmd {
    pub fn new(name: String) -> Result<Self, which::Error> {
        Ok(Self {
            path: which(&name)?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Display for Cmd {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.path().display())
    }
}

#[fixture]
/// The guestmount command
pub fn cmd_guestmount() -> Result<Cmd, which::Error> {
    Cmd::new("guestmount".to_string())
}

#[fixture]
/// The guestunmount command
pub fn cmd_guestunmount() -> Result<Cmd, which::Error> {
    Cmd::new("guestunmount".to_string())
}

#[fixture]
/// The mkosi command
pub fn cmd_mkosi() -> Result<Cmd, which::Error> {
    Cmd::new("mkosi".to_string())
}

#[fixture]
/// The openssl command
pub fn cmd_openssl() -> Result<Cmd, which::Error> {
    Cmd::new("openssl".to_string())
}

#[fixture]
/// The qemu-img command
pub fn cmd_qemu_img() -> Result<Cmd, which::Error> {
    Cmd::new("qemu-img".to_string())
}

#[fixture]
/// The qemu-system-x86_64 command
pub fn cmd_qemu_system() -> Result<Cmd, which::Error> {
    Cmd::new("qemu-system-x86_64".to_string())
}

#[fixture]
/// The rauc command
pub fn cmd_rauc() -> Result<Cmd, which::Error> {
    Cmd::new("rauc".to_string())
}
