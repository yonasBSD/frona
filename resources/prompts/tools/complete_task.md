---
id: complete_task
provider: task
parameters:
  result:
    type: string
    description: The full markdown message to deliver to the user or parent agent. Write the complete result — not a summary. Omit when the task's action was already performed (reminders, calls, deploys) — don't duplicate work that's already done.
  deliverables:
    type: array
    items:
      type: string
    description: File paths (relative to your workspace) to deliver as output artifacts. Only listed files are delivered.
required: []
---
Signal that the current task is complete. Provide `result` with the full deliverable text when the task produces information (research, analysis, answers). List output files in `deliverables`. When the task's action was already performed (sent a reminder, made a call, triggered a deploy), call with no arguments — don't duplicate work that's already done.
