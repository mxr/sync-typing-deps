use std::collections::HashMap;
use std::path::Path;

use crate::Error;

const TYPING_SUBSTITUTIONS: &[(&str, &str)] = &[("homeassistant", "homeassistant-stubs")];

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
    let custom_components = cwd.join("custom_components");
    if custom_components.is_dir() {
        for entry in std::fs::read_dir(&custom_components)?.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let manifest = entry.path().join("manifest.json");
                if manifest.exists() {
                    deps.extend(parse_manifest_json(&manifest)?);
                }
            }
        }
    }

    for dep in &mut deps {
        let name = normalize_pkg_name(dep_name(dep));
        for &(from, to) in TYPING_SUBSTITUTIONS {
            if name == from {
                *dep = to.to_owned();
                break;
            }
        }
    }

    let plugins = find_coverage_plugins(cwd)?;
    if !plugins.is_empty() {
        deps.retain(|dep| {
            let name = normalize_pkg_name(dep_name(dep));
            !plugins.contains(&name)
        });
    }

    Ok(deps)
}

fn dep_name(dep: &str) -> &str {
    dep.find(['>', '<', '=', '!', '~', '^', ',', ';', ' ', '['])
        .map_or(dep, |i| &dep[..i])
}

fn normalize_pkg_name(name: &str) -> String {
    name.to_lowercase().replace(['-', '_', '.'], "-")
}

