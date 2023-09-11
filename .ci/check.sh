#!/bin/bash
#
# SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
# SPDX-License-Identifier: LGPL-3.0-or-later

set -euxo pipefail

cargo fmt -- --check
cargo clippy --all -- -D warnings
cargo test -- --skip integration
codespell
reuse lint
