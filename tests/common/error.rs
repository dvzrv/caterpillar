// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT

use assert_cmd::assert::AssertError;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TestError {
    #[error("Running an external command failed: {0}")]
    ExternalCommand(AssertError),
    #[error("Something is missing: {0}")]
    Missing(String),
    #[error("An external command is missing: {0}")]
    CommandMissing(which::Error),
    #[error("An I/O error occurred: {0}")]
    IO(std::io::Error),
}

impl From<AssertError> for TestError {
    fn from(value: AssertError) -> Self {
        TestError::ExternalCommand(value)
    }
}

impl From<which::Error> for TestError {
    fn from(value: which::Error) -> Self {
        TestError::CommandMissing(value)
    }
}

impl From<std::io::Error> for TestError {
    fn from(value: std::io::Error) -> Self {
        TestError::IO(value)
    }
}
