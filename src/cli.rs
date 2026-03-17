// CLI command definitions and dispatch
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::session;
use crate::state::Store;
use crate::ticket::{Priority, TicketStatus};

#[derive(Parser)]
#[command(
    name = "plan",
    about = "CLI task tracker for AI agents and humans",
    version,
    propagate_version = true
)]
pub struct Cli {
    /// Path to project directory (default: current directory, walks up to find .todo/)
    #[arg(long, global = true)]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new .todo/ store (placed at git root if inside a repo)
    Init,

    /// Show current session identity: process tree from plan up to the agent
    Status,

    /// Manage tickets
    #[command(subcommand)]
    Ticket(TicketCommands),

    /// Manage epics
    #[command(subcommand)]
    Epic(EpicCommands),

    /// List all open, unassigned tickets
    Backlog,

    /// Show overall project summary
    Summary,

    /// Print the SKILL.md content for AI agent onboarding
    Skill,
}

// ── Ticket subcommands ───────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum TicketCommands {
    /// Create a new ticket
    New {
        /// Ticket title
        #[arg(short, long)]
        title: String,
        /// Epic name to group under (ticket ID will be epic-N)
        #[arg(short, long)]
        epic: Option<String>,
        /// Priority: low, medium, high
        #[arg(short, long, default_value = "medium")]
        priority: String,
        /// Initial description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// List tickets (default: all)
    List {
        /// Filter by status: open, in-progress, done, blocked
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by epic name
        #[arg(short, long)]
        epic: Option<String>,
        /// Filter by assignee session ID
        #[arg(short, long)]
        assignee: Option<String>,
    },
    /// Show full details of a ticket
    Show {
        /// Ticket ID (flexible: 1 = 01 = 001, auth-1 = auth-01)
        id: String,
    },
    /// Assign a ticket to a specific session ID
    Assign {
        /// Ticket ID
        id: String,
        /// Session ID (hex) to assign to
        session: String,
    },
    /// Pick a ticket and assign it to the current session (implicit from process tree)
    Pick {
        /// Ticket ID
        id: String,
        /// Override session ID (default: auto-detected from parent PID)
        #[arg(long)]
        session: Option<String>,
    },
    /// Mark a ticket as done
    Done {
        /// Ticket ID
        id: String,
    },
    /// Set the status of a ticket
    Status {
        /// Ticket ID
        id: String,
        /// New status: open, in-progress, done, blocked
        status: String,
    },
    /// Append a note to a ticket
    Note {
        /// Ticket ID
        id: String,
        /// Note text to append
        note: String,
    },
    /// Unassign a ticket (clear assignee, reset to open)
    Unassign {
        /// Ticket ID
        id: String,
    },
    /// Delete a ticket
    Delete {
        /// Ticket ID
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

// ── Epic subcommands ─────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum EpicCommands {
    /// Create a new epic
    New {
        /// Short identifier used in ticket IDs (e.g. 'backend')
        #[arg(long)]
        name: String,
        /// Human-readable title
        #[arg(long)]
        title: String,
    },
    /// List all epics with ticket counts
    List,
    /// Show all tickets in an epic
    Show {
        /// Epic name
        name: String,
    },
}

// ── Command dispatch ─────────────────────────────────────────────────────────

pub fn run(cli: Cli) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let start = cli.dir.as_deref().unwrap_or(&cwd);

    match &cli.command {
        Commands::Init => cmd_init(start),
        Commands::Status => cmd_status(),
        Commands::Ticket(sub) => {
            let store = Store::find(start)?;
            match sub {
                TicketCommands::New {
                    title,
                    epic,
                    priority,
                    description,
                } => {
                    let p: Priority = priority.parse()?;
                    cmd_ticket_new(&store, title, epic.as_deref(), p, description.as_deref())
                }
                TicketCommands::List {
                    status,
                    epic,
                    assignee,
                } => {
                    let s = status
                        .as_deref()
                        .map(|s| s.parse::<TicketStatus>())
                        .transpose()?;
                    cmd_ticket_list(&store, s.as_ref(), epic.as_deref(), assignee.as_deref())
                }
                TicketCommands::Show { id } => cmd_ticket_show(&store, id),
                TicketCommands::Assign { id, session } => cmd_ticket_assign(&store, id, session),
                TicketCommands::Pick { id, session } => {
                    let sid = resolve_session_id(session.as_deref());
                    cmd_ticket_assign(&store, id, &sid)
                }
                TicketCommands::Done { id } => {
                    cmd_ticket_set_status(&store, id, TicketStatus::Done)
                }
                TicketCommands::Status { id, status } => {
                    let s: TicketStatus = status.parse()?;
                    cmd_ticket_set_status(&store, id, s)
                }
                TicketCommands::Note { id, note } => cmd_ticket_note(&store, id, note),
                TicketCommands::Unassign { id } => cmd_ticket_unassign(&store, id),
                TicketCommands::Delete { id, yes } => cmd_ticket_delete(&store, id, *yes),
            }
        }
        Commands::Epic(sub) => {
            let store = Store::find(start)?;
            match sub {
                EpicCommands::New { name, title } => cmd_epic_new(&store, name, title),
                EpicCommands::List => cmd_epic_list(&store),
                EpicCommands::Show { name } => cmd_epic_show(&store, name),
            }
        }
        Commands::Backlog => {
            let store = Store::find(start)?;
            cmd_ticket_list(&store, Some(&TicketStatus::Open), None, None)
        }
        Commands::Summary => {
            let store = Store::find(start)?;
            cmd_summary(&store)
        }
        Commands::Skill => cmd_skill(),
    }
}

