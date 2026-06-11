use std::collections::HashMap;
use std::path::Path;

use crate::Error;

pub fn find_deps(cwd: &Path) -> Result<Vec<String>, Error> {
    let mut deps = Vec::new();
    let setup_cfg = cwd.join("setup.cfg");
    if setup_cfg.exists() {
        deps.extend(parse_setup_cfg(&setup_cfg)?);
    }
    let pyproject_toml = cwd.join("pyproject.toml");
    if pyproject_toml.exists() {
        deps.extend(parse_pyproject_toml(&pyproject_toml)?);
    }
    Ok(deps)
}

fn parse_setup_cfg(path: &Path) -> Result<Vec<String>, Error> {
    let content = std::fs::read_to_string(path)?;
    let sections = parse_ini(&content);

    let mut deps = Vec::new();

    if let Some(options) = sections.get("options") {
        if let Some(val) = options.get("install_requires") {
            deps.extend(nonempty_lines(val));
        }
    }

    if let Some(extras) = sections.get("options.extras_require") {
        for val in extras.values() {
            deps.extend(nonempty_lines(val));
        }
    }

    Ok(deps)
}

// Parses INI content with Python-style multiline values (continuation lines start with whitespace).
fn parse_ini(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = String::new();
    let mut key: Option<String> = None;

    for line in content.lines() {
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                section = line[1..end].trim().to_lowercase();
                key = None;
            }
        } else if line.starts_with(|c: char| c.is_whitespace()) {
            // Continuation line: appended to the current key's value.
            if let Some(ref k) = key {
                sections
                    .entry(section.clone())
                    .or_default()
                    .entry(k.clone())
                    .and_modify(|v| {
                        v.push('\n');
                        v.push_str(line.trim());
                    });
            }
        } else if let Some(eq) = line.find('=') {
            let k = line[..eq].trim().to_lowercase();
            let v = line[eq + 1..].trim().to_owned();
            sections
                .entry(section.clone())
                .or_default()
                .insert(k.clone(), v);
            key = Some(k);
        }
    }

    sections
}

