#!/bin/bash
# SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
# SPDX-License-Identifier: LGPL-3.0-or-later

set -eu

readonly state_dir="${1:-/mnt}"
readonly state_booted_a="$state_dir/ab_tests_booted_a"
readonly state_booted_b="$state_dir/ab_tests_booted_b"

if (( $(id -u) > 0 )); then
  >&2 printf "This script must be run as root!\n"
  exit 1
fi

if [[ ! -d "$state_dir" ]]; then
  printf "Creating state dir %s!\n" "$state_dir"
  mkdir -vp -- "$state_dir"
fi

if [[ ! -f "$state_dir/ab_tests_booted_a" ]]; then
  printf "0\n" > "$state_booted_a"
fi

if [[ ! -f "$state_dir/ab_tests_booted_b" ]]; then
  printf "0\n" > "$state_booted_b"
fi

current_boot_a_count=$(< "$state_booted_a")
current_boot_b_count=$(< "$state_booted_b")

if df | grep /dev/sda4 > /dev/null; then
  booted_a=1
  printf "%s\n" "$(( booted_a + current_boot_a_count ))" > "$state_booted_a"
  current_boot_a_count=$(< "$state_booted_a")
else
  booted_a=0
fi

if df | grep /dev/sda5 > /dev/null; then
  booted_b=1
  printf "%s\n" "$(( booted_b + current_boot_b_count ))" > "$state_booted_b"
  current_boot_b_count=$(< "$state_booted_b")
else
  booted_b=0
fi

printf "Boot count A/B: %s/ %s\n" "$current_boot_a_count" "$current_boot_b_count"
