// Session hub: tracks active sessions and inter-session messaging.
//
// Layout: .todo/sessions/<sid>.md  (one file per live session)
//
// Session ID = "{ppid_hex}-{ppid_start_secs}"
//
// Every plan invocation:
//   1. Prunes dead sessions (kill -0 check on ppid).
//   2. Upserts own session file (last_seen, cursor).
//   3. Returns HubSummary for header display and message relay.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::ticket::atomic_write;

// ── Session kind ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Human,
    Agent,
}

impl fmt::Display for SessionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionKind::Human => write!(f, "human"),
            SessionKind::Agent => write!(f, "agent"),
        }
    }
}

// ── Message stored inside a session file ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub text: String,
}

// ── Session file (frontmatter + messages) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Stable ID: ppid_hex-start_secs
    pub sid: String,
    pub ppid: u32,
    pub kind: SessionKind,
    /// Raw command of the parent process (e.g. "opencode", "zsh")
    pub command: String,
    /// Detected client name (e.g. "opencode", "claude-code", "zsh", "unknown")
    pub client: String,
    pub last_seen: DateTime<Utc>,
    /// For each peer SID: how many of their messages we have already seen.
    #[serde(default)]
    pub cursors: HashMap<String, u64>,
    /// Messages sent BY this session (appended by `plan hub "..."`)
    #[serde(default)]
    pub messages: Vec<Message>,
}

impl Session {
    pub fn new(sid: String, ppid: u32, kind: SessionKind, command: String, client: String) -> Self {
        Session {
            sid,
            ppid,
            kind,
            command,
            client,
            last_seen: Utc::now(),
            cursors: HashMap::new(),
            messages: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read session file: {}", path.display()))?;
        parse_session(&raw)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = render_session(self)?;
        atomic_write(path, &content)
    }

    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }

    /// Append a message sent by this session.
    pub fn append_message(&mut self, text: &str) {
        let seq = self.messages.last().map(|m| m.seq + 1).unwrap_or(1);
        self.messages.push(Message {
            seq,
            ts: Utc::now(),
            text: text.to_string(),
        });
    }

    /// Advance cursor for a peer and return the unread messages from them.
    pub fn drain_unread<'a>(&mut self, peer: &'a Session) -> Vec<&'a Message> {
        let cursor = self.cursors.entry(peer.sid.clone()).or_insert(0);
        let unread: Vec<&Message> = peer.messages.iter().filter(|m| m.seq > *cursor).collect();
        if let Some(last) = unread.last() {
            *cursor = last.seq;
        }
        unread
    }
}

// ── Serialization: TOML frontmatter + message list ───────────────────────────

#[derive(Serialize, Deserialize)]
struct SessionFrontmatter {
    sid: String,
    ppid: u32,
    kind: SessionKind,
    command: String,
    #[serde(default)]
    client: String,
    last_seen: DateTime<Utc>,
    #[serde(default)]
    cursors: HashMap<String, u64>,
}

fn render_session(s: &Session) -> Result<String> {
    let fm = SessionFrontmatter {
        sid: s.sid.clone(),
        ppid: s.ppid,
        kind: s.kind.clone(),
        command: s.command.clone(),
        client: s.client.clone(),
        last_seen: s.last_seen,
        cursors: s.cursors.clone(),
    };
    let fm_str = toml::to_string(&fm).context("Failed to serialize session frontmatter")?;
    let mut out = format!("+++\n{}+++\n", fm_str);
    for msg in &s.messages {
        out.push_str(&format!(
            "\n[[message]]\nseq = {}\nts = \"{}\"\ntext = {}\n",
            msg.seq,
            msg.ts.to_rfc3339(),
            toml::Value::String(msg.text.clone()),
        ));
    }
    Ok(out)
}

fn parse_session(raw: &str) -> Result<Session> {
    let content = raw.trim_start();
    if !content.starts_with("+++") {
        anyhow::bail!("Session file missing frontmatter +++");
    }
    let after_open = &content[3..];
    let close = after_open
        .find("\n+++")
        .ok_or_else(|| anyhow::anyhow!("Session file: unclosed frontmatter"))?;
    let fm_str = after_open[..close].trim();
    let rest = &after_open[close + 4..];

    let fm: SessionFrontmatter =
        toml::from_str(fm_str).context("Failed to parse session frontmatter")?;

    let messages = parse_messages(rest)?;

    Ok(Session {
        sid: fm.sid,
        ppid: fm.ppid,
        kind: fm.kind,
        command: fm.command,
        client: fm.client,
        last_seen: fm.last_seen,
        cursors: fm.cursors,
        messages,
    })
}

