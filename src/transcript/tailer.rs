use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::transcript::events::{TranscriptEvent, parse_transcript_line};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptLocator {
    claude_home: PathBuf,
}

impl TranscriptLocator {
    pub fn new(claude_home: impl Into<PathBuf>) -> Self {
        Self {
            claude_home: claude_home.into(),
        }
    }

    pub fn default_home() -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("home directory unavailable")?;
        Ok(Self::new(home.join(".claude")))
    }

    pub fn find_transcript(&self, session_id: &str) -> anyhow::Result<Option<PathBuf>> {
        let projects = self.claude_home.join("projects");
        if !projects.exists() {
            return Ok(None);
        }

        let target = format!("{session_id}.jsonl");
        find_file_named(&projects, &target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptTailer {
    session_id: String,
    path: PathBuf,
    offset: u64,
}

impl TranscriptTailer {
    pub fn from_path(session_id: impl Into<String>, path: impl AsRef<Path>) -> Self {
        Self {
            session_id: session_id.into(),
            path: path.as_ref().to_path_buf(),
            offset: 0,
        }
    }

    pub fn from_path_at_end(
        session_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let offset = std::fs::metadata(&path)
            .with_context(|| format!("stat transcript {}", path.display()))?
            .len();
        Ok(Self {
            session_id: session_id.into(),
            path,
            offset,
        })
    }

    pub fn from_locator(
        session_id: impl Into<String>,
        locator: &TranscriptLocator,
    ) -> anyhow::Result<Option<Self>> {
        let session_id = session_id.into();
        Ok(locator
            .find_transcript(&session_id)?
            .map(|path| Self::from_path(session_id, path)))
    }

    pub fn from_locator_at_end(
        session_id: impl Into<String>,
        locator: &TranscriptLocator,
    ) -> anyhow::Result<Option<Self>> {
        let session_id = session_id.into();
        locator
            .find_transcript(&session_id)?
            .map(|path| Self::from_path_at_end(session_id, path))
            .transpose()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn poll(&mut self) -> anyhow::Result<Vec<TranscriptEvent>> {
        let mut file = File::open(&self.path)
            .with_context(|| format!("open transcript {}", self.path.display()))?;
        file.seek(SeekFrom::Start(self.offset))
            .with_context(|| format!("seek transcript {}", self.path.display()))?;

        let mut reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .with_context(|| format!("read transcript {}", self.path.display()))?;
            if read == 0 {
                break;
            }
            self.offset += read as u64;
            for event in parse_transcript_line(line.trim_end_matches(['\r', '\n']))? {
                match event.session_id() {
                    Some(session_id) if session_id == self.session_id => events.push(event),
                    None => events.push(event),
                    _ => {}
                }
            }
        }
        Ok(events)
    }
}

fn find_file_named(root: &Path, filename: &str) -> anyhow::Result<Option<PathBuf>> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path)
            .with_context(|| format!("read transcript directory {}", path.display()))?
        {
            let entry = entry?;
            let entry_path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry_path);
            } else if file_type.is_file()
                && entry_path.file_name().and_then(|name| name.to_str()) == Some(filename)
            {
                return Ok(Some(entry_path));
            }
        }
    }
    Ok(None)
}
