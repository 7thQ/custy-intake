//! Manages the `ufw` firewall rule that lets phones on the building
//! network reach this server. The server itself binds `0.0.0.0`, but
//! the OS firewall still blocks unsolicited incoming connections by
//! default — that's what stops a customer's phone from loading the
//! sign-in page even though the server is running fine and the QR
//! code points at a reachable address.
//!
//! `sudo` is invoked directly and left to prompt for the password on
//! its own familiar `[sudo] password for ...:` terminal prompt,
//! rather than this program reading the password itself. That prompt
//! is what people already recognize and know to type into their
//! terminal (not into some other window) — a homegrown prompt here
//! is just a chance to get that confused.

use std::process::Command;

use anyhow::{Context, Result, bail};

/// The port this server listens on and the port `ufw` needs to allow.
pub const PORT: u16 = 3000;

/// Tracks whether *this* process is the one that opened the port, so
/// shutdown only closes a rule it added. A rule that was already
/// there — opened by hand, or left behind by a previous run that
/// didn't shut down cleanly — is left alone.
pub struct FirewallGuard {
    opened_by_us: bool,
}

impl FirewallGuard {
    /// Opens `PORT` for incoming TCP via `ufw`, prompting for the
    /// computer's sudo password on the terminal if one is needed.
    /// Blocking: run this off the async runtime (e.g. via
    /// `spawn_blocking`) since it waits on terminal input.
    pub fn open() -> Result<Self> {
        if !ufw_installed() {
            println!(
                "ufw isn't installed — skipping firewall setup. Make sure port {PORT} is reachable from the building network some other way."
            );
            return Ok(Self { opened_by_us: false });
        }

        if port_already_allowed()? {
            println!("Port {PORT} is already open — leaving the existing firewall rule alone.");
            return Ok(Self { opened_by_us: false });
        }

        println!("Opening port {PORT} so phones on the building network can reach this server.");
        println!("(Running `sudo ufw allow {PORT}/tcp` — enter your computer password at the prompt below.)");
        let status = Command::new("sudo")
            .args(["ufw", "allow", &format!("{PORT}/tcp")])
            .status()
            .context("failed to run `sudo ufw allow`")?;

        if !status.success() {
            bail!(
                "`sudo ufw allow {PORT}/tcp` did not succeed — phones on the network won't be able to reach the sign-in page"
            );
        }

        // ufw doesn't error on its own if the rule failed to actually
        // apply for some other reason — confirm it's really there
        // rather than trusting the exit code alone.
        if !port_already_allowed()? {
            bail!(
                "`sudo ufw allow {PORT}/tcp` reported success but the rule isn't showing up in `sudo ufw status` — phones on the network still won't be able to reach the sign-in page"
            );
        }

        println!("Port {PORT} is open.");
        Ok(Self { opened_by_us: true })
    }

    /// Reverses [`open`](Self::open) — removes the rule this process
    /// added, if it added one. Safe to call even when `open` skipped
    /// adding a rule (ufw missing, or the rule already existed).
    pub fn close(&self) {
        if !self.opened_by_us {
            return;
        }

        println!("Closing port {PORT}...");
        // --force skips ufw's interactive "Proceeding with operation
        // (y|n)?" confirmation — nothing is there to answer it during
        // an automated shutdown, so without this it would hang.
        let result = Command::new("sudo")
            .args(["ufw", "--force", "delete", "allow", &format!("{PORT}/tcp")])
            .status();

        match result {
            Ok(status) if status.success() => {}
            _ => eprintln!(
                "Warning: couldn't close port {PORT} automatically — run `sudo ufw delete allow {PORT}/tcp` yourself."
            ),
        }
    }
}

fn ufw_installed() -> bool {
    Command::new("ufw").arg("--version").output().is_ok()
}

fn port_already_allowed() -> Result<bool> {
    let output = Command::new("sudo")
        .args(["ufw", "status"])
        .output()
        .context("failed to run `sudo ufw status`")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|line| rule_line_matches(line, PORT)))
}

/// True if `line` is a `ufw status` row allowing exactly `port`
/// (e.g. `"3000/tcp    ALLOW    Anywhere"`) — not some other rule.
fn rule_line_matches(line: &str, port: u16) -> bool {
    let Some(first_field) = line.split_whitespace().next() else {
        return false;
    };
    first_field == port.to_string() || first_field == format!("{port}/tcp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_real_ufw_status_line() {
        assert!(rule_line_matches("3000/tcp                   ALLOW       Anywhere", 3000));
    }

    #[test]
    fn does_not_match_a_different_port() {
        assert!(!rule_line_matches("22/tcp                     ALLOW       Anywhere", 3000));
    }

    #[test]
    fn does_not_match_unrelated_lines() {
        assert!(!rule_line_matches("Status: active", 3000));
        assert!(!rule_line_matches("", 3000));
        assert!(!rule_line_matches("30000/tcp                  ALLOW       Anywhere", 3000));
    }
}
