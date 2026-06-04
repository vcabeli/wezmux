# Contributing

This repository is maintained by [@pierrickmartos](https://github.com/pierrickmartos).

## How to contribute

Contributions are welcome! Feel free to open a pull request if you'd like to suggest improvements, fix issues, or add new features.

## Development setup

**macOS only** — claude-tower depends on the `security` CLI to read the macOS keychain and assumes the [`cmux`](https://cmux.io) binary is on PATH.

```bash
go mod tidy          # sync deps
go build             # produces ./claude-tower
./claude-tower       # run the TUI
```

## Architecture

Before touching the event pipeline, read [`CLAUDE.md`](CLAUDE.md) — it documents the data flow (cmux events → registry → summarizer), the key invariants (single UI-state owner, debounced summaries, session-ID normalization), and the non-obvious constraints (OAuth headers, transcript tailing, status transitions).

## Validating before committing

CI enforces formatting, static checks, and tests. Run the same checks locally before opening a PR:

```bash
gofmt -l .           # must print nothing
go vet ./...
go test -race ./...
```

## Releases

Every merge to `main` automatically publishes a GitHub release: the workflow bumps the patch version, cross-compiles darwin arm64/amd64 binaries, and attaches them with auto-generated notes.

## Pull Request Guidelines

- **One change per PR** — keep PRs focused and easy to review.
- **Describe your changes** — explain what you changed and why in the PR description.
- **Open an issue first** for significant changes to discuss the approach before investing time.
- **Keep it concise** — prefer small, incremental improvements over large sweeping changes.
- **Be respectful** — follow a constructive and collaborative tone in all discussions.

## License

By contributing, you agree that your contributions will be licensed under the same license as this repository.
