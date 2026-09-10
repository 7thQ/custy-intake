#!/usr/bin/env bash
# Launches the CST Shop server for real use: opens the firewall (ufw)
# so customer phones on the building network can reach it, and closes
# that port again on shutdown so it isn't left open when nobody's
# running the shop.
#
# You'll be prompted twice: once for the launch password below (gates
# who can start this script at all), and once for your own system
# sudo password (needed for the ufw firewall commands — that's your
# normal Linux password, not the one below).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# Reuses the admin portal password by default so there's one fewer
# credential to remember. Change this if you'd rather it be separate.
LAUNCH_PASSWORD="Cst#1Shop"
PORT=3000
BINARY="./target/release/customer-intake"

if ! command -v ufw >/dev/null 2>&1; then
    echo "ufw isn't installed — can't manage the firewall. Aborting." >&2
    exit 1
fi

FIREWALL_RULE_ADDED=0
SERVER_PID=""
CLEANED_UP=0

cleanup() {
    if [[ "$CLEANED_UP" -eq 1 ]]; then
        return
    fi
    CLEANED_UP=1

    echo
    echo "Shutting down..."
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -TERM "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi

    if [[ "$FIREWALL_RULE_ADDED" -eq 1 ]]; then
        echo "Closing port $PORT..."
        sudo ufw delete allow "$PORT"/tcp
    fi
    echo "Stopped."
}
trap cleanup INT TERM EXIT

read -r -s -p "Launch password: " entered_password
echo
if [[ "$entered_password" != "$LAUNCH_PASSWORD" ]]; then
    echo "Incorrect password." >&2
    exit 1
fi

echo "Opening port $PORT for the building network..."
if sudo ufw status | grep -qE "^${PORT}(/tcp)?[[:space:]]"; then
    echo "Port $PORT is already open — leaving the existing rule alone."
else
    sudo ufw allow "$PORT"/tcp
    FIREWALL_RULE_ADDED=1
fi

echo "Building (release)..."
cargo build --release -p customer-intake

echo "Starting server. Press Ctrl+C to stop and close the port again."
"$BINARY" &
SERVER_PID=$!

wait "$SERVER_PID"
