---
id: manage_service
group: app
parameters:
  action:
    type: string
    description: "Action to perform: deploy, stop, start, restart, destroy, status"
    enum:
      - deploy
      - stop
      - start
      - restart
      - destroy
      - status
  manifest:
    type: object
    description: "Full service manifest. Required for deploy/start. For stop/restart/destroy, only `id` is needed."
required:
  - action
---
Manage web services deployed from your workspace. You can build a web app (any language/runtime), then deploy it as a live service accessible via URL.

## Workflow

1. Choose an app id (e.g. `my-dashboard`)
2. Create the directory `apps/{id}/` and write your source code there
3. Deploy with `manage_service` — commands run from `apps/{id}/` automatically

## Actions

- **deploy** — Start a new service or update an existing one. Requires user approval on first deploy or when security-relevant manifest fields change (command, kind, network destinations, credentials, paths, expose). Code-only changes restart automatically without re-approval.
- **stop** — Stop a running service. Process is terminated but the app entity is preserved.
- **start** — Start a previously stopped service.
- **restart** — Stop and restart a running service.
- **destroy** — Stop the service and delete the app entity permanently.
- **status** — Check the status of your services. If manifest.id is provided, returns that specific app. Otherwise returns all apps.

## Manifest Format

Always pass the full manifest with every call. The system automatically detects changes and triggers re-approval when needed.

```json
{
  "id": "my-dashboard",
  "name": "My Dashboard",
  "description": "A dashboard for monitoring metrics",
  "kind": "service",
  "command": "python app.py",
  "network_destinations": [
    {"host": "api.example.com", "port": 443}
  ],
  "health_check": {
    "path": "/healthz",
    "interval_secs": 10,
    "initial_delay_secs": 5
  },
  "restart_policy": "on_failure",
  "hibernate": true
}
```

### Key Fields

- **id** (required): Stable identifier, e.g. "gold-dashboard". Your app lives in `apps/{id}/`.
- **name** (required): Human-readable name
- **kind**: "service" (default) runs a command, "static" serves files
- **command**: Startup command (e.g. `python app.py`). Runs from `apps/{id}/`, so just use the filename. Your app MUST listen on the PORT environment variable.
- **static_dir**: Directory to serve for static mode, relative to workspace (e.g. "dist/")
- **network_destinations**: Allowed outbound network destinations (host + port pairs)
- **credentials**: Vault credentials to inject as environment variables
- **health_check**: Health check configuration (path, interval, timeout)
- **restart_policy**: "on_failure" (default), "always", or "never"
- **hibernate**: Auto-hibernate after idle period (default: true). Set false for always-on services.
- **expose**: Whether to expose via reverse proxy (default: true). Set false for background workers.

### Logs

App output (stdout and stderr) is written to `apps/{id}/logs/app.log`. Read it with the shell tool to debug issues.

### Environment Variables

For service mode, your app receives:

- **PORT** — The port your app must listen on.

Your app is served behind a reverse proxy. The proxy strips the path prefix on inbound requests, so your server-side code sees requests at `/`.

**Important:** In client-side code (HTML, JavaScript), always use **relative paths** without a leading slash. For example, use `fetch('api/prices')` not `fetch('/api/prices')`, and `<a href="login">` not `<a href="/login">`. Absolute paths (starting with `/`) bypass the app proxy and hit the main server instead.

The full JSON Schema for the manifest is available at: {{schema_path}}

### Static Mode

For static sites, build your HTML/CSS/JS and specify the output directory:

```json
{
  "id": "docs-site",
  "name": "Documentation",
  "kind": "static",
  "static_dir": "build/"
}
```