fn parse_messages(rest: &str) -> Result<Vec<Message>> {
    let wrapped = rest.trim().to_string();
    if wrapped.is_empty() {
        return Ok(vec![]);
    }
    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(default)]
        message: Vec<RawMessage>,
    }
    #[derive(Deserialize)]
    struct RawMessage {
        seq: u64,
        ts: String,
        text: String,
    }
    let w: Wrapper = toml::from_str(&wrapped).unwrap_or(Wrapper { message: vec![] });
    w.message
        .into_iter()
        .map(|m| {
            let ts = m.ts.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now());
            Ok(Message {
                seq: m.seq,
                ts,
                text: m.text,
            })
        })
        .collect()
}

// ── Live check ───────────────────────────────────────────────────────────────

/// Returns true if the given PID is still running.
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let ret = libc_kill0(pid);
        ret == 0
    }
    #[cfg(not(unix))]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}

#[cfg(unix)]
fn libc_kill0(pid: u32) -> i32 {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: kill(pid, 0) sends no signal; it only checks process existence.
    unsafe { kill(pid as i32, 0) }
}

// ── Session ID construction ───────────────────────────────────────────────────

/// Get the start time of a PID in seconds (used to disambiguate reused PIDs).
pub fn pid_start_secs(pid: u32) -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meta) = std::fs::metadata(format!("/proc/{}", pid)) {
            if let Ok(modified) = meta.modified() {
                return modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        0
    }
}

/// Build the full session ID for a given ppid.
pub fn make_sid(ppid: u32) -> String {
    let start = pid_start_secs(ppid);
    format!("{:x}-{}", ppid, start)
}

// ── Client & kind detection ───────────────────────────────────────────────────
//
// `detect` takes the full command-line string of the parent process and returns
// (SessionKind, client_name).
//
// Detection order:
//   1. PLAN_SESSION_TYPE env var overrides kind ("human" | "agent")
//   2. PLAN_CLIENT env var overrides client name
//   3. Regex table matched against the full cmdline (binary + args)

struct ClientDef {
    /// Human-readable client name
    name: &'static str,
    kind: SessionKind,
    /// Regex matched against the full cmdline string
    pattern: &'static str,
}

const CLIENTS: &[ClientDef] = &[
    // ── AI agent runtimes ─────────────────────────────────────────────────────
    ClientDef {
        name: "opencode",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bopencode\b",
    },
    ClientDef {
        name: "claude-code",
        kind: SessionKind::Agent,
        // `claude` binary, or node process running @anthropic-ai/claude-code
        pattern: r"(?i)claude(?:-code)?|@anthropic-ai/claude",
    },
    ClientDef {
        name: "aider",
        kind: SessionKind::Agent,
        pattern: r"(?i)\baider\b",
    },
    ClientDef {
        name: "goose",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bgoose\b",
    },
    ClientDef {
        name: "continue",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bcontinue\b",
    },
    ClientDef {
        name: "codeium",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bcodeium\b",
    },
    ClientDef {
        name: "copilot",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bcopilot\b",
    },
    ClientDef {
        name: "cursor",
        kind: SessionKind::Agent,
        // Cursor wraps VS Code; match the cursor binary or argv
        pattern: r"(?i)\bcursor\b",
    },
    ClientDef {
        name: "windsurf",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bwindsurf\b",
    },
    ClientDef {
        name: "amp",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bamp\b",
    },
    ClientDef {
        name: "devin",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bdevin\b",
    },
    ClientDef {
        name: "mentat",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bmentat\b",
    },
    ClientDef {
        name: "sweep",
        kind: SessionKind::Agent,
        pattern: r"(?i)\bsweep\b",
    },
    ClientDef {
        name: "gpt-engineer",
        kind: SessionKind::Agent,
        pattern: r"(?i)gpt.?engineer",
    },
    ClientDef {
        name: "smol-developer",
        kind: SessionKind::Agent,
        pattern: r"(?i)smol.?developer",
    },
    // ── Human shells ─────────────────────────────────────────────────────────
    ClientDef {
        name: "zsh",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?zsh$",
    },
    ClientDef {
        name: "bash",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?bash$",
    },
    ClientDef {
        name: "fish",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?fish$",
    },
    ClientDef {
        name: "sh",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?sh$",
    },
    ClientDef {
        name: "nu",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?nu$",
    },
    ClientDef {
        name: "elvish",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?elvish$",
    },
    ClientDef {
        name: "xonsh",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?xonsh$",
    },
    ClientDef {
        name: "dash",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?dash$",
    },
    ClientDef {
        name: "ksh",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?ksh$",
    },
    ClientDef {
        name: "tcsh",
        kind: SessionKind::Human,
        pattern: r"(?i)^-?tcsh$",
    },
    // ── Human editors / IDEs ─────────────────────────────────────────────────
    ClientDef {
        name: "emacs",
        kind: SessionKind::Human,
        pattern: r"(?i)\bemacs\b",
    },
    ClientDef {
        name: "vim",
        kind: SessionKind::Human,
        pattern: r"(?i)\b(?:n?vim|helix)\b",
    },
    ClientDef {
        name: "vscode",
        kind: SessionKind::Human,
        // VS Code uses "code" binary, or "Code - OSS", or "code-server"
        pattern: r"(?i)\b(?:code(?:-oss|-server)?|vscode)\b",
    },
];

