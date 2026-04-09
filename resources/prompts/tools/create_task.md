---
id: create_task
provider: task
parameters:
  title:
    type: string
    description: A short title for the task
  instruction:
    type: string
    description: "Detailed, self-contained instructions. The target agent cannot see this conversation, so include all necessary context. When run_at or delay_minutes is set, omit timing language — the scheduler handles the delay."
  target_agent:
    type: string
    description: "Optional: agent name to assign to (from <available_agents>). Omit to create a task for yourself."
  process_result:
    type: boolean
    description: "If true, you will be resumed with the task result when it completes. Multiple tasks can run in parallel — you resume when all complete. Default: false (fire-and-forget, result posted to chat)."
  cron_expression:
    type: string
    description: "5-field cron expression (minute hour day-of-month month day-of-week) for recurring tasks. Omit for one-off tasks."
  delay_minutes:
    type: integer
    description: "Defer execution by N minutes. Cannot be used with run_at or cron_expression."
  run_at:
    type: string
    description: "Defer execution to a specific time. Accepts a unix timestamp or ISO 8601 datetime. Must be in the future. Cannot be used with delay_minutes."
required:
  - title
  - instruction
---
Create a task — one-off or recurring, for yourself or another agent. When a specialist agent in <available_agents> matches the work, always delegate by setting target_agent. Also use to: defer work to a later time, run background work in a separate context, or parallelize across multiple agents. Omit target_agent to create a task for yourself. Set cron_expression for recurring work. Set process_result to receive and act on the result yourself; omit it to let the result post directly to the chat. For periodic autonomous check-ins, use set_heartbeat.
