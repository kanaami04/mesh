# Mesh Language (VS Code)

Syntax highlighting for [Mesh](../../README.md) — no language server, no diagnostics, just a
TextMate grammar so `.mesh` files aren't rendered as plain text.

## What's here

- `package.json` — extension manifest, registers the `mesh` language for `.mesh` files
- `language-configuration.json` — comment toggling (`//`), bracket matching/auto-closing
- `syntaxes/mesh.tmLanguage.json` — the TextMate grammar (keywords, builtins, strings with
  `${...}` interpolation, discriminated-union `{ kind: "..." }` shapes, generics, etc.)

## Installing

This isn't published to the Marketplace, so install it from a locally built `.vsix`.

**Do not hand-copy (or symlink) this directory into `~/.vscode/extensions/`.** VS Code only
trusts extension folders it installed itself; a hand-dropped folder makes every VS Code window
open with an "invalid extension" warning until you delete it.

Build the package (needs network for `npx`):

```sh
cd editors/vscode
npx @vscode/vsce package --skip-license --out mesh-language.vsix
```

Then install it:

```sh
code --install-extension mesh-language.vsix
```

Reload the window ("Developer: Reload Window"). Opening any `.mesh` file should now show
highlighted keywords, types, builtins, and strings.

### Remote-SSH: keep it on the server only

`package.json` declares `"extensionKind": ["workspace"]`, so VS Code installs and activates this
extension on the **remote** side, not on your laptop. Run the `code --install-extension` above
from a terminal inside the Remote-SSH window (or from the server's own
`~/.vscode-server/cli/servers/Stable-*/server/bin/code-server`) and `.mesh` highlighting will be
there when you're connected and absent — with no leftover warning — when you're not.

If you also edit `.mesh` files locally, install the same `.vsix` on the local machine as well;
just install it, don't copy the folder.

### After editing the grammar

Re-package and re-install (`--force` overwrites the same version), then reload the window.

## Known limits (v1 — syntax highlighting only)

- No language server: no real diagnostics, go-to-definition, or hover info in the editor itself
  (use `mesh check`/`mesh fmt` from the CLI, or the `mesh check --json` output, for that).
- Highlighting is regex-based (TextMate grammar), not a real parser, so it can be fooled by
  sufficiently unusual code — same caveat as every other TextMate-grammar-based language.
