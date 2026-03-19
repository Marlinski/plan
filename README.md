# plan

A lightweight CLI task tracker built for AI agents and humans working together.

`plan` stores tickets as Markdown files with TOML frontmatter in a `.todo/` folder at the root of your project. Multiple agents and humans can work on the same project simultaneously — session identity is derived automatically from the process tree, no registration required.

## Why plan?

Your AI agent's built-in todo tool is ephemeral — it disappears on context compaction or session restart. `plan` is persistent, cross-session, and visible to everyone on the project:

- **Persistent** — tickets survive context resets and shell restarts
- **Multi-agent** — multiple AI agents and humans share the same backlog
- **Zero setup** — `.todo/` is created automatically on first use
- **No server** — plain files, works in any git repo

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Marlinski/plan/main/install.sh | sh
```

Installs to `~/.local/bin/plan` (or `/usr/local/bin` if run as root). Supports Linux (x86\_64, aarch64, ARMv7), macOS (x86\_64, Apple Silicon), and Windows.

Or build from source:

```sh
git clone https://github.com/Marlinski/plan.git
cd plan
cargo install --path .
```

## Quick start

```sh
cd /your/project
plan todo add "fix the login bug"
plan todo backlog
plan todo pick 1
plan todo done 1
```

## Commands

```
plan todo                              # show open backlog (default)
plan todo add "title" ["title2" ...]  # create tickets (auto-tagged with your client name)
plan todo add -t TAG "title"          # create tickets with extra tag(s)
plan todo pick <id>                   # claim a ticket
plan todo unpick <id>                 # release a ticket you claimed
plan todo done <id>                   # mark done
plan todo block <id>                  # mark blocked
plan todo show <id>                   # full ticket details
plan todo edit <id> "description"     # update description
plan todo delete <id> [--yes]         # delete a ticket
plan todo backlog [-t TAG]            # list open tickets, optionally filtered by tag

plan status                           # dashboard: ticket counts + active sessions
plan hub                              # active sessions + unread messages
plan hub "message"                    # broadcast a message to all active sessions
plan skill                            # print the AI agent onboarding guide
```

## Ticket statuses

| Symbol | Status  |
|--------|---------|
| `[ ]`  | open    |
| `[~]`  | picked  |
| `[x]`  | done    |
| `[!]`  | blocked |

## Tags

Every ticket is automatically tagged with the creator's client name (`opencode`, `claude-code`, `zsh`, etc.). Add extra tags at creation time:

```sh
plan todo add -t auth -t backend "fix token refresh"
plan todo backlog -t auth
```

## Multi-agent coordination

`plan hub` lets agents and humans see who is active and exchange messages:

```sh
plan hub                   # list active sessions, show unread messages
plan hub "starting auth"   # broadcast a message
```

Session identity is automatic — derived from the PID of the process that launched `plan`. Override if needed:

```sh
PLAN_AGENT_ID=myagent plan todo pick 1
```

## For AI agents — SKILL.md

`plan skill` prints `SKILL.md`, a compact onboarding guide designed to be injected into an agent's context. It covers the full workflow, commands, tags, statuses, and multi-agent etiquette.

If you are an AI agent reading this: run `plan skill` to get the authoritative usage guide.

## Storage format

Tickets are stored as `.todo/<id>.md` files with TOML frontmatter:

```
+++
id = "1"
title = "fix the login bug"
status = "open"
tags = ["opencode"]
created = "2026-03-19T10:00:00Z"
updated = "2026-03-19T10:00:00Z"
+++

Optional description goes here.
```

Plain text, git-friendly, readable without `plan`.

## License

MIT
