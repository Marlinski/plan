// State management: .todo/ folder operations, ID generation, atomic writes
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::epic::Epic;
use crate::ticket::{atomic_write, Priority, Ticket, TicketStatus};

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
        std::fs::create_dir_all(root.join("epics"))?;
        atomic_write(&root.join("next_id"), "1\n")?;
        Ok(Store { root })
    }

    // ── Paths ────────────────────────────────────────────────────────────────

    pub fn ticket_path(&self, id: &str) -> PathBuf {
        self.root.join("tickets").join(format!("{}.md", id))
    }

    pub fn epic_path(&self, name: &str) -> PathBuf {
        self.root.join("epics").join(format!("{}.md", name))
    }

    // ── Ticket ID generation ─────────────────────────────────────────────────

    pub fn next_ticket_id(&self, epic: Option<&str>) -> Result<String> {
        let counter_path = self.root.join("next_id");
        let raw = std::fs::read_to_string(&counter_path).unwrap_or_else(|_| "1\n".to_string());
        let n: u64 = raw.trim().parse().unwrap_or(1);
        atomic_write(&counter_path, &format!("{}\n", n + 1))?;
        Ok(match epic {
            Some(e) => format!("{}-{}", e, n),
            None => format!("{}", n),
        })
    }

    // ── Ticket operations ────────────────────────────────────────────────────

    pub fn create_ticket(
        &self,
        title: &str,
        epic: Option<&str>,
        priority: Priority,
        description: Option<&str>,
    ) -> Result<Ticket> {
        if let Some(e) = epic {
            if !self.epic_path(e).exists() {
                anyhow::bail!(
                    "Epic '{}' does not exist. Create it first with: plan epic new --name {}",
                    e,
                    e
                );
            }
        }
        let id = self.next_ticket_id(epic)?;
        let mut ticket = Ticket::new(
            id.clone(),
            title.to_string(),
            epic.map(|s| s.to_string()),
            priority,
        );
        if let Some(desc) = description {
            ticket.description = desc.to_string();
        }
        let path = self.ticket_path(&id);
        ticket.save(&path)?;
        Ok(ticket)
    }

    pub fn load_ticket(&self, id: &str) -> Result<Ticket> {
        let path = self.ticket_path(id);
        if path.exists() {
            return Ticket::load(&path);
        }
        let resolved = self.resolve_ticket_id(id)?;
        Ticket::load(&self.ticket_path(&resolved))
    }

    /// Resolve a flexible ticket ID (leading zeros, etc.) to the canonical stored ID.
    pub fn resolve_ticket_id(&self, id: &str) -> Result<String> {
        let parsed_num = if id.contains('-') {
            let parts: Vec<&str> = id.rsplitn(2, '-').collect();
            let num_part = parts[0].trim_start_matches('0');
            let epic_part = parts[1];
            let num = if num_part.is_empty() { "0" } else { num_part };
            Some((Some(epic_part.to_string()), num.parse::<u64>().ok()))
        } else {
            let trimmed = id.trim_start_matches('0');
            let num = if trimmed.is_empty() { "0" } else { trimmed };
            Some((None, num.parse::<u64>().ok()))
        };

        let tickets = self.list_tickets()?;
        for ticket in &tickets {
            match &parsed_num {
                Some((Some(epic), Some(n))) => {
                    if let Some(ref te) = ticket.epic {
                        if te == epic {
                            let suffix = ticket
                                .id
                                .rsplitn(2, '-')
                                .next()
                                .and_then(|s| s.trim_start_matches('0').parse::<u64>().ok())
                                .unwrap_or(0);
                            if suffix == *n {
                                return Ok(ticket.id.clone());
                            }
                        }
                    }
                }
                Some((None, Some(n))) => {
                    if !ticket.id.contains('-') {
                        let tid = ticket
                            .id
                            .trim_start_matches('0')
                            .parse::<u64>()
                            .unwrap_or(u64::MAX);
                        if tid == *n {
                            return Ok(ticket.id.clone());
                        }
                    }
                }
                _ => {}
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
        tickets.sort_by(|a, b| ticket_id_order(&a.id).cmp(&ticket_id_order(&b.id)));
        Ok(tickets)
    }

    pub fn list_tickets_filtered(
        &self,
        status: Option<&TicketStatus>,
        epic: Option<&str>,
        assignee: Option<&str>,
    ) -> Result<Vec<Ticket>> {
        let all = self.list_tickets()?;
        Ok(all
            .into_iter()
            .filter(|t| status.map_or(true, |s| &t.status == s))
            .filter(|t| epic.map_or(true, |e| t.epic.as_deref() == Some(e)))
            .filter(|t| assignee.map_or(true, |a| t.assignee.as_deref() == Some(a)))
            .collect())
    }

    // ── Epic operations ──────────────────────────────────────────────────────

    pub fn create_epic(&self, name: &str, title: &str) -> Result<Epic> {
        let path = self.epic_path(name);
        if path.exists() {
            anyhow::bail!("Epic '{}' already exists", name);
        }
        let epic = Epic::new(name.to_string(), title.to_string());
        epic.save(&path)?;
        Ok(epic)
    }

    pub fn list_epics(&self) -> Result<Vec<Epic>> {
        let dir = self.root.join("epics");
        let mut epics = Vec::new();
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to read epics directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                match Epic::load(&path) {
                    Ok(e) => epics.push(e),
                    Err(e) => eprintln!("Warning: skipping {:?}: {}", path, e),
                }
            }
        }
        epics.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(epics)
    }
}

fn ticket_id_order(id: &str) -> (String, u64) {
    if let Some(pos) = id.rfind('-') {
        let epic = id[..pos].to_string();
        let num = id[pos + 1..].parse::<u64>().unwrap_or(0);
        (epic, num)
    } else {
        let num = id.parse::<u64>().unwrap_or(0);
        (String::new(), num)
    }
}
