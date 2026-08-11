# AI Workflow

## Standard Task Flow

1. Read `AGENTS.md`, then only the task-relevant docs.
2. Run `git status --short` and protect existing changes.
3. Trace the affected Qt signals/slots, ownership, protocol state, and platform branches before editing.
4. For a scoped task, make the smallest coherent change. For complex work, create an execution plan under `docs/exec-plans/active/`.
5. Run the configured wrappers relevant to the change and perform the manual workflow checks described in `docs/TESTING.md` when needed.
6. Update product, architecture, security, testing, decision, or progress docs when their facts change.
7. Report changed files, validation, and remaining risk.

## Long-Running Work

- Keep goals, scope, steps, decisions, validation, and open questions in an active execution plan; do not rely on chat history.
- Add dated, factual checkpoints to the plan and `docs/agent-harness/progress.md` when another session may need to continue.
- Mark steps complete only after their stated validation succeeds.
- Move the finished plan to `docs/exec-plans/completed/` and record follow-up work separately.
- Use `docs/agent-harness/features.json` only when a future multi-feature effort needs structured end-to-end readiness tracking.

## Review and QA

Use an independent review pass for cryptography, network protocol, path handling, cross-platform packaging, or broad state-machine changes. For desktop UI changes, launch the native app and exercise the affected tray/dialog path on each relevant platform when available.

Do not mark a user-visible feature passing based only on code inspection or compilation. Record the actual peer-to-peer or UI path exercised, including both endpoints when transfer behavior is involved.

## Handoff Checklist

- Active plan and progress notes reflect the current state.
- Partial work is clearly separated from validated behavior.
- Commands and outcomes are recorded.
- Compatibility, platform, security, and manual-test gaps are explicit.
