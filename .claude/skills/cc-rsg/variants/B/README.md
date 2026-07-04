# variants/B/ — Context Optimization mode B

This directory holds an **alternative execution mode** for cc-rsg in which
the main agent delegates each chapter to an isolated `chapter-investigator`
sub-agent via Claude Code's Task tool, instead of writing the chapter body
itself.

## When to use mode B

- **Large codebases** where the conversation context would otherwise
  exceed the model's window if all chapter bodies accumulate in main
- Cases where each chapter benefits from a fresh, isolated reasoning
  context (no cross-chapter contamination)

The default (top-level `SKILL.md`) mode A is "main agent writes each
chapter itself in-line during Phase 3 STEP F". Mode B is opt-in.

## What's here

| File | Purpose |
|---|---|
| `SKILL.phase3-stepG.md` | Overrides Phase 3 STEP G to add Task-based per-chapter delegation, manifest relay, and the mode B return-value contract |
| `chapter-investigator.md` | A mode-B variant of the standard `chapter-investigator` sub-agent: the return value carries only the chapter path + a short summary (no body), keeping the main agent's context lean |

## How to activate

1. In Phase 0, when persisting `goal.json`, add a top-level field:

   ```json
   { "...": "...", "context_optimization_mode": "B" }
   ```

2. From Phase 3 onwards, the main agent reads
   `variants/B/SKILL.phase3-stepG.md` for STEP G semantics and delegates
   chapter authoring via the Task tool with `subagent_type =
   "chapter-investigator"` configured against
   `variants/B/chapter-investigator.md`.

Mode B is **not** required for normal cc-rsg use; treat this directory as
a reference variant.

## Runtime notes

- The Task tool dispatches each sub-agent in an isolated context. Token
  usage is 5–10× higher than mode A because the prompt cache is not
  shared across sub-agents.
- Hosts integrating cc-rsg into a non-Claude-Code runtime should ensure
  their equivalent of the Task tool genuinely produces isolated contexts;
  in-process execution defeats the purpose of mode B.
- When chapter delegation is not desired or the Task tool is unavailable,
  fall back to mode A (the top-level `SKILL.md`) — there is no setup
  required to do so.