fn find_coverage_plugins(cwd: &Path) -> Result<std::collections::HashSet<String>, Error> {
    let mut plugins = std::collections::HashSet::new();

    for (name, section) in [
        (".coveragerc", "run"),
        ("setup.cfg", "coverage:run"),
        ("tox.ini", "coverage:run"),
    ] {
        let path = cwd.join(name);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let sections = parse_ini(&content);
            if let Some(val) = sections.get(section).and_then(|s| s.get("plugins")) {
                plugins.extend(
                    nonempty_lines(val)
                        .into_iter()
                        .map(|p| normalize_pkg_name(&p)),
                );
            }
        }
    }

    let pyproject_toml = cwd.join("pyproject.toml");
    if pyproject_toml.exists() {
        let content = std::fs::read_to_string(&pyproject_toml)?;
        let data: toml::Value = toml::from_str(&content)?;
        for item in data
            .get("tool")
            .and_then(|v| v.get("coverage"))
            .and_then(|v| v.get("run"))
            .and_then(|v| v.get("plugins"))
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(s) = item.as_str() {
                plugins.insert(normalize_pkg_name(s));
            }
        }
    }

    Ok(plugins)
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

    // project.dependencies (PEP 621 runtime deps)
    for item in data
        .get("project")
        .and_then(|v| v.get("dependencies"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(s) = item.as_str() {
            deps.push(s.to_owned());
        }
    }

    // PEP 735 dependency-groups (dev and test)
    if let Some(groups) = data.get("dependency-groups").and_then(|v| v.as_table()) {
        for key in ["dev", "test"] {
            for item in groups
                .get(key)
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(s) = item.as_str() {
                    deps.push(s.to_owned());
                }
            }
        }
    }

    // project.optional-dependencies (dev and test)
    if let Some(extras) = data
        .get("project")
        .and_then(|v| v.get("optional-dependencies"))
        .and_then(|v| v.as_table())
    {
        for key in ["dev", "test"] {
            for item in extras
                .get(key)
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(s) = item.as_str() {
                    deps.push(s.to_owned());
                }
            }
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

    // build-system.requires (PEP 517)
    for item in data
        .get("build-system")
        .and_then(|v| v.get("requires"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(s) = item.as_str() {
            deps.push(s.to_owned());
        }
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

fn parse_manifest_json(path: &Path) -> Result<Vec<String>, Error> {
    let content = std::fs::read_to_string(path)?;
    let data: serde_json::Value = serde_json::from_str(&content)?;
    let mut deps = Vec::new();
    for item in data
        .get("requirements")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(s) = item.as_str() {
            deps.push(s.to_owned());
        }
    }
    Ok(deps)
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

    use rstest::rstest;
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

    #[rstest]
    #[case("", Vec::<String>::new())]
    #[case("  \n  \n", Vec::<String>::new())]
    #[case("a\n  b  \n\nc", vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])]
    fn test_nonempty_lines(#[case] input: &str, #[case] expected: Vec<String>) {
        assert_eq!(nonempty_lines(input), expected);
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
    fn test_parse_pyproject_project_dependencies() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[project]\ndependencies = [\"aiohttp>=3\", \"requests\"]\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["aiohttp>=3", "requests"]);
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
    fn test_parse_pyproject_dependency_groups_multiple() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy\"]\ntest = [\"pytest>7\", \"coverage\"]\ndocs = [\"sphinx\"]\n",
        );
        let mut deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        deps.sort();
        assert_eq!(deps, ["coverage", "mypy", "pytest>7"]);
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
    fn test_parse_pyproject_optional_deps_multiple() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[project.optional-dependencies]\ndev = [\"mypy\"]\ntest = [\"pytest\", \"coverage\"]\ndocs = [\"sphinx\"]\n",
        );
        let mut deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        deps.sort();
        assert_eq!(deps, ["coverage", "mypy", "pytest"]);
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
    fn test_parse_pyproject_build_system_requires() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[build-system]\nrequires = [\"setuptools\", \"wheel\"]\nbuild-backend = \"setuptools.build_meta\"\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert_eq!(deps, ["setuptools", "wheel"]);
    }

    #[test]
    fn test_parse_pyproject_build_system_no_requires() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[build-system]\nbuild-backend = \"setuptools.build_meta\"\n",
        );
        let deps = parse_pyproject_toml(&dir.path().join("pyproject.toml")).unwrap();
        assert!(deps.is_empty());
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

    #[rstest]
    #[case("setup.cfg")]
    #[case("pyproject.toml")]
    fn test_find_deps_unreadable_file(#[case] filename: &str) {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(filename)).unwrap();
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

    #[rstest::rstest]
    #[case("covdefaults", "covdefaults")]
    #[case("mypy>=1.0", "mypy")]
    #[case("mypy[extra]", "mypy")]
    fn test_dep_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(dep_name(input), expected);
    }

    #[rstest]
    #[case("Cov_Defaults", "cov-defaults")]
    #[case("cov.defaults", "cov-defaults")]
    fn test_normalize_pkg_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(normalize_pkg_name(input), expected);
    }

    #[test]
    fn test_find_deps_coveragerc_no_plugins_key() {
        let dir = TempDir::new().unwrap();
        write(&dir, ".coveragerc", "[run]\nomit = tests/*\n");
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[rstest::rstest]
    #[case(".coveragerc", "[run]\nplugins = covdefaults\n")]
    #[case("tox.ini", "[coverage:run]\nplugins = covdefaults\n")]
    fn test_find_deps_excludes_coverage_plugin_separate_file(
        #[case] config_file: &str,
        #[case] config_content: &str,
    ) {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n    covdefaults\n",
        );
        write(&dir, config_file, config_content);
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_find_deps_excludes_coverage_plugin_setup_cfg() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n    covdefaults\n\n[coverage:run]\nplugins = covdefaults\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_find_deps_excludes_coverage_plugin_pyproject() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pyproject.toml",
            "[dependency-groups]\ndev = [\"mypy\", \"covdefaults\"]\n\n[tool.coverage.run]\nplugins = [\"covdefaults\"]\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_find_deps_excludes_coverage_plugin_multiline() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".coveragerc",
            "[run]\nplugins =\n    covdefaults\n    other-plugin\n",
        );
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n    covdefaults\n    other-plugin\n    requests\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy", "requests"]);
    }

    #[test]
    fn test_find_deps_excludes_plugin_with_version_specifier() {
        let dir = TempDir::new().unwrap();
        write(&dir, ".coveragerc", "[run]\nplugins = covdefaults\n");
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n    covdefaults>=1.0\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_find_deps_excludes_plugin_name_normalization() {
        // Plugin declared as "cov_defaults", dep listed as "cov-defaults" — same after normalize.
        let dir = TempDir::new().unwrap();
        write(&dir, ".coveragerc", "[run]\nplugins = cov_defaults\n");
        write(
            &dir,
            "setup.cfg",
            "[options]\ninstall_requires =\n    mypy\n    cov-defaults\n",
        );
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["mypy"]);
    }

    #[test]
    fn test_parse_manifest_json_requirements() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("custom_components/my_component")).unwrap();
        fs::write(
            dir.path()
                .join("custom_components/my_component/manifest.json"),
            r#"{"domain":"my_component","requirements":["aiosqlite~=0.21.0","requests>=2.0"]}"#,
        )
        .unwrap();
        let deps = find_deps(dir.path()).unwrap();
        assert_eq!(deps, ["aiosqlite~=0.21.0", "requests>=2.0"]);
    }

    #[test]
    fn test_parse_manifest_json_no_requirements() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("custom_components/my_component")).unwrap();
        fs::write(
            dir.path()
                .join("custom_components/my_component/manifest.json"),
            r#"{"domain":"my_component","version":"1.0.0"}"#,
        )
        .unwrap();
        let deps = find_deps(dir.path()).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_manifest_json_invalid() {
        let path = Path::new("/nonexistent/manifest.json");
        assert!(parse_manifest_json(path).is_err());
    }

    #[test]
    fn test_find_deps_custom_components_subdir_no_manifest() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("custom_components/no_manifest")).unwrap();
        let deps = find_deps(dir.path()).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_find_deps_custom_components_file_not_dir() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("custom_components")).unwrap();
        fs::write(dir.path().join("custom_components/not_a_dir"), "").unwrap();
        let deps = find_deps(dir.path()).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_find_deps_homeassistant_replaced_with_stubs() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("custom_components/my_component")).unwrap();
        fs::write(
            dir.path()
                .join("custom_components/my_component/manifest.json"),
            r#"{"domain":"my_component","requirements":["homeassistant>=2024.1","aiohttp>=3"]}"#,
        )
        .unwrap();
        let mut deps = find_deps(dir.path()).unwrap();
        deps.sort();
        assert_eq!(deps, ["aiohttp>=3", "homeassistant-stubs"]);
    }

    #[test]
    fn test_find_deps_manifest_json_multiple_components() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("custom_components/comp_a")).unwrap();
        fs::create_dir_all(dir.path().join("custom_components/comp_b")).unwrap();
        fs::write(
            dir.path().join("custom_components/comp_a/manifest.json"),
            r#"{"domain":"comp_a","requirements":["aiohttp>=3"]}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("custom_components/comp_b/manifest.json"),
            r#"{"domain":"comp_b","requirements":["requests"]}"#,
        )
        .unwrap();
        let mut deps = find_deps(dir.path()).unwrap();
        deps.sort();
        assert_eq!(deps, ["aiohttp>=3", "requests"]);
    }
}
