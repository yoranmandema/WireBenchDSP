# Rules for agents working in pedal-dsp

These rules apply to every agent session in this repo, including kanban workers.
Kanban runs strictly one worker at a time, because every worker shares this one
working tree. Spend the budget on code that ends up committed.

## Roles (the kanban board)
Your card's assignee says which role you are. Each role runs on a model sized
for its job, so stay in your lane:

| Assignee | Model | Job |
|---|---|---|
| `pedal-architect` | Opus 5.5 (expensive; #1 on frontend leaderboards) | Design and plan, including UI and visual design. Turns a feature into small, precise builder cards. Writes little code. |
| `pedal-ui-builder` | Haiku 5.5 (cheap) | Implements one UI card: Preact, TypeScript and CSS in `ui/`. |
| `pedal-builder` | Haiku 5.5 (cheap) | Implements one non-UI card: the Rust engine, DSP, tooling, docs. |
| `pedal-reviewer` | Sonnet 5.5 | Checks a builder card against its acceptance criteria, then approves or sends it back. |

### pedal-architect
- Read the code the feature touches, plus `docs/ARCHITECTURE.md` and
  `docs/IPC-PROTOCOL.md`. If the design is non-trivial (a data model, a protocol
  change, threading, or DSP), write it down in `docs/design/<topic>.md` and commit.
- Split the work into **builder cards** with `kanban_create`. Each card must be:
  - assigned to `pedal-ui-builder` if it touches `ui/` (components, styling,
    layout, interaction), or to `pedal-builder` otherwise. Split a mixed change
    into an engine card and a UI card, with the UI card depending on the engine
    one. Use `workspace_kind: dir`,
    `workspace_path: /Users/yoran/Documents/Repos/pedal-dsp`, and the same
    `priority` as your own card;
  - one session of work: roughly 1–4 files, and it must leave the app working;
  - concrete: the exact files, types and functions to touch, the approach, and
    the IPC messages if any. Also give acceptance criteria phrased as checks,
    the verify commands, and the "Try it" steps for the product owner. A
    builder that reads only the card and this file must not need to make design
    decisions.
- Order dependent cards with `parents`. Use at most ~6 cards per feature.
- Builder cards for the feature may already exist from an earlier automatic
  split; they're often vague because that split never read the code. Don't
  duplicate them. Add your precise spec as a comment on each existing card
  (`hermes kanban --board pedal comment <id> "..."`), and only create cards for
  missing work.
- Only for a card that is genuinely hard (real-time audio-thread code, DSP maths,
  lock-free or concurrency work, a subtle protocol migration, or a UI change
  spanning many components), pin it with
  `model: anthropic/claude-sonnet-5.5`, `provider: nous`. Everything else stays
  on the builders' default cheap model. The daily model allowance is small, so
  make this rare.
- For UI cards, state the visual intent precisely: sizes, spacing scale,
  colours or tokens, states (hover, active, disabled, focus), behaviour at
  ~800 px and ~1440 px wide, and what must not move or change. The UI builder
  is a cheap model: the more exact the card, the better the result.
- If a product decision is needed, block your own card with `needs_input` (see
  below) before creating cards.
- Finish with `kanban_complete`, not review. The first line of the summary says
  what will change for the user once the cards are built, e.g. "Planned
  cross-channel routing: 5 build steps queued". List the card ids in
  `created_cards`.

### pedal-builder and pedal-ui-builder
- Do what the card says, and only that. If the spec turns out to be wrong or
  ambiguous, don't redesign. Block with `needs_input` and ask one precise
  question.
- UI builders: look at your result before handing off (see "Looking at the UI"
  below), at ~800 px and ~1440 px wide.
- If the card is too big for one session, finish one verifiable slice, commit it,
  record the remaining steps as a comment on the card, then block with
  `needs_input`. Don't create cards.
- When done, hand off with `kanban_request_review` and `reviewer:
  pedal-reviewer`, using the product-owner summary format below.

### pedal-reviewer
- Find the card's commits with `git log --oneline --grep <card id>`, read the
  diff, run the verify commands, and check each acceptance criterion.
- For anything that changes `ui/`, also look at it (see "Looking at the UI"):
  screenshot it at ~800 px and ~1440 px wide and check it with the vision tool
  against the card's visual intent. Check alignment, overflow, clipping and
  states. A UI card that builds but looks wrong is not done.
- Fix trivial issues yourself (typos, a missing import, a few lines) and commit
  them. That costs less than another round trip.
- Approve with `kanban_complete`. Keep the builder's product-owner summary and
  its "Try it" steps.
- Otherwise call `kanban_request_changes` with a numbered list of specific
  fixes. If you are about to request changes on this card for the **second**
  time, first escalate it to a stronger model (either builder):
  `hermes kanban --board pedal set-model <card id> anthropic/claude-sonnet-5.5 --provider nous`.
  For a third time, block with `needs_input` instead.
- **Feature cards** are the user's original request. Their parents are the build
  steps, so they run only after all of those are done. Check the original ask
  end to end against the code. If it's met, complete the card with a
  product-owner summary of the whole feature. If something is missing, block
  with `needs_input` and list the gaps so the user can send them back through
  Triage.

### Everyone
- Work only on your own card.
- Never run `hermes kanban specify`/`decompose` or reassign cards. Only the
  architect creates cards.

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

## Looking at the UI (UI builders and reviewers)
Run a private, silent copy of the app on different ports. Never touch 8737 or
5173, because the user's own app may be using them:

```sh
./target/debug/pedal-engine --headless --port 18737        # background; no audio device, output discarded
cd ui && VITE_ENGINE_URL=ws://127.0.0.1:18737 pnpm exec vite --port 15173 --strictPort   # background
```

Open `http://localhost:15173` with the browser tool, set the viewport to
~1440×900 and then ~800×600, take screenshots, and check them with vision.
Click through the states the card touches (select a block, open the modal,
drag). Afterwards, kill both processes (`lsof -ti tcp:18737,15173 | xargs kill`).
If `pnpm` isn't on PATH, use `~/.hermes/tools/node-*/bin` and
`~/.hermes/tools/pnpm-*`, like `shell/run.sh` does.

## Handoffs to the product owner (the user)
The user is the product owner, not a code reviewer. The `summary` you pass to
`kanban_complete` or `kanban_request_review` is what pops up as their desktop
notification. Write it for someone who will open the app and try the change:

1. First line (shown in the notification): what is different in the app, in
   plain words. Example: "Knob min/max labels no longer overlap; they sit just
   outside the ring in smaller text." Don't put file names or test counts here.
2. **Try it:** the exact command to run the app (see `docs/RUN-THE-APP.md`) and
   what to click, drag or listen for.
3. **Not done / known limits:** anything left out on purpose.
4. Last: commit hash(es). Put test results in `metadata`, not in the summary.

Reviewers who complete a card after review must keep this format and carry the
"Try it" steps forward. Don't replace the summary with "approved".

## When you need a decision
If the card needs a product decision (look, feel, sound, scope) or something
only the user can do (hardware, accounts, listening tests), don't guess. Call
`kanban_block` with kind `needs_input`, and make the reason:
- one concrete question in one sentence,
- 2–3 labelled options with a one-line trade-off each,
- your recommendation.

Commit or stash your work first. The user answers with a comment on the card
and unblocks it. Read their comment before you continue.

## Audio safety
- Clean up after yourself. Before you complete, block, or stop, kill every
  `pedal-engine`, `pedal-shell` and Vite (`pnpm dev`) process you started.
  A leftover engine holds port 8737, and the user's app then silently talks to
  your stale engine instead of its own.
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
