// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

use super::*;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let workspace = root.path().join("project");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    (root, home, workspace)
}

fn server<'a>(scan: &'a ImportScan, id: &str) -> &'a ImportedMcpServer {
    match &scan
        .find(id)
        .unwrap_or_else(|| panic!("{id} in {scan:#?}"))
        .kind
    {
        ImportKind::McpServer { server } => server,
        other => panic!("not a server: {other:?}"),
    }
}

#[test]
fn claude_code_servers_keep_names_of_secrets_but_never_values() {
    let (_root, home, workspace) = fixture();
    write(
        &workspace,
        ".mcp.json",
        r#"{"mcpServers":{
            "GitHub":{"command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_TOKEN":"ghp_SYNTHETICSYNTHETIC0123456789"}},
            "docs":{"type":"http","url":"https://user:pw@docs.example.test/mcp?api_key=SYNTHETIC","headers":{"Authorization":"Bearer SYNTHETIC"}}
        }}"#,
    );
    let scan = scan(Some(&home), &workspace);
    let github = server(&scan, "mcp:claude_code_project:github");
    assert_eq!(github.env_names, ["GITHUB_TOKEN"]);
    assert!(!github.enabled);
    let docs = server(&scan, "mcp:claude_code_project:docs");
    assert_eq!(
        docs.transport,
        ImportedMcpTransport::Http {
            url: "https://docs.example.test/mcp".into()
        }
    );
    assert_eq!(docs.header_names, ["Authorization"]);
    let json = serde_json::to_string(&scan).unwrap();
    assert!(!json.contains("SYNTHETIC"), "{json}");
    assert!(!json.contains("pw@"));
}

#[test]
fn an_argument_that_looks_like_a_key_is_removed() {
    let (_root, home, workspace) = fixture();
    write(
        &home,
        ".claude.json",
        r#"{"mcpServers":{"search":{"command":"search-server","args":["--key","sk-SYNTHETIC0123456789abcdef"]}}}"#,
    );
    let scan = scan(Some(&home), &workspace);
    let search = server(&scan, "mcp:claude_code_user:search");
    match &search.transport {
        ImportedMcpTransport::Stdio { args, .. } => {
            assert_eq!(args[1], REMOVED_SECRET);
        }
        other => panic!("{other:?}"),
    }
    assert!(!serde_json::to_string(&scan)
        .unwrap()
        .contains("SYNTHETIC0123"));
}

#[test]
fn claude_project_servers_in_the_account_file_are_found_for_this_project_only() {
    let (_root, home, workspace) = fixture();
    let here = workspace.canonicalize().unwrap();
    let text = serde_json::json!({
        "projects": {
            here.to_string_lossy(): {"mcpServers": {"local": {"command": "local-server"}}},
            "/elsewhere": {"mcpServers": {"other": {"command": "other-server"}}}
        }
    });
    write(&home, ".claude.json", &text.to_string());
    let scan = scan(Some(&home), &workspace);
    assert!(scan.find("mcp:claude_code_project:local").is_some());
    assert!(scan.items.iter().all(|item| item.name() != "other"));
}

#[test]
fn codex_and_opencode_servers_are_read() {
    let (_root, home, workspace) = fixture();
    write(
        &home,
        ".codex/config.toml",
        "model = \"x\"\n[mcp_servers.files]\ncommand = \"files-server\"\nargs = [\"--root\", \".\"]\n[mcp_servers.files.env]\nFILES_TOKEN = \"SYNTHETIC\"\n[mcp_servers.remote]\nurl = \"https://remote.example.test/mcp\"\nbearer_token_env_var = \"REMOTE_TOKEN\"\n",
    );
    write(
        &workspace,
        "opencode.jsonc",
        "{\n // comment with \"quotes\"\n \"mcp\": {\"browser\": {\"type\": \"local\", \"command\": [\"npx\", \"browser\"], \"environment\": {\"B\": \"SYNTHETIC\"}}}\n}",
    );
    let scan = scan(Some(&home), &workspace);
    let files = server(&scan, "mcp:codex_user:files");
    assert_eq!(files.env_names, ["FILES_TOKEN"]);
    let remote = server(&scan, "mcp:codex_user:remote");
    assert_eq!(remote.header_names, ["Authorization"]);
    let browser = server(&scan, "mcp:opencode_project:browser");
    assert_eq!(
        browser.transport,
        ImportedMcpTransport::Stdio {
            command: "npx".into(),
            args: vec!["browser".into()]
        }
    );
    assert!(!serde_json::to_string(&scan).unwrap().contains("SYNTHETIC"));
}

