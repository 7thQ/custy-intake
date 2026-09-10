use std::path::PathBuf;

/// Where the actual filled-out PDFs land — a stand-in for sending a
/// print job, until there's a printer to send to. Each submission gets
/// its own subfolder (named with its session token) containing the
/// filled page(s).
pub fn completed_forms_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/completed_forms"))
}