/// Detect (kind, client) from the full cmdline of the parent process.
/// Env var overrides: PLAN_SESSION_TYPE=human|agent, PLAN_CLIENT=<name>
pub fn detect(cmdline: &str) -> (SessionKind, String) {
    // Env overrides
    let kind_override =
        std::env::var("PLAN_SESSION_TYPE")
            .ok()
            .and_then(|v| match v.to_lowercase().as_str() {
                "human" => Some(SessionKind::Human),
                "agent" => Some(SessionKind::Agent),
                _ => None,
            });
    let client_override = std::env::var("PLAN_CLIENT").ok().filter(|v| !v.is_empty());

    // Match against cmdline basename first, then full cmdline
    let basename = cmdline
        .split_whitespace()
        .next()
        .unwrap_or(cmdline)
        .rsplit('/')
        .next()
        .unwrap_or(cmdline);

    for def in CLIENTS {
        let re = match Regex::new(def.pattern) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if re.is_match(basename) || re.is_match(cmdline) {
            let kind = kind_override.clone().unwrap_or_else(|| def.kind.clone());
            let client = client_override
                .clone()
                .unwrap_or_else(|| def.name.to_string());
            return (kind, client);
        }
    }

    // No match — fall back
    let kind = kind_override.unwrap_or(SessionKind::Agent);
    let client = client_override.unwrap_or_else(|| {
        // Use the basename of the command as the client name
        basename
            .trim_start_matches('-')
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string()
    });
    (kind, client)
}

// ── Hub: the sessions directory ──────────────────────────────────────────────

pub struct Hub {
    pub dir: PathBuf,
}

pub struct HubSummary {
    #[allow(dead_code)]
    pub my_sid: String,
    pub active_sessions: Vec<Session>,
    pub unread: Vec<UnreadMessage>,
}

pub struct UnreadMessage {
    #[allow(dead_code)]
    pub from_sid: String,
    pub from_client: String,
    pub from_kind: SessionKind,
    pub text: String,
    pub ts: DateTime<Utc>,
}

impl Hub {
    pub fn open(sessions_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&sessions_dir)?;
        Ok(Hub { dir: sessions_dir })
    }

    fn session_path(&self, sid: &str) -> PathBuf {
        self.dir.join(format!("{}.md", sid))
    }

    fn load_live_sessions(&self) -> Result<Vec<Session>> {
        let mut live = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return Ok(vec![]),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match Session::load(&path) {
                Ok(s) => {
                    if pid_alive(s.ppid) {
                        live.push(s);
                    } else {
                        let _ = std::fs::remove_file(&path);
                    }
                }
                Err(_) => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(live)
    }

    /// Called on every plan invocation. Returns HubSummary with active sessions
    /// and unread messages, after updating our own session file.
    pub fn tick(
        &self,
        ppid: u32,
        kind: SessionKind,
        command: String,
        client: String,
    ) -> Result<HubSummary> {
        let my_sid = make_sid(ppid);
        let my_path = self.session_path(&my_sid);

        let mut me = if my_path.exists() {
            Session::load(&my_path).unwrap_or_else(|_| {
                Session::new(
                    my_sid.clone(),
                    ppid,
                    kind.clone(),
                    command.clone(),
                    client.clone(),
                )
            })
        } else {
            Session::new(my_sid.clone(), ppid, kind, command, client)
        };
        me.touch();

        let peers = self.load_live_sessions()?;

        let mut unread = Vec::new();
        for peer in &peers {
            if peer.sid == my_sid {
                continue;
            }
            let messages = me.drain_unread(peer);
            for msg in messages {
                unread.push(UnreadMessage {
                    from_sid: peer.sid.clone(),
                    from_client: peer.client.clone(),
                    from_kind: peer.kind.clone(),
                    text: msg.text.clone(),
                    ts: msg.ts,
                });
            }
        }

        me.save(&my_path)?;

        let mut active: Vec<Session> = peers.into_iter().filter(|s| s.sid != my_sid).collect();
        active.push(me);
        active.sort_by(|a, b| a.sid.cmp(&b.sid));

        Ok(HubSummary {
            my_sid,
            active_sessions: active,
            unread,
        })
    }

    /// Append a message from the current session.
    pub fn say(
        &self,
        ppid: u32,
        kind: SessionKind,
        command: String,
        client: String,
        text: &str,
    ) -> Result<()> {
        let my_sid = make_sid(ppid);
        let my_path = self.session_path(&my_sid);
        let mut me = if my_path.exists() {
            Session::load(&my_path)?
        } else {
            Session::new(my_sid.clone(), ppid, kind, command, client)
        };
        me.append_message(text);
        me.touch();
        me.save(&my_path)
    }

    #[allow(dead_code)]
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        self.load_live_sessions()
    }
}
