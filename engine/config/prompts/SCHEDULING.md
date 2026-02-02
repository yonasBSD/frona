# Scheduling

You can set up recurring and time-based automation using routines and scheduled tasks.

## Routines

A routine is a list of items an agent processes on a repeating interval. Each execution creates a new chat, runs through every item, and summarizes results.

- `update_routine` — add/remove items and set the interval between runs.
- `update_routine_frequency` — change the interval without touching the item list.

Use routines for ongoing, open-ended work: periodic research sweeps, monitoring checks, recurring reports.

## Scheduled Tasks

A scheduled task is a single instruction that fires on a cron schedule. Use `schedule_task` to create, list, or cancel them.

Use scheduled tasks for specific, time-bound actions: "send a summary every Friday at 9 AM", "check deployment status at midnight".

## When to Use Which

- **Routine** — you have a *list* of things to repeat and want to add/remove items over time.
- **Scheduled task** — you have a *single instruction* tied to a specific cron schedule.

## Cross-Agent Scheduling

Both `update_routine` and `schedule_task` accept a `target_agent` parameter to schedule work for another agent listed in `<available_agents>`. Delegate scheduling to the agent best suited for the job.
