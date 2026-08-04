# AGENTS.md — AI-OS Entry Point

This file is the entry point for any AI assistant working on this repository
(Codex, Claude, Gemini, Cursor, or any other). Read this file first.

---

## 1. Reading order

1. **`HANDOFF.md`** — the current state of the repository. This is the ONLY
   authoritative source for what is done, what is in progress, and what comes next.
2. **`AI_OS_MASTER_GUIDE.md`** — product direction, target architecture, roadmap,
   and development rules.

**Conflict resolution:**

- Current status, branch, milestone, or "what is done" → `HANDOFF.md` wins.
- Product direction, architecture, boundaries, or scope → `AI_OS_MASTER_GUIDE.md` wins.
- Any other `.md` file in this repository is a design or planning reference.
  It never describes current implementation state.
- Existing source code is the implementation baseline. Preserve and evolve it.
  Do not restart or redesign.

---

## 2. Who you are working with

The project owner does not read or write code. He works as product owner and
acceptance tester only. He will:

- Describe the desired outcome
- Paste code you give him into VS Code, or run one command in the terminal
- Report the terminal output back to you

He will not make technical judgements. Architecture, technology selection, and
code organisation are **your** responsibility. Decide, then record the decision
in the `Technical decisions` section of `HANDOFF.md`.

If a task requires him to make a technical judgement, the task is not broken
down far enough.

---

## 3. Before proposing any change

1. Check `HANDOFF.md` → `Completed` and `Rejected` sections. Do not propose
   something that is already built or already ruled out.
2. Confirm which module owns the responsibility (see Master Guide §4).
3. Inspect the existing implementation before changing it.
4. Propose the smallest correct change.

State explicitly which section of `HANDOFF.md` you checked. If you cannot
confirm, ask — do not assume.

---

## 4. How to deliver work

Reply in this format, with no extra commentary:

```
【做什么】One sentence describing the effect
【改哪里】File path + function or line range
【代码】The complete function or block to replace
【验收脚本】Save as verify/verify_<feature>.sh
【怎么跑】One command
【跑完应该看到】Expected successful output
```

Rules:

- Do **not** modify code files with `python -c`, `sed`, or `awk` string
  replacement. Give the complete block; the owner replaces it in VS Code.
- Terminal commands are for creating files, installing dependencies, running
  tests, and git only.
- Only give commands that run on a standard macOS terminal.
- Give complete runnable code. No ellipses, no "rest unchanged".
- Mark destructive operations (delete, overwrite, config change) with
  【⚠️破坏性操作】, list the affected files, and give a backup command first.
- If the same problem survives two fix attempts, stop and list what information
  you need. Do not keep guessing.

---

## 5. Acceptance

Every feature ships with an acceptance script at `verify/verify_<feature>.sh`:

- Pass → print `PASS: <feature>` and `exit 0`
- Fail → print `FAIL: <feature>` with the reason and `exit 1`
- Print `✅` or `❌` per individual check
- Verify **actual behaviour**, not that a function exists
- Anything that cannot be checked automatically (browser behaviour, visual
  result) goes in 【跑完应该看到】 for the owner to confirm manually

Repository tooling:

- `./verify_all.sh` — run every acceptance script and report a summary
- `./done.sh "message"` — run acceptance, commit if it passes, append to the
  `HANDOFF.md` change log
- `./context.sh` — generate the project context block for a new AI session

---

## 6. Before any commit

Run and confirm all of the following pass:

```
cargo fmt --check
cargo test
cargo check
npm run build
git diff --check
```

---

## 7. Updating HANDOFF.md

At the end of any completed piece of work, output the exact lines to add or
change in `HANDOFF.md`, following its existing template. Keep it under 150
lines: move superseded detail to `docs/archive/HANDOFF_HISTORY.md` rather than
letting the file grow.

Never leave two conflicting statements about current state in the file.

---

## 8. Most important rule

> Continue the project. Do not restart the project.
