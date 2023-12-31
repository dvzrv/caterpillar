// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT
use config::{Config, ConfigError, File};

pub const DEVICE_REGEX: &str = "^/org/freedesktop/UDisks2/block_devices/sd[a-z]{1}[1-9]{1}[0-9]*?$";

/// Read the configuration for the application
///
/// This uses built-in defaults, which can be overridden with an optional configuration file found in /etc/caterpillar/caterpillar.toml
pub async fn read_config() -> Result<Config, ConfigError> {
    Config::builder()
        .set_default("autorun", true)?
        .set_default("bundle_extension", "raucb")?
        .set_default("device_regex", DEVICE_REGEX)?
        .set_default("override_dir", "override")?
        .add_source(File::with_name("/etc/caterpillar/caterpillar").required(false))
        .add_source(config::Environment::with_prefix("CATERPILLAR"))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::macros;
    use rstest::rstest;

    #[tokio::test]
    #[rstest]
    async fn test_read_config() {
        let config = read_config().await.unwrap();
        let device_regex_string = config.get_string("device_regex").unwrap();
        assert!(macros::regex_once!(device_regex_string)
            .is_match("/org/freedesktop/UDisks2/block_devices/sda1"));
    }
}
