use std::path::Path;

use crate::Error;

pub fn is_typing_hook(repo_url: &str, hook_id: &str) -> bool {
    hook_id == "mypy"
        || hook_id == "ty"
        || repo_url.contains("mirrors-mypy")
        || repo_url.contains("mirrors-ty")
}

pub fn update_config(config_path: &Path, deps: &[String]) -> Result<bool, Error> {
    let content = std::fs::read_to_string(config_path)?;

    // Validate YAML structure before doing anything.
    let doc: serde_yaml::Value = serde_yaml::from_str(&content)?;
    doc.get("repos")
        .and_then(|r| r.as_sequence())
        .ok_or_else(|| Error::Config("missing 'repos' sequence".into()))?;

    let mut sorted_deps = deps.to_vec();
    sorted_deps.sort_unstable();

    let new_content = rewrite_additional_deps(&content, &sorted_deps);

    if new_content != content {
        std::fs::write(config_path, new_content)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Rewrites `additional_dependencies:` blocks for typing hooks using line-level
/// manipulation so that comments and unrelated formatting are preserved.
///
/// Outputs deps as a sorted block-sequence (compact notation, `-` at the same
/// indent as the key).
fn rewrite_additional_deps(content: &str, sorted_deps: &[String]) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let trailing_newline = content.ends_with('\n');

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + sorted_deps.len());
    let mut repo_url = String::new();
    let mut hook_id: Option<String> = None;
    // Indent of direct hook fields (id:, additional_dependencies:, entry:, …).
    let mut hook_field_indent: usize = 0;
    let mut dep_injected = false;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let significant = !trimmed.is_empty() && !trimmed.starts_with('#');

        // A non-blank, non-comment line with indent < hook_field_indent means
        // we have left the current hook's body.
        if significant && indent < hook_field_indent {
            if let Some(ref id) = hook_id {
                if !dep_injected && is_typing_hook(&repo_url, id) {
                    inject_deps(&mut out, hook_field_indent, sorted_deps);
                }
                hook_id = None;
                dep_injected = false;
            }
        }

        if significant {
            // List items (- repo: / - id: / other list items) share the leading '-'.
            if let Some(rest) = trimmed.strip_prefix('-').map(|r| r.trim_start()) {
                if let Some(url) = rest.strip_prefix("repo:") {
                    repo_url = url.trim().trim_matches('"').trim_matches('\'').to_owned();
                    out.push(line.to_owned());
                } else if let Some(id_raw) = rest.strip_prefix("id:") {
                    let id = id_raw
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_owned();
                    hook_id = Some(id);
                    hook_field_indent = indent + 2;
                    dep_injected = false;
                    out.push(line.to_owned());
                } else {
                    out.push(line.to_owned());
                }
            } else if indent == hook_field_indent
                && trimmed.starts_with("additional_dependencies:")
                && hook_id
                    .as_deref()
                    .is_some_and(|id| is_typing_hook(&repo_url, id))
            {
                inject_deps(&mut out, hook_field_indent, sorted_deps);
                dep_injected = true;
                i += 1;
                // Skip the existing dep block: blank lines, comments, lines more
                // indented than the key, and compact-notation list items at the
                // same indent as the key.
                while i < lines.len() {
                    let next = lines[i];
                    let nt = next.trim_start();
                    let ni = next.len() - nt.len();
                    if nt.is_empty()
                        || nt.starts_with('#')
                        || ni > hook_field_indent
                        || (ni == hook_field_indent && nt.starts_with("- "))
                    {
                        i += 1;
                    } else {
                        break;
                    }
                }
                continue;
            } else {
                out.push(line.to_owned());
            }
        } else {
            out.push(line.to_owned());
        }

        i += 1;
    }

    // End-of-file: flush the final hook if it never saw additional_dependencies.
    if let Some(ref id) = hook_id {
        if !dep_injected && is_typing_hook(&repo_url, id) {
            inject_deps(&mut out, hook_field_indent, sorted_deps);
        }
    }

    let mut result = out.join("\n");
    if trailing_newline {
        result.push('\n');
    }
    result
}

