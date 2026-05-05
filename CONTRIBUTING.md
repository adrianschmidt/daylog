# Contributing to vitalog

## Quick Start

```bash
git clone https://github.com/your-username/vitalog
cd vitalog
cargo build
cargo test
```

## Development

```bash
just build    # cargo build
just test     # cargo test
just lint     # cargo fmt --check && cargo clippy
just demo     # init with demo data + run TUI
```

## Ways to Contribute

### Submit a preset
Use different exercises or metrics? Share your `config.toml` in an issue or PR.

### Build a module
See [AGENTS.md](AGENTS.md#3-extending) for the scaffolding guide and [CLAUDE.md](CLAUDE.md) for code conventions.

### Report bugs
Open an issue with:
- What you ran
- What you expected
- What happened instead
- Your OS and `vitalog --version`

## Code Conventions

- No `.unwrap()` in library code — use `color_eyre::Result`
- `cargo fmt` and `cargo clippy` clean
- Every user-facing error gets a `.suggestion()` with a concrete next step
- Modules are self-contained — never import from another module

## Commit conventions

vitalog uses [Conventional Commits](https://www.conventionalcommits.org/)
to drive automated releases. The accepted types are:

| Type       | Effect on next release |
|------------|------------------------|
| `feat`     | minor bump             |
| `fix`      | patch bump             |
| `perf`     | patch bump             |
| `BREAKING CHANGE:` footer (or `feat!:` / `fix!:`) | major bump |
| `docs`, `style`, `refactor`, `test`, `build`, `ci`, `chore` | no bump |

A commit subject line looks like:

```
feat(today): show streak counter on the dashboard

Some optional body explaining motivation and trade-offs.
```

Scope (the `(today)` part) is optional but encouraged. Use the module
or feature area, not a file path.
