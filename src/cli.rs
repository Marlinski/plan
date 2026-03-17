// CLI command definitions and dispatch
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::hub::{self, HubSummary, SessionKind};
use crate::session;
use crate::state::Store;
use crate::ticket::{Ticket, TicketStatus};

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

    /// Show project status: ticket counts + active sessions
    Status,

    /// Add one or more tickets
    ///
    /// Examples:
    ///   plan add "fix login bug"
    ///   plan add -t auth "fix login" "add tests" "update docs"
    Add {
        /// Tag(s) to apply to the new tickets (repeatable)
        #[arg(short, long = "tag")]
        tags: Vec<String>,
        /// Ticket title(s) — multiple titles create multiple tickets
        titles: Vec<String>,
    },

    /// Pick a ticket — assigns it to the current session and marks it picked
    Pick {
        /// Ticket ID
        id: String,
    },

    /// Unpick a ticket — removes assignment and resets to open (only if you picked it)
    Unpick {
        /// Ticket ID
        id: String,
    },

    /// Mark a ticket as done
    Done {
        /// Ticket ID
        id: String,
    },

    /// Mark a ticket as blocked
    Block {
        /// Ticket ID
        id: String,
    },

    /// Show full details of a ticket
    Show {
        /// Ticket ID
        id: String,
    },

    /// Replace the description of a ticket
    Edit {
        /// Ticket ID
        id: String,
        /// New description content
        content: String,
    },

    /// Delete a ticket
    Delete {
        /// Ticket ID
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// List open tickets (backlog). Use -t TAG to filter by tag.
    Backlog {
        /// Filter by tag
        #[arg(short, long = "tag")]
        tag: Option<String>,
    },

    /// Read or send messages on the shared session hub
    ///
    /// With no argument: show active sessions + unread messages.
    /// With a message: broadcast it to all active sessions.
    Hub {
        /// Message to broadcast (omit to read)
        message: Option<String>,
    },

    /// Print the SKILL.md content for AI agent onboarding
    Skill,
}

// ── Command dispatch ─────────────────────────────────────────────────────────

pub fn run(cli: Cli) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let start = cli.dir.as_deref().unwrap_or(&cwd);

    match &cli.command {
        Commands::Init => return cmd_init(start),
        Commands::Skill => return cmd_skill(),
        Commands::Hub { message } => {
            let store = Store::find(start)?;
            return cmd_hub(&store, message.as_deref());
        }
        _ => {}
    }

    let store = Store::find(start)?;
    let summary = hub_tick(&store);
    print_header(&summary);

    match &cli.command {
        Commands::Init | Commands::Skill | Commands::Hub { .. } => unreachable!(),
        Commands::Status => cmd_status(start, &summary),
        Commands::Add { tags, titles } => cmd_add(&store, tags, titles, &summary),
        Commands::Pick { id } => cmd_pick(&store, id, &summary),
        Commands::Unpick { id } => cmd_unpick(&store, id, &summary),
        Commands::Done { id } => cmd_done(&store, id),
        Commands::Block { id } => cmd_block(&store, id),
        Commands::Show { id } => cmd_show(&store, id),
        Commands::Edit { id, content } => cmd_edit(&store, id, content),
        Commands::Delete { id, yes } => cmd_delete(&store, id, *yes),
        Commands::Backlog { tag } => cmd_backlog(&store, tag.as_deref()),
    }
}

// ── Hub helpers ───────────────────────────────────────────────────────────────

/// Returns (parent_cmd_basename, full_cmdline)
fn parent_cmdline(ppid: u32) -> (String, String) {
    let info = session::process_info(ppid);
    let full = info.as_ref().map(|p| p.args.clone()).unwrap_or_default();
    let base = full
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();
    (base, full)
}

/// Run hub.tick() for the current session. Silently ignores errors (hub is best-effort).
fn hub_tick(store: &Store) -> Option<HubSummary> {
    let ppid = session::session_id();
    let (base, full) = parent_cmdline(ppid);
    let (kind, client) = hub::detect(&full);
    store
        .hub()
        .ok()
        .and_then(|h| h.tick(ppid, kind, base, client).ok())
}

/// Detected client name for the current session (used as creator tag).
fn my_client(ppid: u32) -> String {
    let (_, full) = parent_cmdline(ppid);
    hub::detect(&full).1
}

/// Print the brief one-liner header, plus any unread messages inline.
fn print_header(summary: &Option<HubSummary>) {
    let Some(s) = summary else { return };

    let mut client_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for sess in &s.active_sessions {
        *client_counts.entry(sess.client.clone()).or_insert(0) += 1;
    }
    let mut parts: Vec<String> = client_counts
        .iter()
        .map(|(name, count)| {
            if *count > 1 {
                format!("{} ×{}", name, count)
            } else {
                name.clone()
            }
        })
        .collect();
    parts.sort();

    let who = if parts.is_empty() {
        "1 session".to_string()
    } else {
        parts.join(", ")
    };

    if s.unread.is_empty() {
        println!("[{} active]", who);
    } else {
        println!("[{} active | {} unread]", who, s.unread.len());
        for msg in &s.unread {
            let kind_tag = match msg.from_kind {
                SessionKind::Human => "human",
                SessionKind::Agent => "agent",
            };
            println!("  {} ({}) says: {}", msg.from_client, kind_tag, msg.text);
        }
    }
    println!();
}

