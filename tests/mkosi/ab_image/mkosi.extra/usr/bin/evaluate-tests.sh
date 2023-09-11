#!/bin/bash
# SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
# SPDX-License-Identifier: LGPL-3.0-or-later

set -eu

readonly state_dir="${1:-/mnt}"
readonly log_file="$state_dir/evaluate_tests.log"
readonly state_booted_a="$state_dir/ab_tests_booted_a"
readonly state_booted_b="$state_dir/ab_tests_booted_b"
readonly reevaluate="${REEVALUATE:-0}"
booted_a=0
booted_b=0

touch "$log_file"

if (( $(id -u) > 0 )); then
  >&2 printf "This script must be run as root!\n" | tee -a "$log_file"
  exit 1
fi

current_boot_a_count=$(< "$state_booted_a")
current_boot_b_count=$(< "$state_booted_b")

if df | grep /dev/sda4 > /dev/null; then
  booted_a=1
fi

if df | grep /dev/sda5 > /dev/null; then
  booted_b=1
fi

printf "Boot count A/B: %s/ %s\n" "$current_boot_a_count" "$current_boot_b_count" | tee -a "$log_file"

if (( booted_a == 1 )); then
  printf "Booted into A\n" | tee -a "$log_file"
fi

if (( booted_b == 1 )); then
  printf "Booted into B\n" | tee -a "$log_file"
fi

if (( reevaluate == 1 )); then
  printf "Running test evaluation after payload\n" | tee -a "$log_file"
fi

config="$(< "/run/credentials/@system/test_environment")"
printf "Evaluating test case '%s'\n" "$config" | tee -a "$log_file"

case "$config" in
  "success_single")
    printf "Checking if slot B is booted...\n" | tee -a "$log_file"
    if (( booted_b > 0 )); then
      printf "Booted into slot B, powering off...\n" | tee -a "$log_file"
      umount /mnt
      sleep 1
      systemctl poweroff
    else
      printf "Not booted into slot B, exiting...\n" | tee -a "$log_file"
      exit 1
    fi
    ;;
  "success_multiple")
    target_version="2.0.0"

    printf "Checking if slot B is booted...\n" | tee -a "$log_file"
    if (( booted_b > 0 )); then
      printf "Booted into slot B, getting version...\n" | tee -a "$log_file"
      version="$(jq '.slots[] | select(."rootfs.1") | ."rootfs.1".slot_status.bundle.version' /mnt/rauc-status.json | sed 's/"//g')"

      if [[ "$version" == "$target_version" ]]; then
        printf "Booted into slot B with target version %s, powering off...\n" "$target_version" | tee -a "$log_file"
        systemctl poweroff
      else
        printf "Slot B uses version %s instead of %s...\n" "$version" "$target_version" | tee -a "$log_file"
        exit 1
      fi
    else
      printf "Not booted into slot B, exiting...\n" | tee -a "$log_file"
      exit 1
    fi
    ;;
  "success_override")
    target_version="1.0.0"

    printf "Checking if slot B is booted...\n" | tee -a "$log_file"
    if (( booted_b > 0 )); then
      printf "Booted into slot B, getting version...\n" | tee -a "$log_file"
      version="$(jq '.slots[] | select(."rootfs.1") | ."rootfs.1".slot_status.bundle.version' /mnt/rauc-status.json | sed 's/"//g')"

      if [[ "$version" == "$target_version" ]]; then
        printf "Booted into slot B with target version %s, powering off...\n" "$target_version" | tee -a "$log_file"
        systemctl poweroff
      else
        printf "Slot B uses version %s instead of %s...\n" "$version" "$target_version" | tee -a "$log_file"
        exit 1
      fi
    else
      printf "Not booted into slot B, exiting...\n" | tee -a "$log_file"
      exit 1
    fi
    ;;
  "skip_empty")
    if (( reevaluate == 1 )) && (( booted_a == 1 )) && (( current_boot_b_count == 0 )); then
      printf "Slot B never booted and currently booted into slot A, powering off...\n" | tee -a "$log_file"
      systemctl poweroff
    else
      printf "Test success criteria not met, exiting...\n" | tee -a "$log_file"
      exit 1
    fi
    ;;
  *)
    >&2 printf "Unknown configuration '%s' encountered!\n" "$config" | tee -a "$log_file"
    exit 1
    ;;
esac
