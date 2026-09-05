# sync-typing-deps

Keeps `additional_dependencies` in your Python typing pre-commit hooks in sync with your project's dev dependencies.

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

Returns `1` if config is updated and `0` otherwise.

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

Dependencies listed as coverage plugins are automatically excluded from `additional_dependencies`. Coverage plugins are not type-checking tools and should not be injected into hooks.

Plugin names are read from:

| File | Section / key |
|------|---------------|
| `.coveragerc` | `[run] plugins` |
| `setup.cfg` | `[coverage:run] plugins` |
| `tox.ini` | `[coverage:run] plugins` |
| `pyproject.toml` | `[tool.coverage.run] plugins` |

## Typing substitutions

Some packages don't ship inline types and use a differently-named stub package. These are substituted automatically (matched case-insensitively; substituted name is exact-case):

| Dependency | Substituted with |
|------|------|
| `cachetools` | `types-cachetools` |
| `croniter` | `types-croniter` |
| `decorator` | `types-decorator` |
| `defusedxml` | `types-defusedxml` |
| `docutils` | `types-docutils` |
| `homeassistant` | `homeassistant-stubs` |
| `html5lib` | `types-html5lib` |
| `httplib2` | `types-httplib2` |
| `jsonschema` | `types-jsonschema` |
| `Markdown` | `types-Markdown` |
| `mock` | `types-mock` |
| `paramiko` | `types-paramiko` |
| `pexpect` | `types-pexpect` |
| `protobuf` | `types-protobuf` |
| `psycopg2` | `types-psycopg2` |
| `pycurl` | `types-pycurl` |
| `PyMySQL` | `types-PyMySQL` |
| `pynput` | `types-pynput` |
| `pyserial` | `types-pyserial` |
| `python-dateutil` | `types-python-dateutil` |
| `pytz` | `types-pytz` |
| `PyYAML` | `types-PyYAML` |
| `regex` | `types-regex` |
| `Send2Trash` | `types-Send2Trash` |
| `setuptools` | `types-setuptools` |
| `simplejson` | `types-simplejson` |
| `six` | `types-six` |
| `tabulate` | `types-tabulate` |
| `toml` | `types-toml` |
| `waitress` | `types-waitress` |
| `xmltodict` | `types-xmltodict` |

## How it works

Hooks matched:

- `id: mypy` in a repo whose URL contains `mirrors-mypy`
- `id: ty` in a repo whose URL contains `mirrors-ty`
- `id: pyright` in a repo whose URL contains `mirrors-pyright`

`additional_dependencies` is rewritten as a sorted block list. Comments and unrelated YAML formatting are preserved.
