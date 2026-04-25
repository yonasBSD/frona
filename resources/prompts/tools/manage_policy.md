---
id: manage_policy
provider: policy
parameters:
  action:
    type: string
    description: "The action to perform: 'create', 'update', 'delete', 'list', or 'validate'"
    enum:
      - create
      - update
      - delete
      - list
      - validate
  id:
    type: string
    description: "Policy identifier (human-readable, unique per user). Required for create, update, and delete."
  description:
    type: string
    description: "A short description of what this policy does. Required for create."
  policy_text:
    type: string
    description: "The policy statements (without @id/@description annotations — those are added automatically from the id and description parameters). Required for create, update, and validate."
required:
  - action
---
Manage authorization policies that control which tools agents can use, which agents can delegate to each other, and which agents can communicate.

## Policy language

Policies use the Cedar policy language with the `Policy` namespace. Two statement types:
- `permit(...)` — allow an action
- `forbid(...)` — deny an action (overrides permits)

## Entity types

- `Policy::Agent::"agent-id"` — an agent
- `Policy::Tool::"tool-name"` — a specific tool
- `Policy::ToolGroup::"group"` — a tool group (browser, search, web_fetch, task, etc.)

## Actions

- `Policy::Action::"invoke_tool"` — use a tool
- `Policy::Action::"delegate_task"` — delegate work to another agent
- `Policy::Action::"send_message"` — send a message to another agent

## Examples

Allow an agent to use browser tools:
```
permit(
  principal == Policy::Agent::"my-agent",
  action == Policy::Action::"invoke_tool",
  resource in Policy::ToolGroup::"browser"
);
```

Block an agent from delegating to another:
```
forbid(
  principal == Policy::Agent::"junior",
  action == Policy::Action::"delegate_task",
  resource == Policy::Agent::"admin"
);
```

Allow all tools for an agent:
```
permit(
  principal == Policy::Agent::"power-user",
  action == Policy::Action::"invoke_tool",
  resource
);
```

A policy document can contain multiple statements.