#[test]
fn a_project_server_wins_a_name_clash() {
    let (_root, home, workspace) = fixture();
    write(
        &workspace,
        ".mcp.json",
        r#"{"mcpServers":{"x":{"command":"a"}}}"#,
    );
    write(
        &home,
        ".claude.json",
        r#"{"mcpServers":{"x":{"command":"b"}}}"#,
    );
    let scan = scan(Some(&home), &workspace);
    assert!(scan.find("mcp:claude_code_project:x").unwrap().importable());
    assert!(!scan.find("mcp:claude_code_user:x").unwrap().importable());
}

#[test]
fn skills_drop_allowed_tools_and_other_files() {
    let (_root, home, workspace) = fixture();
    write(
        &workspace,
        ".claude/skills/Release Review/SKILL.md",
        "---\nname: release-review\ndescription: Check a release\nallowed-tools: Bash(git push:*)\nmodel: opus\n---\nRead ROADMAP.md first.\n",
    );
    write(
        &workspace,
        ".claude/skills/Release Review/run.sh",
        "echo hi",
    );
    let scan = scan(Some(&home), &workspace);
    let item = scan
        .find("skill:claude_code_project:release-review")
        .unwrap();
    assert!(item.dropped.iter().any(|d| d.contains("allowed-tools")));
    assert!(item.dropped.iter().any(|d| d.contains("other files")));
    let ImportKind::Skill { skill } = &item.kind else {
        panic!()
    };
    let written = write_skill(&workspace, skill).unwrap();
    assert_eq!(written, ".agents/skills/release-review/SKILL.md");
    let text = fs::read_to_string(workspace.join(&written)).unwrap();
    assert!(!text.contains("allowed-tools"));
    assert!(!text.contains("git push"));
    assert!(text.contains("Read ROADMAP.md first."));
    assert!(!workspace
        .join(".agents/skills/release-review/run.sh")
        .exists());
    assert!(matches!(
        write_skill(&workspace, skill),
        Err(ImportError::SkillExists(_))
    ));
    // A second scan sees the skill is already there.
    let again = super::scan(Some(&home), &workspace);
    assert!(!again
        .find("skill:claude_code_project:release-review")
        .unwrap()
        .importable());
}

#[test]
fn commands_become_skills_with_folder_names() {
    let (_root, home, workspace) = fixture();
    write(
        &workspace,
        ".claude/commands/frontend/Fix Lint.md",
        "---\ndescription: Fix lint errors\nargument-hint: file\n---\nFix the lint errors in $ARGUMENTS.\n",
    );
    write(
        &home,
        ".codex/prompts/review.md",
        "# Review the diff\n\nLook at the diff.\n",
    );
    let scan = scan(Some(&home), &workspace);
    let item = scan
        .find("command:claude_code_project:frontend-fix-lint")
        .unwrap();
    let ImportKind::Command { skill } = &item.kind else {
        panic!()
    };
    assert_eq!(skill.description, "Fix lint errors");
    assert!(skill.body.contains("$ARGUMENTS"));
    let review = scan.find("command:codex_user:review").unwrap();
    let ImportKind::Command { skill } = &review.kind else {
        panic!()
    };
    assert_eq!(skill.description, "Review the diff");
    render_skill(skill).unwrap();
}

#[test]
fn a_linked_skill_folder_is_refused() {
    let (root, _home, workspace) = fixture();
    let elsewhere = root.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(workspace.join(".agents")).unwrap();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&elsewhere, workspace.join(".agents/skills")).is_ok();
    #[cfg(windows)]
    let linked =
        std::os::windows::fs::symlink_dir(&elsewhere, workspace.join(".agents/skills")).is_ok();
    if !linked {
        // Creating links needs developer mode on Windows.
        return;
    }
    let skill = SkillDraft {
        name: "x".into(),
        description: "x".into(),
        license: None,
        compatibility: None,
        metadata: BTreeMap::new(),
        body: "x".into(),
    };
    assert!(matches!(
        write_skill(&workspace, &skill),
        Err(ImportError::SkillFolderUnsafe)
    ));
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[test]
fn claude_instructions_are_listed_but_not_importable() {
    let (_root, home, workspace) = fixture();
    write(&workspace, "CLAUDE.md", "rules");
    let scan = scan(Some(&home), &workspace);
    let item = scan
        .find("instructions:claude_code_project:CLAUDE.md")
        .unwrap();
    assert!(!item.importable());
}

#[test]
fn broken_files_are_reported_not_fatal() {
    let (_root, home, workspace) = fixture();
    write(&workspace, ".mcp.json", "{not json");
    write(&home, ".codex/config.toml", "[[[");
    let scan = scan(Some(&home), &workspace);
    assert!(scan.items.is_empty());
    assert_eq!(scan.problems.len(), 2, "{:?}", scan.problems);
}

#[test]
fn names_are_normalized() {
    assert_eq!(normalize_name("My Server!"), "my-server");
    assert_eq!(normalize_name("--x--"), "x");
    assert_eq!(normalize_name("..."), "");
}
