use std::io::BufRead;

use ark::update::Version;

pub const MAX_EVENT_LINE_BYTES: usize = 16 * 1024;

pub fn read_bounded_line<R: BufRead>(reader: &mut R, max: usize) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|error| format!("could not read update event: {error}"))?;
        if buffer.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(buffer.len(), |index| index + 1);
        if bytes.len().saturating_add(count) > max {
            return Err("update event line is too large".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "update event is not UTF-8".into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateEvent {
    CheckStarted,
    UpdateAvailable {
        version: Version,
        size: u64,
        notes_url: String,
    },
    UpToDate {
        version: Version,
    },
    DownloadStarted {
        version: Version,
    },
    Progress(u8),
    Staged {
        version: Version,
    },
    Ready,
    Installed {
        version: Version,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Default)]
pub struct UpdateEventParser {
    pending: Option<PendingEvent>,
}

impl UpdateEventParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_line(&mut self, line: &str) -> Result<Option<UpdateEvent>, String> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            return Ok(None);
        }
        if line.chars().any(char::is_control) {
            return Err("update event contains a control character".into());
        }
        if let Some(name) = line.strip_prefix("event:") {
            let previous = self.pending.take().map(PendingEvent::finish).transpose()?;
            self.pending = Some(PendingEvent::new(name.trim()));
            return Ok(previous);
        }
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| "update field appeared before an event".to_owned())?;
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| "update field is missing ':'".to_owned())?;
        pending.set(key.trim(), value.trim())?;
        Ok(None)
    }

    pub fn finish(&mut self) -> Result<Option<UpdateEvent>, String> {
        self.pending.take().map(PendingEvent::finish).transpose()
    }
}

#[derive(Debug)]
struct PendingEvent {
    name: String,
    version: Option<String>,
    size: Option<u64>,
    notes_url: Option<String>,
    percent: Option<u8>,
    code: Option<String>,
    message: Option<String>,
}

