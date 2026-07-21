# Mesh Language (VS Code)

Syntax highlighting for [Mesh](../../README.md) — no language server, no diagnostics, just a
TextMate grammar so `.mesh` files aren't rendered as plain text.

## What's here

- `package.json` — extension manifest, registers the `mesh` language for `.mesh` files
- `language-configuration.json` — comment toggling (`//`), bracket matching/auto-closing
- `syntaxes/mesh.tmLanguage.json` — the TextMate grammar (keywords, builtins, strings with
  `${...}` interpolation, discriminated-union `{ kind: "..." }` shapes, generics, etc.)

## Installing locally

This isn't published to the Marketplace. To use it in your own VS Code:

```sh
mkdir -p ~/.vscode/extensions/mesh-language
cp -r editors/vscode/* ~/.vscode/extensions/mesh-language/
```

Restart VS Code (or run "Developer: Reload Window"). Opening any `.mesh` file should now show
highlighted keywords, types, builtins, and strings.

Alternatively, symlink instead of copying so grammar edits show up without re-copying:

```sh
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/mesh-language
```

## Known limits (v1 — syntax highlighting only)

- No language server: no real diagnostics, go-to-definition, or hover info in the editor itself
  (use `mesh check`/`mesh fmt` from the CLI, or the `mesh check --json` output, for that).
- Highlighting is regex-based (TextMate grammar), not a real parser, so it can be fooled by
  sufficiently unusual code — same caveat as every other TextMate-grammar-based language.