fn parse_pyproject_toml(path: &Path) -> Result<Vec<String>, Error> {
    let content = std::fs::read_to_string(path)?;
    let data: toml::Value = toml::from_str(&content)?;
    let mut deps = Vec::new();

    // PEP 735 dependency-groups.dev
    for item in data
        .get("dependency-groups")
        .and_then(|v| v.get("dev"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(s) = item.as_str() {
            deps.push(s.to_owned());
        }
    }

    // project.optional-dependencies.dev
    for item in data
        .get("project")
        .and_then(|v| v.get("optional-dependencies"))
        .and_then(|v| v.get("dev"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(s) = item.as_str() {
            deps.push(s.to_owned());
        }
    }

    // tool.poetry.dev-dependencies (old style)
    if let Some(table) = data
        .get("tool")
        .and_then(|v| v.get("poetry"))
        .and_then(|v| v.get("dev-dependencies"))
        .and_then(|v| v.as_table())
    {
        deps.extend(poetry_deps(table));
    }

    // tool.poetry.group.dev.dependencies (new style)
    if let Some(table) = data
        .get("tool")
        .and_then(|v| v.get("poetry"))
        .and_then(|v| v.get("group"))
        .and_then(|v| v.get("dev"))
        .and_then(|v| v.get("dependencies"))
        .and_then(|v| v.as_table())
    {
        deps.extend(poetry_deps(table));
    }

    Ok(deps)
}

fn poetry_deps(table: &toml::value::Table) -> Vec<String> {
    table
        .iter()
        .filter(|(pkg, _)| pkg.to_lowercase() != "python")
        .map(|(pkg, ver)| match ver.as_str() {
            Some("*") | None => pkg.clone(),
            Some(v) => format!("{pkg}{v}"),
        })
        .collect()
}

fn nonempty_lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write(dir: &TempDir, name: &str, content: &str) {
        fs::write(dir.path().join(name), content).unwrap();
    }

    #[test]
    fn test_parse_ini_section_and_key() {
        let sections = parse_ini("[s]\nkey = val\n");
        assert_eq!(sections["s"]["key"], "val");
    }

    #[test]
    fn test_parse_ini_continuation_line() {
        let sections = parse_ini("[s]\nkey =\n    a\n    b\n");
        assert_eq!(sections["s"]["key"], "\na\nb");
    }

    #[test]
    fn test_parse_ini_orphan_continuation() {
        // Continuation line before any key in the section is silently ignored.
        let sections = parse_ini("[s]\n    orphan\nk = v\n");
        assert_eq!(sections["s"]["k"], "v");
        assert_eq!(sections["s"].len(), 1);
    }

    #[test]
    fn test_parse_ini_unclosed_section_header() {
        // Section header with '[' but no ']' is silently ignored; following keys
        // land in the previous (empty) section.
        let sections = parse_ini("[unclosed\nk = v\n");
        assert!(!sections.contains_key("unclosed"));
        assert_eq!(sections[""]["k"], "v");
    }

    #[test]
    fn test_parse_ini_line_without_equals() {
        // A line that is not a section header, not a continuation, and has no '='
        // is silently ignored.
        let sections = parse_ini("[s]\n# comment\nk = v\n");
        assert_eq!(sections["s"]["k"], "v");
        assert_eq!(sections["s"].len(), 1);
    }

    #[test]
    fn test_parse_ini_lowercases_section_and_key() {
        let sections = parse_ini("[MySection]\nMyKey = val\n");
        assert_eq!(sections["mysection"]["mykey"], "val");
    }

    #[test]
    fn test_parse_ini_no_sections() {
        // Keys before any section header land in the "" bucket; named sections absent.
        let sections = parse_ini("key = val\n");
        assert!(!sections.contains_key("mysection"));
    }

    #[test]
    fn test_nonempty_lines_empty() {
        assert_eq!(nonempty_lines(""), Vec::<String>::new());
    }

    #[test]
    fn test_nonempty_lines_whitespace_only() {
        assert_eq!(nonempty_lines("  \n  \n"), Vec::<String>::new());
    }

    #[test]
    fn test_nonempty_lines_trims_and_filters() {
        assert_eq!(nonempty_lines("a\n  b  \n\nc"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_setup_cfg_install_requires() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    requests>=2.0\n    click\n",
        );
        let deps = parse_setup_cfg(&dir.path().join("setup.cfg")).unwrap();
        assert_eq!(deps, ["requests>=2.0", "click"]);
    }

    #[test]
    fn test_parse_setup_cfg_extras_require() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options.extras_require]\ndev =\n    mypy\n    pytest\n",
        );
        let deps = parse_setup_cfg(&dir.path().join("setup.cfg")).unwrap();
        assert_eq!(deps, ["mypy", "pytest"]);
    }

    #[test]
    fn test_parse_setup_cfg_empty() {
        let dir = TempDir::new().unwrap();
        write(&dir, "setup.cfg", "[metadata]\nname = foo\n");
        let deps = parse_setup_cfg(&dir.path().join("setup.cfg")).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_setup_cfg_options_without_install_requires() {
        // [options] section exists but no install_requires key → empty deps.
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\nsetup_requires = setuptools\n",
        );
        let deps = parse_setup_cfg(&dir.path().join("setup.cfg")).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_setup_cfg_io_error() {
        let result = parse_setup_cfg(Path::new("/nonexistent/setup.cfg"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_pyproject_dependency_groups() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy>=1.0\", \"pytest\"]\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["mypy>=1.0", "pytest"]);
    }

    #[test]
    fn test_parse_pyproject_dependency_groups_non_string_item() {
        // PEP 735 allows {include-group = "..."} table items; non-strings are skipped.
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy\", {include-group = \"typing\"}]\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_parse_pyproject_optional_deps() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[project.optional-dependencies]\ndev = [\"mypy\", \"types-requests\"]\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["mypy", "types-requests"]);
    }

    #[test]
    fn test_parse_pyproject_optional_deps_non_string_item() {
        // Non-string items in the array are skipped.
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[project.optional-dependencies]\ndev = [\"mypy\", 42]\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_parse_pyproject_poetry_old_style() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[tool.poetry.dev-dependencies]\nmypy = \">=1.0\"\npytest = \"*\"\npython = \"^3.9\"\n",
        );
        let mut deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        deps.sort();
        assert_eq!(deps, ["mypy>=1.0", "pytest"]);
    }

    #[test]
    fn test_parse_pyproject_poetry_new_style() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[tool.poetry.group.dev.dependencies]\nmypy = \">=1.0\"\npytest = \"*\"\n",
        );
        let mut deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        deps.sort();
        assert_eq!(deps, ["mypy>=1.0", "pytest"]);
    }

    #[test]
    fn test_parse_pyproject_poetry_inline_table_version() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[tool.poetry.dev-dependencies]\nmypy = {version = \">=1.0\", extras = [\"extra\"]}\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_parse_pyproject_empty() {
        let dir = TempDir::new().unwrap();
        write(&dir, "pyproject.toml", "[tool]\n");
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_pyproject_toml_io_error() {
        let result = parse_pyproject_toml(Path::new("/nonexistent/pyproject.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_pyproject_toml_invalid() {
        let dir = TempDir::new().unwrap();
        write(&dir, "pyproject.toml", "not = valid = toml");
        let result = parse_pyproject_toml(&dir.path().join("pyproject.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_find_deps_no_files() {
        let dir = TempDir::new().unwrap();
        let deps = find_deps(dir.path()).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_find_deps_setup_cfg_unreadable() {
        // setup.cfg exists as a directory → read_to_string fails → ? propagates.
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("setup.cfg")).unwrap();
        assert!(find_deps(dir.path()).is_err());
    }

    #[test]
    fn test_find_deps_pyproject_toml_unreadable() {
        // pyproject.toml exists as a directory → read_to_string fails → ? propagates.
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("pyproject.toml")).unwrap();
        assert!(find_deps(dir.path()).is_err());
    }

    #[test]
    fn test_find_deps_setup_cfg_only() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    requests\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["requests"]);
    }

    #[test]
    fn test_find_deps_pyproject_only() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy\"]\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_find_deps_both_files() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    requests\n",
        );
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy\"]\n",
        );
        let mut deps = find_deps(dir.path()).unwrap();
        deps.sort();
        assert_eq!(deps, ["mypy", "requests"]);
    }
}