// ── Session ID resolution ────────────────────────────────────────────────────

/// Resolve the session ID: explicit override > PLAN_AGENT_ID env > parent PID (auto).
fn resolve_session_id(override_id: Option<&str>) -> String {
    if let Some(id) = override_id {
        return id.to_string();
    }
    if let Ok(id) = std::env::var("PLAN_AGENT_ID") {
        if !id.is_empty() {
            return id;
        }
    }
    // Auto-detect: use parent PID as session identity
    session::session_id_hex(session::session_id())
}

// ── Formatting helpers ───────────────────────────────────────────────────────

fn status_icon(s: &TicketStatus) -> &'static str {
    match s {
        TicketStatus::Open => "[ ]",
        TicketStatus::InProgress => "[~]",
        TicketStatus::Done => "[x]",
        TicketStatus::Blocked => "[!]",
    }
}

fn priority_icon(p: &Priority) -> &'static str {
    match p {
        Priority::Low => "↓",
        Priority::Medium => "→",
        Priority::High => "↑",
    }
}

// ── Command implementations ──────────────────────────────────────────────────

fn cmd_init(dir: &std::path::Path) -> Result<()> {
    let store = Store::init(dir)?;
    let display = store
        .root
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| dir.to_path_buf());
    println!("Initialized .todo/ in {}", display.display());
    println!("Next steps:");
    println!("  plan epic new --name <n> --title <t>  # create an epic");
    println!("  plan ticket new --title <t>           # create a ticket");
    println!("  plan status                           # show your session identity");
    Ok(())
}

