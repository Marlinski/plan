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

## Setup

No setup needed. `.todo/` is created automatically at the git root (or cwd) on
the first `plan` command.

## Session identity — automatic

Your session ID is derived from the PID of the process that invoked `plan`. It is
stable for the lifetime of your process. Your **client name** (e.g. `opencode`,
`claude-code`, `zsh`) is auto-detected and used as a tag on tickets you create.

Override if needed:
```sh
PLAN_AGENT_ID=myagent plan todo pick 1
PLAN_CLIENT=mybot plan todo add "ticket"
```

## Core workflow

```sh
plan todo                            # see open unassigned tickets (backlog)
plan todo pick <id>                  # claim a ticket
plan todo done <id>                  # mark complete
plan todo backlog -t <tag>           # filter by tag
```

## Commands

```sh
plan todo add "title" ["title2" ...]      # create one or more tickets (auto-tagged with your client name)
plan todo add -t TAG "title" ["title" ...]# create tickets with additional tag(s)

plan todo pick <id>                       # pick a ticket (assigns to current session, status → picked)
plan todo unpick <id>                     # unpick a ticket (only if you picked it, status → open)
plan todo done <id>                       # mark a ticket done
plan todo block <id>                      # mark a ticket blocked

plan todo show <id>                       # show full ticket details
plan todo edit <id> "new description"     # replace ticket description
plan todo delete <id> [--yes]             # delete a ticket

plan todo backlog                         # list all open tickets
plan todo backlog -t TAG                  # list open tickets filtered by tag

plan status                               # project dashboard: ticket counts, active sessions

plan hub                                  # show active sessions + unread messages
plan hub "message"                        # broadcast message to all active sessions

plan skill                                # print this file
```

## Tags

- Every ticket is automatically tagged with the creator's client name (`opencode`, `zsh`, etc.)
- Add extra tags at creation: `plan todo add -t auth -t backend "fix token refresh"`
- Filter backlog by tag: `plan todo backlog -t auth`
- Tags are plain strings — no registration needed

## Ticket statuses

`[ ]` open · `[~]` picked · `[x]` done · `[!]` blocked

## Multi-agent etiquette

- Check the backlog before picking: `plan todo backlog`
- Don't pick tickets already assigned to another session
- Use `plan hub` to coordinate before picking overlapping work
- `plan todo unpick <id>` if you abandon a ticket (only works if you picked it)
- Use tags to carve up areas of work: `plan todo backlog -t auth`
