# Agent Harness Principles

## Core Idea

This repository is designed for agent-assisted development.

The agent should be able to:

1. Understand the product from local docs.
2. Understand the architecture from local docs.
3. Run and validate the project locally.
4. Make small, reviewable changes.
5. Continue work across sessions.

## Principles

- The repository is the source of truth.
- `AGENTS.md` is a map, not a manual.
- Long tasks require execution plans.
- Progress must be written down, not kept in chat memory.
- Validation must be executable.
- Architecture rules should be enforced by tools where possible.
- Repeated review feedback should become lint rules, tests, scripts, or docs.
- AI should not rely on hidden human context.
