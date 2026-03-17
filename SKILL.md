# SKILL: plan — CLI Task Tracker for AI Agents

`plan` manages a `.todo/` folder as persistent state for task tracking across agents,
shell restarts, and context compaction. Multiple agents can work in parallel on the
same project; session identity is derived automatically from the process tree.

## plan vs your internal todo tool

As an AI agent you likely have a built-in todo/task tool. Use them for different things:

- **your internal todo** — short-lived subtasks within a single session or task
- **plan** — persistent, cross-agent, cross-session project work. Survives compaction.
  Visible to humans and other agents.

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

## Session identity — automatic

Your session ID is derived from the PID of the process that invoked `plan`. It is
stable for the lifetime of your process. Your **client name** (e.g. `opencode`,
`claude-code`, `zsh`) is auto-detected and used as a tag on tickets you create.

Override if needed:
```sh
PLAN_AGENT_ID=myagent plan pick 1
PLAN_CLIENT=mybot plan add "ticket"
```

## Core workflow

```sh
plan backlog                         # see open unassigned tickets
plan pick <id>                       # claim a ticket
plan done <id>                       # mark complete
plan backlog -t <tag>                # filter by tag
```

## Commands

```sh
plan init                            # initialize .todo/ store

plan add "title" ["title2" ...]      # create one or more tickets (auto-tagged with your client name)
plan add -t TAG "title" ["title" ...]# create tickets with additional tag(s)

plan pick <id>                       # pick a ticket (assigns to current session, status → picked)
plan unpick <id>                     # unpick a ticket (only if you picked it, status → open)
plan done <id>                       # mark a ticket done
plan block <id>                      # mark a ticket blocked

plan show <id>                       # show full ticket details
plan edit <id> "new description"     # replace ticket description
plan delete <id> [--yes]             # delete a ticket

plan backlog                         # list all open tickets
plan backlog -t TAG                  # list open tickets filtered by tag

plan status                          # project dashboard: ticket counts, active sessions

plan hub                             # show active sessions + unread messages
plan hub "message"                   # broadcast message to all active sessions

plan skill                           # print this file
```

## Tags

- Every ticket is automatically tagged with the creator's client name (`opencode`, `zsh`, etc.)
- Add extra tags at creation: `plan add -t auth -t backend "fix token refresh"`
- Filter backlog by tag: `plan backlog -t auth`
- Tags are plain strings — no registration needed

## Ticket statuses

`[ ]` open · `[~]` picked · `[x]` done · `[!]` blocked

## Multi-agent etiquette

- Check the backlog before picking: `plan backlog`
- Don't pick tickets already assigned to another session
- Use `plan hub` to coordinate before picking overlapping work
- `plan unpick <id>` if you abandon a ticket (only works if you picked it)
- Use tags to carve up areas of work: `plan backlog -t auth`