fn cmd_status() -> Result<()> {
    let my_pid = std::process::id();
    let session_ppid = session::session_id();
    let session_hex = session::session_id_hex(session_ppid);

    println!("=== plan session status ===");
    println!();
    println!("Session ID (use this as assignee): {}", session_hex);
    println!();
    println!("Process tree (plan → parent → grandparent ...):");
    println!();

    // Start from plan's own PID and walk up
    let chain = session::process_chain(my_pid, 8);
    for (i, info) in chain.iter().enumerate() {
        let marker = if i == 0 {
            "plan"
        } else if i == 1 {
            "caller (session ID source)"
        } else {
            "ancestor"
        };
        println!(
            "  {:>6}  {:>6}  [{:<26}]  {}",
            info.pid,
            info.ppid,
            marker,
            truncate(&info.args, 60)
        );
    }

    println!();
    println!("pid     ppid    [role]");
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn cmd_ticket_new(
    store: &Store,
    title: &str,
    epic: Option<&str>,
    priority: Priority,
    description: Option<&str>,
) -> Result<()> {
    let ticket = store.create_ticket(title, epic, priority, description)?;
    println!("Created ticket: {}", ticket.id);
    println!("Title:    {}", ticket.title);
    println!("Priority: {}", ticket.priority);
    if let Some(e) = &ticket.epic {
        println!("Epic:     {}", e);
    }
    Ok(())
}

fn cmd_ticket_list(
    store: &Store,
    status: Option<&TicketStatus>,
    epic: Option<&str>,
    assignee: Option<&str>,
) -> Result<()> {
    let tickets = store.list_tickets_filtered(status, epic, assignee)?;
    if tickets.is_empty() {
        println!("No tickets found.");
        return Ok(());
    }
    println!(
        "{:<14} {:<4} {:<4} {:<12} {:<10} TITLE",
        "ID", "ST", "PR", "ASSIGNEE", "UPDATED"
    );
    println!("{}", "-".repeat(72));
    for t in tickets {
        println!(
            "{:<14} {:<4} {:<4} {:<12} {:<10} {}",
            t.id,
            status_icon(&t.status),
            priority_icon(&t.priority),
            t.assignee.as_deref().unwrap_or("-"),
            t.updated.format("%Y-%m-%d"),
            t.title
        );
    }
    println!();
    println!("Legend: [ ] open  [~] in-progress  [x] done  [!] blocked  ↑ high  → medium  ↓ low");
    Ok(())
}

fn cmd_ticket_show(store: &Store, id: &str) -> Result<()> {
    let ticket = store.load_ticket(id)?;
    println!("ID:          {}", ticket.id);
    println!("Title:       {}", ticket.title);
    println!("Status:      {}", ticket.status);
    println!("Priority:    {}", ticket.priority);
    println!("Epic:        {}", ticket.epic.as_deref().unwrap_or("-"));
    println!("Assignee:    {}", ticket.assignee.as_deref().unwrap_or("-"));
    println!("Created:     {}", ticket.created);
    println!("Updated:     {}", ticket.updated);
    if !ticket.description.is_empty() {
        println!("\nDescription:\n{}", ticket.description);
    }
    Ok(())
}

fn cmd_ticket_assign(store: &Store, id: &str, session_id: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    ticket.assignee = Some(session_id.to_string());
    ticket.status = TicketStatus::InProgress;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("Ticket {} assigned to session {}", canonical_id, session_id);
    println!("Status set to: in-progress");
    Ok(())
}

fn cmd_ticket_set_status(store: &Store, id: &str, status: TicketStatus) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    ticket.status = status.clone();
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("Ticket {} status: {}", canonical_id, status);
    Ok(())
}

fn cmd_ticket_note(store: &Store, id: &str, note: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
    if ticket.description.is_empty() {
        ticket.description = format!("## Notes\n\n- [{}] {}", timestamp, note);
    } else {
        ticket
            .description
            .push_str(&format!("\n- [{}] {}", timestamp, note));
    }
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("Note added to ticket {}", canonical_id);
    Ok(())
}

fn cmd_ticket_unassign(store: &Store, id: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    ticket.assignee = None;
    ticket.status = TicketStatus::Open;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("Ticket {} unassigned, status reset to open", canonical_id);
    Ok(())
}

fn cmd_ticket_delete(store: &Store, id: &str, yes: bool) -> Result<()> {
    let ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    if !yes {
        print!(
            "Delete ticket '{}' ({})? [y/N] ",
            canonical_id, ticket.title
        );
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }
    let path = store.ticket_path(&canonical_id);
    std::fs::remove_file(&path)
        .with_context(|| format!("Failed to delete ticket file: {}", path.display()))?;
    println!("Deleted ticket {}", canonical_id);
    Ok(())
}

