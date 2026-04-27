# Integration and Profile Configuration
Status: Active
Owner: Maintainers
Source of truth: this document for feature-level integration and profile configuration guidance
Parent: [Configuration Feature Guide](configuration.md)

Use this guide when configuring notifications, webhooks, plugins, profiles, and environment-variable substitution. Exact fields and defaults live in [Main Configuration Reference](../configuration.md).

---

## Notifications

`agent.notification` controls local completion/failure/loop/watch notifications and optional sound behavior.

Start with:

- [Notifications feature guide](./notifications.md)
- [Notification configuration reference](../configuration.md#notification-configuration)

---

## Webhooks

`agent.webhook` controls outbound HTTP event delivery.

Safety defaults are conservative:

- HTTPS expected by default
- Insecure HTTP requires explicit opt-in (`allow_insecure_http`)
- Private-network targets require explicit opt-in (`allow_private_targets`)

For signing and retries:

- Use `secret` for signature verification on receivers.
- Keep retry and queue settings bounded.
- Never commit secrets.

References:

- [Webhooks feature guide](./webhooks.md)
- [Webhook configuration reference](../configuration.md#webhook-configuration)

---

## Plugins

Plugins are powerful and not sandboxed.

- Enable only trusted plugins.
- Project-local plugin discovery/settings require repo trust.
- Keep plugin manifests and plugin config minimal and auditable.

References:

- [Plugins feature guide](./plugins.md)
- [Plugin configuration reference](../configuration.md#plugin-configuration)
- [Repo execution trust](../configuration.md#repo-execution-trust)

---

## Profiles

Profiles are named `AgentConfig`-shaped patches for fast workflow switching.

References:

- [Profiles feature guide](./profiles.md)
- [Profiles configuration reference](../configuration.md#profiles)

---

## Environment Variables

Use `${VAR}` or `$VAR` in string values to inject environment-based values (for secrets and machine-specific values).

Unsupported path override variables:

- `RALPH_REPO_ROOT_OVERRIDE`
- `RALPH_QUEUE_PATH_OVERRIDE`
- `RALPH_DONE_PATH_OVERRIDE`

---

## Example

```jsonc
{
  "version": 2,
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "${WEBHOOK_URL}",
      "secret": "${WEBHOOK_SECRET}",
      "events": ["task_completed", "task_failed"]
    }
  }
}
```

---

## See Also

- [Configuration Feature Guide](configuration.md)
- [Main Configuration Reference](../configuration.md)
- [Notifications](./notifications.md)
- [Webhooks](./webhooks.md)
- [Plugins](./plugins.md)
- [Profiles](./profiles.md)