impl PendingEvent {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: None,
            size: None,
            notes_url: None,
            percent: None,
            code: None,
            message: None,
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if value.is_empty() || value.len() > 2048 {
            return Err(format!("invalid update field: {key}"));
        }
        match key {
            "version" => set_once(&mut self.version, value.to_owned()),
            "size" => {
                let size = value
                    .parse::<u64>()
                    .map_err(|_| "invalid update size".to_owned())?;
                if size == 0 {
                    return Err("update size must be positive".into());
                }
                set_once(&mut self.size, size)
            }
            "notes-url" => set_once(&mut self.notes_url, value.to_owned()),
            "percent" => {
                let percent = value
                    .parse::<u8>()
                    .map_err(|_| "invalid update progress".to_owned())?;
                if percent > 100 {
                    return Err("update progress exceeds 100".into());
                }
                set_once(&mut self.percent, percent)
            }
            "code" => set_once(&mut self.code, value.to_owned()),
            "message" => set_once(&mut self.message, value.to_owned()),
            _ => Ok(()),
        }
    }

    fn finish(self) -> Result<UpdateEvent, String> {
        let version = || {
            self.version
                .as_deref()
                .ok_or_else(|| "update event is missing version".to_owned())
                .and_then(|value| Version::parse(value).map_err(|error| error.to_string()))
        };
        match self.name.as_str() {
            "check-start" => Ok(UpdateEvent::CheckStarted),
            "update-available" => Ok(UpdateEvent::UpdateAvailable {
                version: version()?,
                size: self
                    .size
                    .ok_or_else(|| "update event is missing size".to_owned())?,
                notes_url: self
                    .notes_url
                    .ok_or_else(|| "update event is missing notes-url".to_owned())?,
            }),
            "up-to-date" => Ok(UpdateEvent::UpToDate {
                version: version()?,
            }),
            "download-start" => Ok(UpdateEvent::DownloadStarted {
                version: version()?,
            }),
            "progress" => {
                Ok(UpdateEvent::Progress(self.percent.ok_or_else(|| {
                    "progress event is missing percent".to_owned()
                })?))
            }
            "staged" => Ok(UpdateEvent::Staged {
                version: version()?,
            }),
            "ready" => Ok(UpdateEvent::Ready),
            "installed" => Ok(UpdateEvent::Installed {
                version: version()?,
            }),
            "error" => Ok(UpdateEvent::Error {
                code: self
                    .code
                    .ok_or_else(|| "error event is missing code".to_owned())?,
                message: self
                    .message
                    .ok_or_else(|| "error event is missing message".to_owned())?,
            }),
            other => Err(format!("unknown update event: {other}")),
        }
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.replace(value).is_some() {
        Err("duplicate update event field".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_helper_transcript() {
        let mut parser = UpdateEventParser::new();
        let mut events = Vec::new();
        for line in [
            "event: check-start",
            "event: update-available",
            "version: 0.1.1",
            "size: 4212334",
            "notes-url: https://example.com/releases/0.1.1",
            "event: download-start",
            "version: 0.1.1",
            "event: progress",
            "percent: 42",
            "event: staged",
            "version: 0.1.1",
            "event: ready",
        ] {
            if let Some(event) = parser.push_line(line).unwrap() {
                events.push(event);
            }
        }
        if let Some(event) = parser.finish().unwrap() {
            events.push(event);
        }
        assert_eq!(
            events,
            vec![
                UpdateEvent::CheckStarted,
                UpdateEvent::UpdateAvailable {
                    version: Version::parse("0.1.1").unwrap(),
                    size: 4_212_334,
                    notes_url: "https://example.com/releases/0.1.1".into(),
                },
                UpdateEvent::DownloadStarted {
                    version: Version::parse("0.1.1").unwrap(),
                },
                UpdateEvent::Progress(42),
                UpdateEvent::Staged {
                    version: Version::parse("0.1.1").unwrap(),
                },
                UpdateEvent::Ready,
            ]
        );
    }

    #[test]
    fn parses_terminal_success_and_error_events() {
        let mut parser = UpdateEventParser::new();
        assert_eq!(parser.push_line("event: up-to-date").unwrap(), None);
        assert!(parser.push_line("version: 0.1.0").unwrap().is_none());
        assert_eq!(
            parser.push_line("event: installed").unwrap(),
            Some(UpdateEvent::UpToDate {
                version: Version::parse("0.1.0").unwrap(),
            })
        );
        assert!(parser.push_line("version: 0.1.0").unwrap().is_none());
        assert_eq!(
            parser.push_line("event: error").unwrap(),
            Some(UpdateEvent::Installed {
                version: Version::parse("0.1.0").unwrap(),
            })
        );
        assert!(parser
            .push_line("code: signature-invalid")
            .unwrap()
            .is_none());
        assert!(parser
            .push_line("message: bad signature")
            .unwrap()
            .is_none());
        assert_eq!(
            parser.finish().unwrap(),
            Some(UpdateEvent::Error {
                code: "signature-invalid".into(),
                message: "bad signature".into(),
            })
        );
    }

    #[test]
    fn rejects_missing_fields_and_out_of_range_progress() {
        let mut parser = UpdateEventParser::new();
        parser.push_line("event: progress").unwrap();
        assert!(parser.push_line("percent: 101").is_err());

        let mut parser = UpdateEventParser::new();
        parser.push_line("event: update-available").unwrap();
        parser.push_line("version: 0.1.1").unwrap();
        assert!(parser.finish().is_err());
    }

    #[test]
    fn bounded_line_reader_rejects_unterminated_oversized_input() {
        let mut input = std::io::Cursor::new(b"event: ready\n");
        assert_eq!(
            read_bounded_line(&mut input, MAX_EVENT_LINE_BYTES).unwrap(),
            Some("event: ready".to_owned())
        );
        assert_eq!(
            read_bounded_line(&mut input, MAX_EVENT_LINE_BYTES).unwrap(),
            None
        );

        let mut input = std::io::Cursor::new(vec![b'x'; MAX_EVENT_LINE_BYTES + 1]);
        assert!(read_bounded_line(&mut input, MAX_EVENT_LINE_BYTES).is_err());
    }
}
