use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ark::update::UpdateError;

pub(super) fn insecure_http_allowed() -> bool {
    env_flag("ARK_ALLOW_INSECURE_HTTP")
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value == "1")
}

pub(super) fn download(
    url: &str,
    destination: &Path,
    max: u64,
    expected: Option<u64>,
) -> Result<(), UpdateError> {
    if !url.starts_with("https://") && !(insecure_http_allowed() && url.starts_with("http://")) {
        return Err(UpdateError::InsecureUrl(url.to_owned()));
    }
    let tool = downloader()?;
    let mut file = File::create(destination)?;
    let mut command = Command::new(tool);
    match tool {
        "curl" => {
            command.args(["--fail", "--location", "--silent", "--show-error"]);
            if url.starts_with("https://") {
                command.args(["--proto", "=https", "--tlsv1.2"]);
            }
            command.arg(url);
        }
        "wget" => {
            command.args(["--quiet", "--output-document=-"]);
            if url.starts_with("https://") {
                command.arg("--https-only");
            }
            command.arg(url);
        }
        _ => unreachable!("downloader is validated"),
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| UpdateError::CommandFailed {
            tool: tool.into(),
            detail: error.to_string(),
        })?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    let mut last_percent = 0u8;
    loop {
        let count = match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        };
        total = total.saturating_add(count as u64);
        if total > max {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UpdateError::DownloadTooLarge { size: total, max });
        }
        if let Err(error) = file.write_all(&buffer[..count]) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        if let Some(expected) = expected {
            let percent = ((total.saturating_mul(100) / expected.max(1)).min(100) as u8).min(99);
            if percent >= last_percent + 5 {
                println!("event: progress");
                println!("percent: {percent}");
                last_percent = percent;
            }
        }
    }
    drop(stdout);
    let output = child
        .wait_with_output()
        .map_err(|error| UpdateError::CommandFailed {
            tool: tool.into(),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(UpdateError::CommandFailed {
            tool: tool.into(),
            detail,
        });
    }
    if let Some(expected) = expected {
        if total != expected {
            return Err(UpdateError::SizeMismatch {
                expected,
                actual: total,
            });
        }
    }
    if expected.is_some() {
        println!("event: progress");
        println!("percent: 100");
    }
    Ok(())
}

fn downloader() -> Result<&'static str, UpdateError> {
    if let Some(tool) = std::env::var_os("ARK_DOWNLOAD_TOOL") {
        let tool = tool.to_string_lossy();
        if tool == "curl" || tool == "wget" {
            return Ok(if tool == "curl" { "curl" } else { "wget" });
        }
        return Err(UpdateError::MissingTool("curl or wget"));
    }
    if command_exists("curl") {
        Ok("curl")
    } else if command_exists("wget") {
        Ok("wget")
    } else {
        Err(UpdateError::MissingTool("curl or wget"))
    }
}

fn command_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(super) fn verify_checksum(path: &Path, expected: &str) -> Result<(), UpdateError> {
    let output = run_output("sha256sum", &[path.to_string_lossy().as_ref()])?;
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    if actual != expected {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(())
}

pub(super) fn verify_signature(archive: &Path, signature: &Path) -> Result<(), UpdateError> {
    let runtime_key = std::env::var_os("ARK_UPDATE_PUBLIC_KEY").map(PathBuf::from);
    let mut temporary_key = None;
    let public_key = if let Some(path) = runtime_key {
        path
    } else {
        let embedded = option_env!("ARK_UPDATE_PUBLIC_KEY_PEM")
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                UpdateError::SignatureInvalid(
                    "no update public key is embedded or configured".into(),
                )
            })?;
        let path = archive.with_file_name(".ark-update-public.pem");
        std::fs::write(&path, embedded).map_err(|error| {
            UpdateError::SignatureInvalid(format!(
                "could not materialize embedded update public key: {error}"
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&path)
                .map_err(UpdateError::from)?
                .permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&path, permissions).map_err(UpdateError::from)?;
        }
        temporary_key = Some(path.clone());
        path
    };
    let result = Command::new("openssl")
        .args(["pkeyutl", "-verify", "-pubin", "-rawin"])
        .arg("-inkey")
        .arg(&public_key)
        .arg("-in")
        .arg(archive)
        .arg("-sigfile")
        .arg(signature)
        .output();
    if temporary_key.is_some() {
        if let Err(error) = std::fs::remove_file(&public_key) {
            super::warn(format!(
                "could not remove temporary update public key {}: {error}",
                public_key.display()
            ));
        }
    }
    let output = result.map_err(|error| UpdateError::CommandFailed {
        tool: "openssl".into(),
        detail: error.to_string(),
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(UpdateError::SignatureInvalid(detail));
    }
    Ok(())
}

pub(super) fn run_output_bounded(
    tool: &str,
    args: &[&str],
    max: u64,
) -> Result<Output, UpdateError> {
    let mut child = Command::new(tool)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| UpdateError::CommandFailed {
            tool: tool.into(),
            detail: error.to_string(),
        })?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(UpdateError::CommandFailed {
                    tool: tool.into(),
                    detail: error.to_string(),
                });
            }
        };
        if bytes.len() as u64 + count as u64 > max {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UpdateError::DownloadTooLarge {
                size: bytes.len() as u64 + count as u64,
                max,
            });
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    drop(stdout);
    let mut output = child
        .wait_with_output()
        .map_err(|error| UpdateError::CommandFailed {
            tool: tool.into(),
            detail: error.to_string(),
        })?;
    output.stdout = bytes;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(UpdateError::CommandFailed {
            tool: tool.into(),
            detail,
        });
    }
    Ok(output)
}

pub(super) fn run_output(tool: &str, args: &[&str]) -> Result<Output, UpdateError> {
    let output =
        Command::new(tool)
            .args(args)
            .output()
            .map_err(|error| UpdateError::CommandFailed {
                tool: tool.into(),
                detail: error.to_string(),
            })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(UpdateError::CommandFailed {
            tool: tool.into(),
            detail,
        });
    }
    Ok(output)
}
