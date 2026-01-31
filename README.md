# dumpo

`dumpo` creates a safe, bounded snapshot of a repo and formats it as a paste-ready prompt for LLMs (to clipboard on macOS, or to stdout).

Design goals:
- One command → paste-ready output
- Hard output budget (max per file + max total)
- Stable ordering and deterministic output
- Avoid leaking common secrets by default

Non-goals:
- Dependency graphs, AST parsing, agent workflows, config DSLs

## Install

From the repo:
```bash
cargo install --path .
```

## Usage
- Pack the current directory: `dumpo pack`
- Pack a specific repo root: `dumpo pack /path/to/repo`
- Write to stdout (useful on non-macOS or for piping): `dumpo pack --stdout`
- Copy to clipboard (macOS only; uses pbcopy): `dumpo pack --clipboard`
- Show resolved settings (debug): `dumpo pack --verbose`
- Include / exclude globs:
  - Repeatable glob patterns matched against repo-relative paths (with / separators): `dumpo pack --include 'src/**' --include 'Cargo.toml' --exclude '**/generated/**'`

## Configuration (dumpo.toml)

dumpo can load a dumpo.toml from the nearest ancestor directory of the repo root.

### Precedence:
1. CLI flags (highest)
2. --config <path> (explicit config file)
3. nearest dumpo.toml found by walking ancestors
4. built-in defaults (lowest)

- Disable config loading entirely: `dumpo pack --no-config`


### Example dumpo.toml:

max_file_bytes = 20000
max_total_bytes = 400000
include_hidden = false

# Empty means "include all files" (subject to safety filters)
include = []

# Empty means "exclude nothing" (subject to safety filters)
exclude = []
