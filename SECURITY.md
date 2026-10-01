# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Use GitHub's
**"Report a vulnerability"** (private security advisory) on this repository instead. Include steps to reproduce,
the affected version/OS and the impact you expect. You will get an answer within a few days.

## Scope and design

Luau is a local desktop app. Its security model is summarised in
[`docs/SPEC.md` §12](docs/SPEC.md#12-security-model). In short:

- the webview is treated as untrusted: file operations only accept locations picked in native dialogs, the app's own
  folders and registered boards;
- board files are served through `luau://` with normalised paths (never `.luau/` or `.git/`, no symlinks out of the
  board) and a sandboxing CSP;
- code cells only run in boards you explicitly trust, through a native confirmation;
- link previews are fetched by the backend with SSRF protection;
- integration tokens are stored in the OS keychain; logs never contain card text, tokens or emails.

Findings that bypass any of these are in scope.
