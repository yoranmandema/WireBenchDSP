# Rules for agents working in pedal-dsp

These rules apply to every agent session in this repo, including kanban workers.
Kanban runs strictly one worker at a time on a rate-limited model. Spend the
budget on code that ends up committed.

## Scope
- Work only on your own card. Do the smallest change that meets its acceptance
  criteria, then stop.
- If the card is too big for one session, finish one verifiable slice, commit it,
  and record the remaining steps as a kanban comment on the same card. Then call
  `kanban_block` with kind `needs_input`, or complete the card with a summary that
  lists what is left. Do not create subcards.
- Never run `hermes kanban specify`/`decompose` or assign cards to anyone. Do not
  create new cards unless the card body asks for them.

## Commit discipline (non-negotiable)
- Commit to `main` after every verified step. Use small commits with messages in
  the style `area: what changed (card id)`, for example `ui: clamp knob labels (t_adc96a8c)`.
- Commit only what you changed for this card. Stage files by path, never
  `git add -A`, and never commit `.DS_Store`, `.vscode/`, `ui/standalone-tmp/`,
  `target/`, or `*.wav`.
- Never leave the tree dirty when you finish, block, or hit a rate limit:
  - Commit the work that passes the checks below.
  - If something doesn't pass, `git stash push -m "<card id> wip"` and say so in a
    kanban comment.
- Don't rewrite history or force-push.

## Verify before you commit
- Rust: `cargo test --workspace --exclude pedal-shell`. It must stay green.
- UI: `cd ui && pnpm run build`. This typechecks and builds. Commit the rebuilt
  `ui/dist/` together with the matching `ui/src` change.
- If you touch the Tauri shell, note that a plain `cargo build` produces a dev
  build that loads Vite's dev URL. A self-contained binary needs
  `--features custom-protocol`.

## Audio safety
- Never play sound on this machine. Run the engine with `--headless` (simulated
  device, output discarded) and check audio behaviour with tests or offline
  renders. Never open the speakers, headphones, or the Scarlett output.

## Context
- Read `docs/HANDOFF.md` and `docs/ROADMAP.md` for product context.
- Read `docs/IPC-PROTOCOL.md` before changing anything between the engine and
  the UI.
- Unreviewed work from earlier parallel runs is on the branch
  `wip/parallel-workers-2026-11-15`. If your card's work already exists there,
  port it. Don't rewrite it from scratch.