// ── Session ID resolution ─────────────────────────────────────────────────────

/// Resolve current session ID: PLAN_AGENT_ID env override > hub SID (auto).
fn my_sid() -> String {
    if let Ok(id) = std::env::var("PLAN_AGENT_ID") {
        if !id.is_empty() {
            return id;
        }
    }
    hub::make_sid(session::session_id())
}

// ── Formatting helpers ───────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
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
    println!("  plan add \"ticket title\"               # create a ticket");
    println!("  plan add -t mytag \"ticket title\"      # create a tagged ticket");
    println!("  plan backlog                          # see open tickets");
    println!("  plan pick <id>                        # pick a ticket");
    Ok(())
}

fn cmd_status(start: &std::path::Path, summary: &Option<HubSummary>) -> Result<()> {
    if let Ok(store) = Store::find(start) {
        let tickets = store.list_tickets()?;
        let open = tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Open)
            .count();
        let picked = tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Picked)
            .count();
        let done = tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Done)
            .count();
        let blocked = tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Blocked)
            .count();

        println!("Tickets:");
        println!("  [ ] open    {}", open);
        println!("  [~] picked  {}", picked);
        println!("  [x] done    {}", done);
        println!("  [!] blocked {}", blocked);
        println!("  total       {}", tickets.len());

        // Currently picked
        let in_flight: Vec<&Ticket> = tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Picked)
            .collect();
        if !in_flight.is_empty() {
            println!();
            println!("In flight:");
            for t in in_flight {
                println!(
                    "  {:>4}  [{}]  {}",
                    t.id,
                    t.assignee.as_deref().unwrap_or("?"),
                    t.title
                );
            }
        }

        // Collect all tags with counts
        let mut tag_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for t in &tickets {
            for tag in &t.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        if !tag_counts.is_empty() {
            let mut tag_list: Vec<(&String, &usize)> = tag_counts.iter().collect();
            tag_list.sort_by_key(|(k, _)| k.as_str());
            println!();
            print!("Tags:");
            for (tag, count) in tag_list {
                print!("  {} ({})", tag, count);
            }
            println!();
        }
    }

    // Active sessions
    if let Some(s) = summary {
        println!();
        println!("Active sessions:");
        if s.active_sessions.is_empty() {
            println!("  (none)");
        } else {
            let my = my_sid();
            for sess in &s.active_sessions {
                let marker = if sess.sid == my { " ← you" } else { "" };
                println!(
                    "  {:<8} {:<28} {}{}",
                    sess.kind, sess.sid, sess.client, marker
                );
            }
        }
    }

    Ok(())
}

fn cmd_add(
    store: &Store,
    extra_tags: &[String],
    titles: &[String],
    summary: &Option<HubSummary>,
) -> Result<()> {
    if titles.is_empty() {
        anyhow::bail!("Provide at least one ticket title");
    }
    let creator_tag = my_client(session::session_id());
    for title in titles {
        let mut tags = vec![creator_tag.clone()];
        for t in extra_tags {
            if t != &creator_tag {
                tags.push(t.clone());
            }
        }
        let ticket = store.create_ticket(title, tags)?;
        let tag_str = if ticket.tags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", ticket.tags.join(", "))
        };
        println!("#{}: {}{}", ticket.id, ticket.title, tag_str);
    }
    // If multiple tickets, show a summary line
    if titles.len() > 1 {
        println!("{} tickets created.", titles.len());
    }

    // Print the header's unread reminder if there are unread messages
    if let Some(s) = summary {
        if !s.unread.is_empty() {
            println!(
                "({} unread hub messages — run `plan hub` to read)",
                s.unread.len()
            );
        }
    }

    Ok(())
}

fn cmd_pick(store: &Store, id: &str, summary: &Option<HubSummary>) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    if ticket.status == TicketStatus::Picked {
        let who = ticket.assignee.as_deref().unwrap_or("someone else");
        if who == my_sid() {
            anyhow::bail!("You already picked ticket #{}", ticket.id);
        } else {
            anyhow::bail!("Ticket #{} is already picked by session {}", ticket.id, who);
        }
    }
    let sid = my_sid();
    ticket.assignee = Some(sid.clone());
    ticket.status = TicketStatus::Picked;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("#{}: picked  {}", ticket.id, ticket.title);

    // Remind about active peers
    if let Some(s) = summary {
        let peers: Vec<&str> = s
            .active_sessions
            .iter()
            .filter(|sess| sess.sid != sid)
            .map(|sess| sess.client.as_str())
            .collect();
        if !peers.is_empty() {
            println!("(other active sessions: {})", peers.join(", "));
        }
    }

    Ok(())
}

