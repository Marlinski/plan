// State management: .todo/ folder operations, ID generation, atomic writes
use anyhow::{Context, Result};
use rand::Rng;
use std::path::{Path, PathBuf};

use crate::agent::Agent;
use crate::epic::Epic;
use crate::ticket::{atomic_write, Priority, Ticket, TicketStatus};

pub struct Store {
    pub root: PathBuf, // path to .todo/
}

impl Store {
    /// Walk up from `start` to find the git root (directory containing .git/).
    /// Returns None if no git repo is found.
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

    /// Find and open the .todo/ store.
    ///
    /// Search order:
    ///   1. Walk up from `start` looking for an existing .todo/ directory.
    ///   2. If none found, fall back to the git root (or `start` if not in a git repo)
    ///      and report a helpful error pointing to the right init location.
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
        // No .todo found — give a helpful error pointing to the right place
        let init_dir = Self::find_git_root(start).unwrap_or_else(|| start.to_path_buf());
        anyhow::bail!(
            "No .todo directory found. Run `todo init` in {}",
            init_dir.display()
        )
    }

    /// Initialize a new .todo/ store.
    ///
    /// The store is created at the git root when inside a git repository,
    /// or in `dir` otherwise.
    pub fn init(dir: &Path) -> Result<Self> {
        let target = Self::find_git_root(dir).unwrap_or_else(|| dir.to_path_buf());
        let root = target.join(".todo");
        if root.exists() {
            anyhow::bail!(".todo/ already exists in {}", target.display());
        }
        std::fs::create_dir_all(root.join("tickets"))?;
        std::fs::create_dir_all(root.join("epics"))?;
        std::fs::create_dir_all(root.join("agents"))?;
        // Initialize ticket counter
        atomic_write(&root.join("next_id"), "1\n")?;
        Ok(Store { root })
    }

    // ── Ticket ID generation ─────────────────────────────────────────────────

    /// Allocate the next ticket ID (numeric, flexible: "1", "42", "1000")
    pub fn next_ticket_id(&self, epic: Option<&str>) -> Result<String> {
        let counter_path = self.root.join("next_id");
        let raw = std::fs::read_to_string(&counter_path).unwrap_or_else(|_| "1\n".to_string());
        let n: u64 = raw.trim().parse().unwrap_or(1);
        // Write incremented value atomically
        atomic_write(&counter_path, &format!("{}\n", n + 1))?;
        Ok(match epic {
            Some(e) => format!("{}-{}", e, n),
            None => format!("{}", n),
        })
    }

    // ── Agent ID generation ──────────────────────────────────────────────────

    /// Generate a unique 4-hex-digit agent ID, checking for collisions
    pub fn generate_agent_id(&self) -> Result<String> {
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let id = format!("{:04x}", rng.gen::<u16>());
            if !self.agent_path(&id).exists() {
                return Ok(id);
            }
        }
        anyhow::bail!("Could not generate a unique agent ID after 100 attempts");
    }

    // ── Paths ────────────────────────────────────────────────────────────────

    pub fn ticket_path(&self, id: &str) -> PathBuf {
        self.root.join("tickets").join(format!("{}.md", id))
    }

    pub fn agent_path(&self, id: &str) -> PathBuf {
        self.root.join("agents").join(format!("{}.md", id))
    }

    pub fn epic_path(&self, name: &str) -> PathBuf {
        self.root.join("epics").join(format!("{}.md", name))
    }

    // ── Ticket operations ────────────────────────────────────────────────────

    pub fn create_ticket(
        &self,
        title: &str,
        epic: Option<&str>,
        priority: Priority,
        description: Option<&str>,
    ) -> Result<Ticket> {
        // Validate epic exists if provided
        if let Some(e) = epic {
            if !self.epic_path(e).exists() {
                anyhow::bail!(
                    "Epic '{}' does not exist. Create it first with: todo epic new --name {}",
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
        // Flexible ID matching: "1" matches "1", "01", "001", "0001" etc., and "epic-1"
        let path = self.ticket_path(id);
        if path.exists() {
            return Ticket::load(&path);
        }
        // Try to find by numeric suffix match
        let resolved = self.resolve_ticket_id(id)?;
        Ticket::load(&self.ticket_path(&resolved))
    }

    /// Resolve a flexible ticket ID to the canonical stored ID
    pub fn resolve_ticket_id(&self, id: &str) -> Result<String> {
        // Parse numeric value (strip leading zeros)
        let parsed_num = if id.contains('-') {
            // "epic-1", "epic-01" etc: normalize the numeric part
            let parts: Vec<&str> = id.rsplitn(2, '-').collect();
            let num_part = parts[0].trim_start_matches('0');
            let epic_part = parts[1];
            // look for epic-N match
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
                    // match epic-N
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
        // Sort by ID: numeric if possible, else lexicographic
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

    // ── Agent operations ─────────────────────────────────────────────────────

    pub fn register_agent(&self, id: Option<&str>) -> Result<Agent> {
        let id = match id {
            Some(existing_id) => {
                let path = self.agent_path(existing_id);
                if path.exists() {
                    // Reattach: load and mark active
                    let mut agent = Agent::load(&path)?;
                    agent.status = crate::agent::AgentStatus::Active;
                    agent.touch();
                    agent.save(&path)?;
                    return Ok(agent);
                }
                existing_id.to_string()
            }
            None => self.generate_agent_id()?,
        };
        let agent = Agent::new(id.clone());
        agent.save(&self.agent_path(&id))?;
        Ok(agent)
    }

    pub fn load_agent(&self, id: &str) -> Result<Agent> {
        Agent::load(&self.agent_path(id))
    }

    pub fn list_agents(&self) -> Result<Vec<Agent>> {
        let dir = self.root.join("agents");
        let mut agents = Vec::new();
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to read agents directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                match Agent::load(&path) {
                    Ok(a) => agents.push(a),
                    Err(e) => eprintln!("Warning: skipping {:?}: {}", path, e),
                }
            }
        }
        agents.sort_by(|a, b| a.registered.cmp(&b.registered));
        Ok(agents)
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

/// Comparable sort key for ticket IDs: (epic_prefix, numeric_suffix)
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
