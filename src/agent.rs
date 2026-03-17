// Agent registration and session management
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

use crate::ticket::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Active,
    Crashed,
    Retired,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Active => write!(f, "active"),
            AgentStatus::Crashed => write!(f, "crashed"),
            AgentStatus::Retired => write!(f, "retired"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub registered: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub status: AgentStatus,
    pub notes: String,
}

impl Agent {
    pub fn new(id: String) -> Self {
        let now = Utc::now();
        Agent {
            id,
            registered: now,
            last_seen: now,
            status: AgentStatus::Active,
            notes: String::new(),
        }
    }

    pub fn from_markdown(content: &str) -> Result<Self> {
        let (frontmatter, body) = parse_agent_frontmatter(content)?;
        let mut agent: Agent = toml::from_str(&frontmatter)
            .with_context(|| format!("Failed to parse agent frontmatter:\n{}", frontmatter))?;
        agent.notes = body.trim().to_string();
        Ok(agent)
    }

    pub fn to_markdown(&self) -> Result<String> {
        let fm = toml::to_string(self).context("Failed to serialize agent to TOML")?;
        Ok(format!("+++\n{}+++\n\n{}\n", fm, self.notes))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read agent file: {}", path.display()))?;
        Self::from_markdown(&content)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = self.to_markdown()?;
        atomic_write(path, &content)
    }

    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }
}

fn parse_agent_frontmatter(content: &str) -> Result<(String, String)> {
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
