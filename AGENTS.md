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

**Never verify by matching source text.**

Do not write checks that grep the codebase or documentation for a literal
string. A variable rename, a reformat, or a reworded sentence will break the
check while the behaviour is still correct — and a passing check proves only
that a string exists, not that the feature works.

Incorrect:

```bash
grep -q "const result = Promise.all(" src/services/aiCenter.ts
grep -q "AI-OS v1.0 has one operational execution Agent" HANDOFF.md
```

Correct: run the code and assert on what it does — exit codes, output, HTTP
status, file contents produced, database state. If a behaviour genuinely cannot
be asserted from the outside, put it in 【跑完应该看到】 for manual confirmation
instead of faking it with a text match.

The one exception is verifying that a file or config entry exists at all; even
then, check the file, not a sentence inside it.

Repository tooling:

- `./verify_all.sh` — run every acceptance script and report a summary
- `./done.sh "message"` — run acceptance, commit if it passes, append to the
  `HANDOFF.md` change log
- `./context.sh` — generate the project context block for a new AI session

---

## 6. Diagnosing UI problems

When the owner reports that something in the UI does not respond — a button does
nothing, a dialog does not appear, a panel is missing — **inspect the running
page before changing any code.**

Ask for browser-console output first. Useful checks:

```javascript
// Is the element actually there, and is anything covering it?
(() => { const b = document.querySelector('.the-class'); const r = b.getBoundingClientRect();
  const top = document.elementFromPoint(r.left + r.width/2, r.top + r.height/2);
  return { text: b.textContent, covered: top !== b && !b.contains(top), coveredBy: top?.className }; })()

// What styles is it actually computing?
(() => { const s = getComputedStyle(document.querySelector('.the-class'));
  return { display: s.display, opacity: s.opacity, visibility: s.visibility,
           zIndex: s.zIndex, position: s.position, pointerEvents: s.pointerEvents }; })()

// Does the handler fire, and does the DOM change?
(() => { const before = document.body.innerHTML.length;
  document.querySelector('.the-class').click();
  setTimeout(() => console.log('delta:', document.body.innerHTML.length - before), 300); })()
```

Read the result before forming a hypothesis. `position: static` on an element
that should be an overlay, or an unchanged DOM after a click, points at CSS or
render state — not at the click handler.

**A handler that executes is not proof the logic is correct, and a handler that
appears correct is not proof the user can reach it.** Confirming that a function
exists or that an event fires proves neither. If two independent code paths fail
the same way, the fault is almost certainly downstream of both.

Precedent: the My AI Provider buttons appeared dead for over an hour of logic
rewrites. The handlers were correct throughout; `.provider-setup-backdrop` had
been deleted from `App.css` during the P13 migration, so the dialog rendered
with `position: static` behind the page. One computed-style check would have
found it. Deleting a module can silently remove CSS the surviving UI still
needs, and the build will still pass.

---

## 7. Before any commit

Run and confirm all of the following pass:

```
cargo fmt --check
cargo test
cargo check
npm run build
git diff --check
```

---

## 8. Updating HANDOFF.md

At the end of any completed piece of work, output the exact lines to add or
change in `HANDOFF.md`, following its existing template. Keep it under 150
lines: move superseded detail to `docs/archive/HANDOFF_HISTORY.md` rather than
letting the file grow.

Never leave two conflicting statements about current state in the file.

---

## 9. Token discipline

You are running with a limited budget. The cost is context, not verbosity.
What you read and what your commands print is where the budget goes.

**Reading files**

- Locate first, then read. Use `rg -n "<pattern>" <path>` to find line numbers,
  then `sed -n 'N,Mp' <file>` to read only that range.
- Never `cat` a file over 200 lines.
- Never read `node_modules/`, `dist/`, `target/`, `package-lock.json`, or any
  generated output.
- Read `HANDOFF.md` once at the start of a session. Do not re-read it mid-task.
- Do not read a file "to be safe". Read it because a specific question needs it.

**Running commands**

Filter anything that produces long output:

```bash
npm test 2>&1 | grep -E "FAIL|error" | head -20
npm run build 2>&1 | tail -20
cargo test 2>&1 | grep -E "FAILED|^error" | head -20
git diff --stat        # before git diff
```

Only view a full diff for files you are actually changing.

**Working**

- Change one module per task. Do not refactor code you were not asked to touch.
- Do not re-verify work you already verified in this session.
- If the same fix has failed twice, stop and report what you need. Do not keep
  iterating blind.

**Reporting**

- Report after each file, not after a batch.
- State what changed and the verification result. Do not summarise what you read.

---

## 10. Most important rule

> Continue the project. Do not restart the project.