fn cmd_epic_new(store: &Store, name: &str, title: &str) -> Result<()> {
    let epic = store.create_epic(name, title)?;
    println!("Created epic: {} — {}", epic.name, epic.title);
    println!(
        "Tickets in this epic will have IDs like: {}-1, {}-2, ...",
        epic.name, epic.name
    );
    Ok(())
}

fn cmd_epic_list(store: &Store) -> Result<()> {
    let epics = store.list_epics()?;
    if epics.is_empty() {
        println!("No epics found. Create one with: plan epic new --name <n> --title <t>");
        return Ok(());
    }
    let all_tickets = store.list_tickets()?;
    println!(
        "{:<16} {:<8} {:<8} {:<8} TITLE",
        "EPIC", "OPEN", "WIP", "DONE"
    );
    println!("{}", "-".repeat(60));
    for epic in epics {
        let epic_tickets: Vec<_> = all_tickets
            .iter()
            .filter(|t| t.epic.as_deref() == Some(&epic.name))
            .collect();
        let open = epic_tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Open)
            .count();
        let wip = epic_tickets
            .iter()
            .filter(|t| t.status == TicketStatus::InProgress)
            .count();
        let done = epic_tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Done)
            .count();
        println!(
            "{:<16} {:<8} {:<8} {:<8} {}",
            epic.name, open, wip, done, epic.title
        );
    }
    Ok(())
}

fn cmd_epic_show(store: &Store, name: &str) -> Result<()> {
    let path = store.epic_path(name);
    if !path.exists() {
        anyhow::bail!("Epic '{}' not found", name);
    }
    let epic = crate::epic::Epic::load(&path)?;
    println!("Epic: {} — {}", epic.name, epic.title);
    println!("Created: {}", epic.created);
    if !epic.description.is_empty() {
        println!("\n{}", epic.description);
    }
    println!();
    cmd_ticket_list(store, None, Some(name), None)
}

fn cmd_summary(store: &Store) -> Result<()> {
    let tickets = store.list_tickets()?;

    let open = tickets
        .iter()
        .filter(|t| t.status == TicketStatus::Open)
        .count();
    let wip = tickets
        .iter()
        .filter(|t| t.status == TicketStatus::InProgress)
        .count();
    let done = tickets
        .iter()
        .filter(|t| t.status == TicketStatus::Done)
        .count();
    let blocked = tickets
        .iter()
        .filter(|t| t.status == TicketStatus::Blocked)
        .count();

    println!("=== Project Summary ===");
    println!();
    println!("Tickets:");
    println!("  [ ] Open:        {}", open);
    println!("  [~] In progress: {}", wip);
    println!("  [x] Done:        {}", done);
    println!("  [!] Blocked:     {}", blocked);
    println!("  Total:           {}", tickets.len());

    let epics = store.list_epics()?;
    if !epics.is_empty() {
        println!();
        println!("Epics:");
        for epic in &epics {
            let count = tickets
                .iter()
                .filter(|t| t.epic.as_deref() == Some(&epic.name))
                .count();
            println!("  {} — {} ({} tickets)", epic.name, epic.title, count);
        }
    }

    if wip > 0 {
        println!();
        println!("In progress:");
        for t in tickets
            .iter()
            .filter(|t| t.status == TicketStatus::InProgress)
        {
            println!(
                "  {} [{}] {}",
                t.id,
                t.assignee.as_deref().unwrap_or("unassigned"),
                t.title
            );
        }
    }

    Ok(())
}

fn cmd_skill() -> Result<()> {
    let candidates = vec![
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("SKILL.md"))),
        Some(PathBuf::from("SKILL.md")),
        dirs_skill_path(),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate)?;
            println!("{}", content);
            return Ok(());
        }
    }
    println!("{}", EMBEDDED_SKILL);
    Ok(())
}

fn dirs_skill_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        PathBuf::from(h)
            .join(".local")
            .join("share")
            .join("plan")
            .join("SKILL.md")
    })
}

const EMBEDDED_SKILL: &str = include_str!("../SKILL.md");
