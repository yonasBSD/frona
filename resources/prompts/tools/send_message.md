---
id: send_message
group: messaging
parameters:
  content:
    type: string
    description: The message text to send to the user (supports markdown)
  attachments:
    type: array
    items:
      type: string
    description: File paths to attach to the message
required:
  - content
---
Send a message to the user. The message will be delivered to the most relevant chat automatically. Use this to proactively notify the user about important information, completed work, reminders, or anything they should know about.
