#!/usr/bin/env bash
# Launches the CST Shop server for real use.
#
# You'll be prompted twice: once for the launch password below (gates
# who can start this script at all), and once for your own system
# sudo password — the server binary itself handles opening the
# firewall (and closing it again on Ctrl+C), and will ask for that
# password when it does.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# Reuses the admin portal password by default so there's one fewer
# credential to remember. Change this if you'd rather it be separate.
LAUNCH_PASSWORD="Cst#1Shop"
BINARY="./target/release/customer-intake"

read -r -s -p "Launch password: " entered_password
echo
if [[ "$entered_password" != "$LAUNCH_PASSWORD" ]]; then
    echo "Incorrect password." >&2
    exit 1
fi

echo "Building (release)..."
cargo build --release -p customer-intake

# exec, not backgrounding + wait: the terminal's Ctrl+C then reaches
# the server binary directly, with no wrapper script left in between
# to duplicate its firewall-cleanup logic.
exec "$BINARY"
