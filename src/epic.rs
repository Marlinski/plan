// Epic data structure
use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::ticket::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epic {
    pub name: String,
    pub title: String,
    pub created: NaiveDate,
    pub description: String,
}

impl Epic {
    pub fn new(name: String, title: String) -> Self {
        Epic {
            name,
            title,
            created: chrono::Local::now().date_naive(),
            description: String::new(),
        }
    }

    pub fn from_markdown(content: &str) -> Result<Self> {
        let content = content.trim_start();
        if !content.starts_with("+++") {
            anyhow::bail!("Missing frontmatter");
        }
        let after_open = &content[3..];
        let close = after_open
            .find("\n+++")
            .ok_or_else(|| anyhow::anyhow!("Unclosed frontmatter"))?;
        let frontmatter = after_open[..close].trim().to_string();
        let body = after_open[close + 4..].to_string();
        let mut epic: Epic = toml::from_str(&frontmatter)
            .with_context(|| format!("Failed to parse epic frontmatter:\n{}", frontmatter))?;
        epic.description = body.trim().to_string();
        Ok(epic)
    }

    pub fn to_markdown(&self) -> Result<String> {
        let fm = toml::to_string(self).context("Failed to serialize epic to TOML")?;
        Ok(format!("+++\n{}+++\n\n{}\n", fm, self.description))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read epic file: {}", path.display()))?;
        Self::from_markdown(&content)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = self.to_markdown()?;
        atomic_write(path, &content)
    }
}
