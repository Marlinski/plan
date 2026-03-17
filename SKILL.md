# SKILL: plan — CLI Task Tracker for AI Agents

`plan` manages a `.todo/` folder as persistent state for task tracking across agents,
shell restarts, and context compaction. Multiple agents can work in parallel on the
same project, each with a unique 4-hex ID.

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

## Register — do this first, remember your ID

```sh
plan register
# prints: Agent registered: a3f2
```

**Store your ID immediately.** It does not persist across shell invocations.

```sh
AGENT_ID=a3f2   # set this in every session
```

Resuming after crash or compaction:

```sh
plan register --id a3f2     # reattaches existing session
```

If `PLAN_AGENT_ID` env var is set, `register` reattaches automatically.

> Your ID is NOT remembered by the shell between tool calls. You must store it
> yourself and pass `--agent $AGENT_ID` on commands that need it.

## Core workflow

```sh
plan backlog                                   # see open unassigned tickets
plan ticket pick <id> --agent $AGENT_ID        # claim a ticket
plan ticket note <id> "progress note"          # log progress
plan ticket done <id>                          # mark complete
```

## Commands

```sh
# Init & register
plan init
plan register [--id <hex>]

# Tickets
plan ticket new --title "..." [--epic <name>] [--priority high|medium|low] [--description "..."]
plan ticket list [--status open|in-progress|done|blocked] [--epic <name>] [--assignee <id>]
plan ticket show <id>
plan ticket pick <id> --agent <id>
plan ticket assign <id> <agent-id>
plan ticket done <id>
plan ticket status <id> open|in-progress|done|blocked
plan ticket note <id> "text"
plan ticket unassign <id>
plan ticket delete <id> [--yes]

# Epics (ticket grouping)
plan epic new --name <name> --title "..."
plan epic list
plan epic show <name>

# Agents
plan agent list
plan agent status <id>
plan agent retire <id>

# Overview
plan summary
plan backlog
plan skill
```

## Ticket IDs

- Without epic: `1`, `2`, `3` (flexible: `1` = `01` = `001`)
- With epic: `auth-1`, `auth-2` (flexible: `auth-1` = `auth-01`)
- Create epic first: `plan epic new --name auth --title "Auth system"`

## Ticket statuses

`[ ]` open · `[~]` in-progress · `[x]` done · `[!]` blocked

## Multi-agent etiquette

- Don't pick tickets already assigned to another agent
- Leave notes on tickets you're working on
- Run `plan ticket unassign <id>` if you abandon a ticket
- Check your assignments: `plan ticket list --assignee $AGENT_ID`
