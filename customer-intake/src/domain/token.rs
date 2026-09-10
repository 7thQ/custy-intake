use rand::RngExt;

/// A fresh 256-bit token, hex-encoded. Used for both portal session
/// cookies and sign-in session tokens — same randomness requirement
/// either way (unguessable, no collisions in practice).
pub fn random_hex_token() -> String {
    let mut rng = rand::rng();
    format!("{:032x}{:032x}", rng.random::<u128>(), rng.random::<u128>())
}
