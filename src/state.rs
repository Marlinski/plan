// State management: .todo/ folder operations, ID generation, atomic writes
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::hub::Hub;
use crate::ticket::{atomic_write, Ticket, TicketStatus};

pub struct Store {
    pub root: PathBuf, // path to .todo/
}

impl Store {
    /// Walk up from `start` to find the git root (directory containing .git/).
    fn find_git_root(start: &Path) -> Option<PathBuf> {
        let mut dir = start.to_path_buf();
        loop {
            if dir.join(".git").exists() {
                return Some(dir);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Find and open the .todo/ store, walking up from `start`.
    pub fn find(start: &Path) -> Result<Self> {
        let mut dir = start.to_path_buf();
        loop {
            let candidate = dir.join(".todo");
            if candidate.is_dir() {
                return Ok(Store { root: candidate });
            }
            if !dir.pop() {
                break;
            }
        }
        let init_dir = Self::find_git_root(start).unwrap_or_else(|| start.to_path_buf());
        anyhow::bail!(
            "No .todo directory found. Run `plan init` in {}",
            init_dir.display()
        )
    }

    /// Initialize a new .todo/ store at the git root, or `dir` if not in a repo.
    pub fn init(dir: &Path) -> Result<Self> {
        let target = Self::find_git_root(dir).unwrap_or_else(|| dir.to_path_buf());
        let root = target.join(".todo");
        if root.exists() {
            anyhow::bail!(".todo/ already exists in {}", target.display());
        }
        std::fs::create_dir_all(root.join("tickets"))?;
        std::fs::create_dir_all(root.join("sessions"))?;
        atomic_write(&root.join("next_id"), "1\n")?;
        Ok(Store { root })
    }

    // ── Paths ────────────────────────────────────────────────────────────────

    pub fn ticket_path(&self, id: &str) -> PathBuf {
        self.root.join("tickets").join(format!("{}.md", id))
    }

    /// Open (or create) the sessions hub for this store.
    pub fn hub(&self) -> Result<Hub> {
        Hub::open(self.root.join("sessions"))
    }

    // ── Ticket ID generation ─────────────────────────────────────────────────

    pub fn next_ticket_id(&self) -> Result<String> {
        let counter_path = self.root.join("next_id");
        let raw = std::fs::read_to_string(&counter_path).unwrap_or_else(|_| "1\n".to_string());
        let n: u64 = raw.trim().parse().unwrap_or(1);
        atomic_write(&counter_path, &format!("{}\n", n + 1))?;
        Ok(format!("{}", n))
    }

    // ── Ticket operations ────────────────────────────────────────────────────

    /// Create one ticket. `tags` should already include the creator tag.
    pub fn create_ticket(&self, title: &str, tags: Vec<String>) -> Result<Ticket> {
        let id = self.next_ticket_id()?;
        let ticket = Ticket::new(id.clone(), title.to_string(), tags);
        let path = self.ticket_path(&id);
        ticket.save(&path)?;
        Ok(ticket)
    }

    pub fn load_ticket(&self, id: &str) -> Result<Ticket> {
        let path = self.ticket_path(id);
        if path.exists() {
            return Ticket::load(&path);
        }
        // Try resolving stripped-zero IDs
        let resolved = self.resolve_ticket_id(id)?;
        Ticket::load(&self.ticket_path(&resolved))
    }

    /// Resolve a flexible ticket ID (e.g. "01" → "1") to the canonical stored ID.
    pub fn resolve_ticket_id(&self, id: &str) -> Result<String> {
        let trimmed = id.trim_start_matches('0');
        let num: u64 = trimmed
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid ticket ID: '{}'", id))?;
        let tickets = self.list_tickets()?;
        for ticket in &tickets {
            let tid: u64 = ticket.id.parse().unwrap_or(u64::MAX);
            if tid == num {
                return Ok(ticket.id.clone());
            }
        }
        anyhow::bail!("Ticket '{}' not found", id)
    }

    pub fn save_ticket(&self, ticket: &Ticket) -> Result<()> {
        let path = self.ticket_path(&ticket.id);
        ticket.save(&path)
    }

    pub fn list_tickets(&self) -> Result<Vec<Ticket>> {
        let dir = self.root.join("tickets");
        let mut tickets = Vec::new();
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to read tickets directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                match Ticket::load(&path) {
                    Ok(t) => tickets.push(t),
                    Err(e) => eprintln!("Warning: skipping {:?}: {}", path, e),
                }
            }
        }
        tickets.sort_by_key(|t| t.id.parse::<u64>().unwrap_or(0));
        Ok(tickets)
    }

    pub fn list_tickets_filtered(
        &self,
        status: Option<&TicketStatus>,
        tag: Option<&str>,
        assignee: Option<&str>,
    ) -> Result<Vec<Ticket>> {
        let all = self.list_tickets()?;
        Ok(all
            .into_iter()
            .filter(|t| status.is_none_or(|s| &t.status == s))
            .filter(|t| tag.is_none_or(|tg| t.tags.iter().any(|tt| tt == tg)))
            .filter(|t| assignee.is_none_or(|a| t.assignee.as_deref() == Some(a)))
            .collect())
    }
}
