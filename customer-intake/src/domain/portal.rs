use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rand::RngExt;

/// A password-gated section of the site. Every non-customer-facing page
/// (admin, and each of the CST/Neets/LRA queues) is one of these,
/// distinguished only by its id, its own hardcoded password, and where
/// an already-authenticated visitor lands. There's no user database —
/// just one shared door per portal — so a real account system would
/// replace `password` with something looked up per-user instead.
#[derive(Debug, Clone, Copy)]
pub struct Portal {
    /// Used as both the URL path segment (`/{id}/login`) and the
    /// session cookie name (`{id}_session`), so it must be URL- and
    /// cookie-name-safe: lowercase ascii, no spaces.
    pub id: &'static str,
    pub display_name: &'static str,
    pub password: &'static str,
    /// Where an authenticated visitor is sent after logging in, or
    /// after hitting `/{id}` directly.
    pub home_path: &'static str,
}

pub const ADMIN: Portal = Portal {
    id: "admin",
    display_name: "Admin",
    password: "Cst#1Shop",
    home_path: "/admin/dashboard",
};

pub const CST: Portal = Portal {
    id: "cst",
    display_name: "CST",
    password: "cstP0tal",
    home_path: "/cst/queue",
};

pub const NEETS: Portal = Portal {
    id: "neets",
    display_name: "Neets",
    password: "Neets",
    home_path: "/neets/queue",
};

pub const LRA: Portal = Portal {
    id: "lra",
    display_name: "LRA",
    password: "LRAP0rtal",
    home_path: "/lra/queue",
};

pub const ALL: [Portal; 4] = [ADMIN, CST, NEETS, LRA];

/// Valid session tokens for every portal, keyed by `Portal::id`.
/// In-memory only — restarting the server signs everyone out, which is
/// fine for hardcoded-password doors with no persistent accounts.
#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<HashMap<&'static str, HashSet<String>>>>);

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verifies `password` against `portal` and, if it matches, mints
    /// and stores a fresh session token for it.
    pub fn login(&self, portal: &Portal, password: &str) -> Option<String> {
        if password != portal.password {
            return None;
        }
        let token = generate_token();
        self.0
            .lock()
            .unwrap()
            .entry(portal.id)
            .or_default()
            .insert(token.clone());
        Some(token)
    }

    pub fn is_valid(&self, portal_id: &str, token: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(portal_id)
            .is_some_and(|tokens| tokens.contains(token))
    }

    pub fn logout(&self, portal_id: &str, token: &str) {
        if let Some(tokens) = self.0.lock().unwrap().get_mut(portal_id) {
            tokens.remove(token);
        }
    }
}

/// A fresh 256-bit token, hex-encoded.
fn generate_token() -> String {
    let mut rng = rand::rng();
    format!("{:032x}{:032x}", rng.random::<u128>(), rng.random::<u128>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_rejects_wrong_password() {
        let sessions = Sessions::new();
        assert!(sessions.login(&CST, "wrong").is_none());
    }

    #[test]
    fn login_then_logout_round_trip() {
        let sessions = Sessions::new();
        let token = sessions.login(&CST, CST.password).expect("correct password logs in");
        assert!(sessions.is_valid(CST.id, &token));

        sessions.logout(CST.id, &token);
        assert!(!sessions.is_valid(CST.id, &token));
    }

    /// A token minted for one portal must not authenticate a different
    /// one — sessions are scoped per portal, not shared.
    #[test]
    fn sessions_are_scoped_per_portal() {
        let sessions = Sessions::new();
        let cst_token = sessions.login(&CST, CST.password).unwrap();
        assert!(sessions.is_valid(CST.id, &cst_token));
        assert!(!sessions.is_valid(NEETS.id, &cst_token));
    }

    #[test]
    fn portal_ids_are_all_distinct() {
        let ids: HashSet<&str> = ALL.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), ALL.len());
    }
}
