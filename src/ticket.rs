// Ticket data structure and markdown serialization
use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum TicketStatus {
    Open,
    Picked,
    Done,
    Blocked,
}

impl Serialize for TicketStatus {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TicketStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketStatus::Open => write!(f, "open"),
            TicketStatus::Picked => write!(f, "picked"),
            TicketStatus::Done => write!(f, "done"),
            TicketStatus::Blocked => write!(f, "blocked"),
        }
    }
}

impl std::str::FromStr for TicketStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "open" => Ok(TicketStatus::Open),
            "picked" | "in-progress" | "inprogress" | "in_progress" => Ok(TicketStatus::Picked),
            "done" => Ok(TicketStatus::Done),
            "blocked" => Ok(TicketStatus::Blocked),
            other => anyhow::bail!(
                "Unknown status: '{}'. Valid: open, picked, done, blocked",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub status: TicketStatus,
    /// Free-form tags. The creator's client name is always the first tag.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Session ID of the session that has picked this ticket.
    pub assignee: Option<String>,
    pub created: NaiveDate,
    pub updated: NaiveDate,
    pub description: String,
}

impl Ticket {
    pub fn new(id: String, title: String, tags: Vec<String>) -> Self {
        let today = chrono::Local::now().date_naive();
        Ticket {
            id,
            title,
            status: TicketStatus::Open,
            tags,
            assignee: None,
            created: today,
            updated: today,
            description: String::new(),
        }
    }

    /// Parse a ticket from markdown content (frontmatter + body)
    pub fn from_markdown(content: &str) -> Result<Self> {
        let (frontmatter, body) = parse_frontmatter(content)?;
        let mut ticket: Ticket = toml::from_str(&frontmatter)
            .with_context(|| format!("Failed to parse ticket frontmatter:\n{}", frontmatter))?;
        ticket.description = body.trim().to_string();
        Ok(ticket)
    }

    /// Serialize ticket to markdown
    pub fn to_markdown(&self) -> Result<String> {
        let fm = toml::to_string(self).context("Failed to serialize ticket to TOML")?;
        Ok(format!("+++\n{}+++\n\n{}\n", fm, self.description))
    }

    /// Load ticket from a file path
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read ticket file: {}", path.display()))?;
        Self::from_markdown(&content)
    }

    /// Save ticket to a file path (atomic write via temp file + rename)
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = self.to_markdown()?;
        atomic_write(path, &content)
    }

    pub fn touch(&mut self) {
        self.updated = chrono::Local::now().date_naive();
    }
}

/// Split markdown into (frontmatter, body) based on +++ delimiters
fn parse_frontmatter(content: &str) -> Result<(String, String)> {
    let content = content.trim_start();
    if !content.starts_with("+++") {
        anyhow::bail!("Missing frontmatter: file must start with +++");
    }
    let after_open = &content[3..];
    let close = after_open
        .find("\n+++")
        .ok_or_else(|| anyhow::anyhow!("Unclosed frontmatter: missing closing +++"))?;
    let frontmatter = after_open[..close].trim().to_string();
    let body = after_open[close + 4..].to_string();
    Ok((frontmatter, body))
}

/// Atomically write content to path using a temp file + rename
pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let tmp_path = parent.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp")
    ));
    std::fs::write(&tmp_path, content)
        .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to rename temp file to: {}", path.display()))?;
    Ok(())
}