fn inject_deps(out: &mut Vec<String>, indent: usize, sorted_deps: &[String]) {
    let prefix = " ".repeat(indent);
    out.push(format!("{prefix}additional_dependencies:"));
    for dep in sorted_deps {
        out.push(format!("{prefix}- {dep}"));
    }
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

    // ── is_typing_hook ────────────────────────────────────────────────────────

    #[rstest]
    #[case("https://github.com/pre-commit/mirrors-mypy", "mypy", true)]
    #[case("https://github.com/pre-commit/mirrors-mypy", "check-something", true)]
    #[case("https://example.com/some-repo", "ty", true)]
    #[case("https://github.com/mxr/mirrors-ty", "check-something", true)]
    #[case("https://github.com/pre-commit/pre-commit-hooks", "check-json", false)]
    fn test_is_typing_hook(#[case] url: &str, #[case] id: &str, #[case] expected: bool) {
        assert_eq!(is_typing_hook(url, id), expected);
    }

    // ── inject_deps ───────────────────────────────────────────────────────────

    #[test]
    fn test_inject_deps_empty() {
        let mut out = Vec::new();
        inject_deps(&mut out, 4, &[]);
        assert_eq!(out, ["    additional_dependencies:"]);
    }

    #[test]
    fn test_inject_deps_with_values() {
        let mut out = Vec::new();
        inject_deps(&mut out, 4, &["b".to_owned(), "a".to_owned()]);
        assert_eq!(out, ["    additional_dependencies:", "    - b", "    - a"]);
    }

    // ── rewrite_additional_deps ───────────────────────────────────────────────

    fn mypy_config(extra: &str) -> String {
        format!(
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n{extra}"
        )
    }

    #[test]
    fn test_rewrite_inserts_when_missing() {
        let input = mypy_config("");
        let out = rewrite_additional_deps(&input, &["a".to_owned(), "b".to_owned()]);
        assert!(out.contains("additional_dependencies:\n    - a\n    - b\n"));
    }

    #[test]
    fn test_rewrite_inserts_via_leaving_hook() {
        // Typing hook with no deps followed by another repo: inject fires at the
        // indent-decrease boundary (not at EOF).
        let input = format!(
            "{}{}",
            mypy_config(""),
            "- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.5.0\n  hooks:\n  - id: trailing-whitespace\n"
        );
        let out = rewrite_additional_deps(&input, &["dep".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - dep\n"));
        assert!(out.contains("id: trailing-whitespace"));
    }

    #[test]
    fn test_rewrite_url_based_hook_matching() {
        // Hook matched by repo URL even when id is not "mypy".
        let input = "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy-custom\n";
        let out = rewrite_additional_deps(input, &["dep".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - dep\n"));
    }

    #[test]
    fn test_rewrite_no_hooks_section() {
        // Repo with no hooks key; hook_id stays None; EOF flush skips safely.
        let input = "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n";
        let out = rewrite_additional_deps(input, &["dep".to_owned()]);
        assert_eq!(out, input);
    }

    #[test]
    fn test_rewrite_sorts_deps() {
        let input = mypy_config("    additional_dependencies:\n    - z\n    - a\n");
        let mut sorted = vec!["a".to_owned(), "z".to_owned()];
        sorted.sort_unstable();
        let out = rewrite_additional_deps(&input, &sorted);
        assert!(out.contains("    - a\n    - z\n"));
    }

    #[test]
    fn test_rewrite_replaces_compact_notation() {
        // Compact notation: deps at same indent as key.
        let input = mypy_config("    additional_dependencies:\n    - old\n");
        let out = rewrite_additional_deps(&input, &["new".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - new\n"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn test_rewrite_replaces_indented_notation() {
        // Standard notation: deps indented 2 more than key.
        let input = mypy_config("    additional_dependencies:\n      - old\n");
        let out = rewrite_additional_deps(&input, &["new".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - new\n"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn test_rewrite_replaces_flow_style() {
        let input = mypy_config("    additional_dependencies: [old-dep]\n");
        let out = rewrite_additional_deps(&input, &["new".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - new\n"));
        assert!(!out.contains("old-dep"));
    }

    #[test]
    fn test_rewrite_empty_inline_list() {
        let input = mypy_config("    additional_dependencies: []\n");
        let out = rewrite_additional_deps(&input, &["dep".to_owned()]);
        assert!(out.contains("    additional_dependencies:\n    - dep\n"));
    }

    #[test]
    fn test_rewrite_preserves_comments() {
        let input = format!(
            "# top comment\nrepos:\n# repo comment\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old\n"
        );
        let out = rewrite_additional_deps(&input, &["new".to_owned()]);
        assert!(out.starts_with("# top comment\n"));
        assert!(out.contains("# repo comment\n"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn test_rewrite_no_typing_hooks() {
        let input = "repos:\n- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.5.0\n  hooks:\n  - id: trailing-whitespace\n";
        let out = rewrite_additional_deps(input, &["dep".to_owned()]);
        assert_eq!(out, input);
    }

    #[test]
    fn test_rewrite_empty_deps_replaces_block() {
        let input = mypy_config("    additional_dependencies:\n    - old\n");
        let out = rewrite_additional_deps(&input, &[]);
        assert!(out.contains("    additional_dependencies:\n"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn test_rewrite_preserves_trailing_newline() {
        let input = mypy_config("");
        assert!(input.ends_with('\n'));
        let out = rewrite_additional_deps(&input, &["dep".to_owned()]);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn test_rewrite_no_trailing_newline() {
        let input = mypy_config("").trim_end_matches('\n').to_owned();
        let out = rewrite_additional_deps(&input, &["dep".to_owned()]);
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn test_rewrite_multiple_repos_only_updates_typing() {
        let input = "repos:\n- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.5.0\n  hooks:\n  - id: trailing-whitespace\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old\n";
        let out = rewrite_additional_deps(input, &["new".to_owned()]);
        assert!(!out.contains("old"));
        assert!(out.contains("    - new"));
        assert!(out.contains("id: trailing-whitespace"));
    }

    #[test]
    fn test_rewrite_multiple_hooks_same_repo() {
        // Only mypy hook updated; non-typing hook in another repo untouched.
        let input = "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old\n- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.0.0\n  hooks:\n  - id: check-yaml\n    additional_dependencies:\n    - should-stay\n";
        let out = rewrite_additional_deps(input, &["new".to_owned()]);
        assert!(!out.contains("old"));
        assert!(out.contains("should-stay"));
    }

    // ── update_config ─────────────────────────────────────────────────────────

    #[test]
    fn test_update_config_updates_ty_hook() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/mxr/mirrors-ty\n  rev: v0.0.1\n  hooks:\n  - id: ty\n",
        );
        let updated = update_config(
            &dir.path().join(".pre-commit-config.yaml"),
            &["mypy>=1.0".to_owned()],
        )
        .unwrap();
        assert!(updated);
    }

    #[test]
    fn test_update_config_updates_mypy_hook() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
        );
        let deps = vec!["mypy>=1.0".to_owned(), "types-requests".to_owned()];
        let updated = update_config(&dir.path().join(".pre-commit-config.yaml"), &deps).unwrap();
        assert!(updated);

        let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
        assert!(content.contains("- mypy>=1.0"));
        assert!(content.contains("- types-requests"));
    }

    #[test]
    fn test_update_config_sorts_deps() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
        );
        let deps = vec!["z-dep".to_owned(), "a-dep".to_owned()];
        update_config(&dir.path().join(".pre-commit-config.yaml"), &deps).unwrap();

        let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
        let a_pos = content.find("a-dep").unwrap();
        let z_pos = content.find("z-dep").unwrap();
        assert!(a_pos < z_pos, "deps should be sorted alphabetically");
    }

    #[test]
    fn test_update_config_no_typing_hooks() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/pre-commit-hooks\n  rev: v4.5.0\n  hooks:\n  - id: trailing-whitespace\n",
        );
        let updated = update_config(
            &dir.path().join(".pre-commit-config.yaml"),
            &["mypy>=1.0".to_owned()],
        )
        .unwrap();
        assert!(!updated);
    }

    #[test]
    fn test_update_config_replaces_existing_deps() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old-dep\n",
        );
        let updated = update_config(
            &dir.path().join(".pre-commit-config.yaml"),
            &["new-dep".to_owned()],
        )
        .unwrap();
        assert!(updated);

        let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
        assert!(content.contains("new-dep"));
        assert!(!content.contains("old-dep"));
    }

    #[test]
    fn test_update_config_idempotent() {
        let dir = TempDir::new().unwrap();
        // Already has the correct sorted deps.
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - a-dep\n    - b-dep\n",
        );
        let updated = update_config(
            &dir.path().join(".pre-commit-config.yaml"),
            &["b-dep".to_owned(), "a-dep".to_owned()],
        )
        .unwrap();
        assert!(!updated, "should be idempotent when content is unchanged");
    }

    #[test]
    fn test_update_config_empty_deps() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n    additional_dependencies:\n    - old-dep\n",
        );
        let updated = update_config(&dir.path().join(".pre-commit-config.yaml"), &[]).unwrap();
        assert!(updated);

        let content = fs::read_to_string(dir.path().join(".pre-commit-config.yaml")).unwrap();
        assert!(!content.contains("old-dep"));
    }

    #[test]
    fn test_update_config_io_error() {
        let result = update_config(Path::new("/nonexistent/.pre-commit-config.yaml"), &[]);
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_update_config_write_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            ".pre-commit-config.yaml",
            "repos:\n- repo: https://github.com/pre-commit/mirrors-mypy\n  rev: v1.0.0\n  hooks:\n  - id: mypy\n",
        );
        let path = dir.path().join(".pre-commit-config.yaml");
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&path, perms).unwrap();
        let result = update_config(&path, &["dep".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_config_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        write(&dir, ".pre-commit-config.yaml", "not: valid: yaml: [[[");
        let result = update_config(&dir.path().join(".pre-commit-config.yaml"), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_config_missing_repos() {
        let dir = TempDir::new().unwrap();
        write(&dir, ".pre-commit-config.yaml", "foo: bar\n");
        let result = update_config(&dir.path().join(".pre-commit-config.yaml"), &[]);
        assert!(result.is_err());
    }
}
