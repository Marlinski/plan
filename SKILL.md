# SKILL: plan — CLI Task Tracker for AI Agents

`plan` manages a `.todo/` folder as persistent state for task tracking across agents,
shell restarts, and context compaction. Multiple agents can work in parallel on the
same project; session identity is derived automatically from the process tree.

## plan vs your internal todo tool

As an AI agent you likely have a built-in todo/task tool. Use them for different things:

- **your internal todo** — short-lived subtasks within a single session or task
- **plan** — persistent, cross-agent, cross-session project work: epics, tickets,
  backlog, assignments. Survives compaction. Visible to humans and other agents.

Use `plan` for anything that needs to outlive the current context window.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Marlinski/plan/main/install.sh | sh
```

## Setup (once per project)

```sh
plan init
```

Automatically placed at the git root if inside a git repository.

## Session identity — no registration needed

Your session ID is derived automatically from the PID of the process that invoked
`plan` (the calling agent). It is stable for the lifetime of the agent process.

To see your session ID and verify the process tree:

```sh
plan status
```

You can also override the session ID with the env var `PLAN_AGENT_ID`:

```sh
PLAN_AGENT_ID=myagent plan ticket pick 1
```

## Core workflow

```sh
plan backlog                    # see open unassigned tickets
plan ticket pick <id>           # claim a ticket (auto-assigns to current session)
plan ticket note <id> "text"    # log progress
plan ticket done <id>           # mark complete
```

## Session hub — inter-agent messaging

Every `plan` command prints a brief header showing who is active:

```
[opencode, zsh active | 2 unread]
```

To read unread messages and see all active sessions:

```sh
plan hub
```

To broadcast a message to all active sessions:

```sh
plan hub "blocked on auth-3, taking ticket auth-4 instead"
```

Session kind (`agent` or `human`) and client name (`opencode`, `claude-code`, `zsh`, …)
are detected automatically from the parent process. Override with env vars if needed:

```sh
PLAN_SESSION_TYPE=agent PLAN_CLIENT=mybot plan hub
```

## Commands

```sh
# Init & diagnostics
plan init
plan status                     # show session ID and process tree

# Tickets
plan ticket new --title "..." [--epic <name>] [--priority high|medium|low] [--description "..."]
plan ticket list [--status open|in-progress|done|blocked] [--epic <name>] [--assignee <id>]
plan ticket show <id>
plan ticket pick <id> [--session <hex>]   # assign to current session (or override)
plan ticket assign <id> <session-hex>
plan ticket done <id>
plan ticket status <id> open|in-progress|done|blocked
plan ticket note <id> "text"
plan ticket unassign <id>
plan ticket delete <id> [--yes]

# Epics (ticket grouping)
plan epic new --name <name> --title "..."
plan epic list
plan epic show <name>

# Overview
plan summary
plan backlog
plan skill

# Hub (inter-agent messaging)
plan hub                        # read unread messages + show active sessions
plan hub "text"                 # broadcast message to all active sessions
```

## Ticket IDs

- Without epic: `1`, `2`, `3` (flexible: `1` = `01` = `001`)
- With epic: `auth-1`, `auth-2` (flexible: `auth-1` = `auth-01`)
- Create epic first: `plan epic new --name auth --title "Auth system"`

## Ticket statuses

`[ ]` open · `[~]` in-progress · `[x]` done · `[!]` blocked

## Multi-agent etiquette

- Don't pick tickets already assigned to another session
- Leave notes on tickets you're working on
- Run `plan ticket unassign <id>` if you abandon a ticket
- Check your assignments: `plan ticket list --assignee <your-session-id>`
- Find your session ID: `plan status`
