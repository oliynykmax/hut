//! Minimal HTTP client using curl subprocess.
//! Zero Rust HTTP dependencies — just shells out to `curl`.

use std::path::Path;
use std::process::Command;

use crate::error::{HutError, HutResult};

/// HTTP GET a URL and return the response body as bytes.
/// Uses curl subprocess for simplicity and zero-dependency footprint.
pub fn http_get(url: &str) -> HutResult<Vec<u8>> {
    let output = Command::new("curl")
        .args(["-sSfL", "--connect-timeout", "30", "--max-time", "120", url])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| HutError::Other(format!("curl failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HutError::Other(format!("HTTP GET {url}: {stderr}")));
    }

    Ok(output.stdout)
}

/// HTTP GET a URL and stream the response body to a file.
/// Shows progress via curl's built-in progress bar.
pub fn http_download(url: &str, dest: &Path) -> HutResult<()> {
    let status = Command::new("curl")
        .args([
            "-sSfL",
            "--connect-timeout",
            "30",
            "--max-time",
            "300",
            "-o",
            &dest.display().to_string(),
            url,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| HutError::Other(format!("curl failed: {e}")))?;

    if !status.success() {
        return Err(HutError::Other(format!("HTTP download {url} failed")));
    }

    Ok(())
}
