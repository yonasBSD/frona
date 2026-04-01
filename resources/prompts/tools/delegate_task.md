---
id: delegate_task
group: delegate
parameters:
  target_agent:
    type: string
    description: The name of the agent to delegate the task to (from <available_agents>)
  title:
    type: string
    description: A short title for the task
  instruction:
    type: string
    description: "Detailed instructions for the target agent. When run_at is set, omit any timing/delay language (e.g., 'after 2 minutes', 'in 1 hour') — the scheduler handles the delay, so the instruction should only describe what to do."
  delay_minutes:
    type: integer
    description: "Number of minutes to wait before executing the task. Cannot be used with run_at."
  run_at:
    type: string
    description: "Optional future time to defer execution. Accepts a unix timestamp (e.g., from `date -d '+2 minutes' +%s`) or ISO 8601 datetime. Must be in the future. If omitted, the task runs immediately."
required:
  - target_agent
  - title
  - instruction
---
The default delegation tool. Delegate a task to another agent — fire-and-forget. The result is posted directly to this chat for the user; your tool loop is NOT resumed. Returns immediately with a task ID. Optionally set run_at to defer execution. For relative delays, use shell first: `date -d "+2 minutes" +%s` to get the timestamp. Omit timing language from instruction — just describe the action. For recurring scheduled work, use schedule_task. For periodic autonomous check-ins, use set_heartbeat.