fn cmd_unpick(store: &Store, id: &str, summary: &Option<HubSummary>) -> Result<()> {
    let _ = summary;
    let mut ticket = store.load_ticket(id)?;
    let sid = my_sid();
    match ticket.status {
        TicketStatus::Picked => {
            let owner = ticket.assignee.as_deref().unwrap_or("");
            if owner != sid {
                anyhow::bail!(
                    "Ticket #{} was picked by session {}, not you",
                    ticket.id,
                    owner
                );
            }
        }
        other => {
            anyhow::bail!("Ticket #{} is not picked (status: {})", ticket.id, other);
        }
    }
    ticket.assignee = None;
    ticket.status = TicketStatus::Open;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("#{}: open  {}", ticket.id, ticket.title);
    Ok(())
}

fn cmd_done(store: &Store, id: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    ticket.status = TicketStatus::Done;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("#{}: done  {}", ticket.id, ticket.title);
    Ok(())
}

fn cmd_block(store: &Store, id: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    ticket.status = TicketStatus::Blocked;
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("#{}: blocked  {}", ticket.id, ticket.title);
    Ok(())
}

fn cmd_show(store: &Store, id: &str) -> Result<()> {
    let ticket = store.load_ticket(id)?;
    println!("#{}: {}", ticket.id, ticket.title);
    println!("Status:   {}", ticket.status);
    println!(
        "Tags:     {}",
        if ticket.tags.is_empty() {
            "-".to_string()
        } else {
            ticket.tags.join(", ")
        }
    );
    println!("Assignee: {}", ticket.assignee.as_deref().unwrap_or("-"));
    println!("Created:  {}", ticket.created);
    println!("Updated:  {}", ticket.updated);
    if !ticket.description.is_empty() {
        println!();
        println!("{}", ticket.description);
    }
    Ok(())
}

fn cmd_edit(store: &Store, id: &str, content: &str) -> Result<()> {
    let mut ticket = store.load_ticket(id)?;
    ticket.description = content.to_string();
    ticket.touch();
    store.save_ticket(&ticket)?;
    println!("#{}: description updated", ticket.id);
    Ok(())
}

fn cmd_delete(store: &Store, id: &str, yes: bool) -> Result<()> {
    let ticket = store.load_ticket(id)?;
    let canonical_id = ticket.id.clone();
    if !yes {
        print!("Delete ticket #{} '{}'? [y/N] ", canonical_id, ticket.title);
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
    println!("Deleted #{}", canonical_id);
    Ok(())
}

fn cmd_backlog(store: &Store, tag: Option<&str>) -> Result<()> {
    let tickets = store.list_tickets_filtered(Some(&TicketStatus::Open), tag, None)?;
    if tickets.is_empty() {
        if let Some(t) = tag {
            println!("No open tickets tagged '{}'.", t);
        } else {
            println!("No open tickets. Run `plan add \"title\"` to create one.");
        }
        return Ok(());
    }
    println!("{:<6} {:<30} TAGS", "ID", "TITLE");
    println!("{}", "-".repeat(60));
    for t in &tickets {
        println!(
            "#{:<5} {:<30} {}",
            t.id,
            truncate(&t.title, 30),
            t.tags.join(", ")
        );
    }
    Ok(())
}
fn cmd_hub(store: &Store, message: Option<&str>) -> Result<()> {
    let ppid = session::session_id();
    let (base, full) = parent_cmdline(ppid);
    let (kind, client) = hub::detect(&full);
    let h = store.hub()?;

    if let Some(text) = message {
        h.say(ppid, kind.clone(), base.clone(), client.clone(), text)?;
        let _ = h.tick(ppid, kind, base, client);
        println!("Message sent.");
    } else {
        let summary = h.tick(ppid, kind, base, client)?;
        let my = hub::make_sid(ppid);

        println!("Active sessions:");
        if summary.active_sessions.is_empty() {
            println!("  (none)");
        } else {
            println!(
                "  {:<28} {:<8} {:<10} {:<12} CLIENT",
                "SID", "KIND", "LAST SEEN", "COMMAND"
            );
            println!("  {}", "-".repeat(72));
            for s in &summary.active_sessions {
                let marker = if s.sid == my { " ← you" } else { "" };
                println!(
                    "  {:<28} {:<8} {:<10} {:<12} {}{}",
                    s.sid,
                    s.kind,
                    s.last_seen.format("%H:%M:%S"),
                    s.command,
                    s.client,
                    marker
                );
            }
        }

        println!();
        if summary.unread.is_empty() {
            println!("No unread messages.");
        } else {
            println!("Unread messages:");
            for msg in &summary.unread {
                let kind_tag = match msg.from_kind {
                    SessionKind::Human => "human",
                    SessionKind::Agent => "agent",
                };
                println!(
                    "  [{}] {} ({}): {}",
                    msg.ts.format("%H:%M:%S"),
                    msg.from_client,
                    kind_tag,
                    msg.text
                );
            }
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
