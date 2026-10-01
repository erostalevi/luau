# GitHub preparation (2026-10-01)

Objective: make the repository ready to publish as an open-source project.

- **README.md** rewritten: what Luau is, features (boards, editor, history, AI, integrations), real native
  screenshots (`docs/screenshots/`, captured from the release build with demo data, light + dark), install table,
  build/check commands, data layout, project layout, contributing and license.
- **docs/SPEC.md** (new): the current product & technical specification (v1.0) — concepts, on-disk format, board,
  editor, workspace, history, import/export, AI, integrations, architecture, security model, platforms, limits.
  The planning draft (v0.5) and the questionnaire moved to `docs/planning/` (employer references neutralised).
- **CONTRIBUTING.md** and **SECURITY.md** (new): setup, ground rules, required checks, change notes, releases;
  private vulnerability reporting and the security model.
- **.gitignore**: editors/OS files, `*.tsbuildinfo`, the whole `.claude/` folder.
- **package.json**: author and keywords. **LICENSE**: MIT (unchanged).
- Repo hygiene: 17 merged agent worktrees/branches removed (36 GB of build output freed); tracked files scanned
  for secrets (none) and personal paths (none).
- Reviews: `docs/reviews/2026-10-01-review-1.md` (before fixes) and `-review-2.md` (after).

Before the first push (manual, needs the owner's decisions):
1. Set the real author email and, if wanted, rewrite existing commits (`personal@example.invalid` placeholder).
2. Create the GitHub repo, then replace `OWNER` in `src-tauri/tauri.conf.json` (updater endpoint).
3. Add release secrets: `TAURI_SIGNING_PRIVATE_KEY` (+ password) and, for notarised macOS builds, the `APPLE_*` ones.
4. Optionally rename the local folder `mykanban` → `luau`.
