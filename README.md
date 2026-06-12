# sync-typing-deps

Keeps `additional_dependencies` in your [mypy](https://github.com/pre-commit/mirrors-mypy) and [ty](https://github.com/mxr/mirrors-ty) pre-commit hooks in sync with your project's dev dependencies.

## Usage

### As a pre-commit hook

Add to `.pre-commit-config.yaml`:

```yaml
-   repo: https://github.com/mxr/sync-typing-deps
    rev: ''  # Use the sha / tag you want to point at
    hooks:
    -   id: sync-typing-deps
```

The hook runs automatically when you change `setup.cfg`, `pyproject.toml`, or `.pre-commit-config.yaml`.

### As a standalone tool

```sh
cargo install sync-typing-deps
sync-typing-deps [--config <path>] [--dir <path>]
```

- `--config` / `-c`: path to `.pre-commit-config.yaml` (default: `.pre-commit-config.yaml`)
- `--dir` / `-d`: directory to search for dep files (default: `.`)

Exits with code `1` if the config was modified (pre-commit convention), `0` if already up to date.

## Supported dep sources

| File | Keys read |
|------|-----------|
| `setup.cfg` | `[options] install_requires`, `[options.extras_require] *` |
| `pyproject.toml` | `[project] dependencies` (PEP 621) |
| `pyproject.toml` | `[build-system] requires` (PEP 517) |
| `pyproject.toml` | `[dependency-groups] dev`, `test` (PEP 735) |
| `pyproject.toml` | `[project.optional-dependencies] dev`, `test` |
| `pyproject.toml` | `[tool.poetry.dev-dependencies]` |
| `pyproject.toml` | `[tool.poetry.group.dev.dependencies]` |
| `custom_components/*/manifest.json` | `requirements` |

## Coverage plugin exclusion

Dependencies listed as coverage plugins are automatically excluded from `additional_dependencies`. Coverage plugins are not type-checking tools and should not be injected into mypy/ty hooks.

Plugin names are read from:

| File | Section / key |
|------|---------------|
| `.coveragerc` | `[run] plugins` |
| `setup.cfg` | `[coverage:run] plugins` |
| `tox.ini` | `[coverage:run] plugins` |
| `pyproject.toml` | `[tool.coverage.run] plugins` |

## How it works

Hooks matched:

- `id: mypy` (any repo)
- `id: ty` (any repo)
- Any hook in a repo whose URL contains `mirrors-mypy`
- Any hook in a repo whose URL contains `mirrors-ty`

`additional_dependencies` is rewritten as a sorted block list. Comments and unrelated YAML formatting are preserved.
