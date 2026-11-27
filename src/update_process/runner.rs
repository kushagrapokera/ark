use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use winit::event_loop::EventLoopProxy;

use crate::update_protocol::{read_bounded_line, UpdateEventParser, MAX_EVENT_LINE_BYTES};
use crate::watch::Wake;

use super::UpdateCommand;

pub(super) fn spawn_helper(
    helper: &PathBuf,
    command: UpdateCommand,
    version: Option<&str>,
    generation: u64,
    proxy: EventLoopProxy<Wake>,
) -> Result<(), String> {
    let mut process = Command::new(helper);
    match command {
        UpdateCommand::Check => {
            process.arg("check");
        }
        UpdateCommand::Stage => {
            process.arg("stage");
        }
        UpdateCommand::Apply => {
            process.args(["apply", "--version", version.unwrap_or_default()]);
        }
    }
    let mut child = process
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("could not start ark-updater: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ark-updater stdout was not piped".to_owned())?;
    let child = Arc::new(Mutex::new(child));
    let waiter = child.clone();
    let spawned = thread::Builder::new()
        .name("ark-updater-reader".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut parser = UpdateEventParser::new();
            let mut protocol_error = None;
            loop {
                match read_bounded_line(&mut reader, MAX_EVENT_LINE_BYTES) {
                    Ok(Some(line)) => match parser.push_line(&line) {
                        Ok(Some(event)) => send_wake(
                            &proxy,
                            Wake::UpdateEvent {
                                generation,
                                event: Box::new(event),
                            },
                        ),
                        Ok(None) => {}
                        Err(error) => {
                            protocol_error = Some(error);
                            break;
                        }
                    },
                    Ok(None) => break,
                    Err(error) => {
                        protocol_error = Some(error);
                        break;
                    }
                }
            }
            if protocol_error.is_none() {
                match parser.finish() {
                    Ok(Some(event)) => send_wake(
                        &proxy,
                        Wake::UpdateEvent {
                            generation,
                            event: Box::new(event),
                        },
                    ),
                    Ok(None) => {}
                    Err(error) => protocol_error = Some(error),
                }
            }
            if protocol_error.is_some() {
                if let Ok(mut child) = waiter.lock() {
                    if let Err(error) = child.kill() {
                        log::debug!("could not stop malformed update helper: {error}");
                    }
                }
            }
            let status = match waiter.lock() {
                Ok(mut child) => child.wait(),
                Err(_) => Err(std::io::Error::other("update helper lock was poisoned")),
            };
            let (mut success, wait_error) = match status {
                Ok(status) => (
                    status.success(),
                    (!status.success()).then(|| format!("exit status {status}")),
                ),
                Err(error) => (
                    false,
                    Some(format!("could not wait for ark-updater: {error}")),
                ),
            };
            if protocol_error.is_some() {
                success = false;
            }
            send_wake(
                &proxy,
                Wake::UpdateFinished {
                    generation,
                    command,
                    success,
                    error: protocol_error.or(wait_error),
                },
            );
        });
    if spawned.is_err() {
        if let Ok(mut child) = child.lock() {
            if let Err(error) = child.kill() {
                log::debug!("could not stop update helper after thread spawn failure: {error}");
            }
            if let Err(error) = child.wait() {
                log::debug!("could not reap update helper after thread spawn failure: {error}");
            }
        }
        return Err("could not start update helper reader thread".into());
    }
    Ok(())
}

fn send_wake(proxy: &EventLoopProxy<Wake>, event: Wake) {
    if let Err(error) = proxy.send_event(event) {
        log::debug!("update event loop is closed: {error}");
    }
}
