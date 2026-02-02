//! Validates that CLI documentation includes TUI entrypoints and keybindings.

const DOCS_CLI: &str = include_str!("../../../docs/cli.md");

fn assert_contains(needle: &str) {
    assert!(
        DOCS_CLI.contains(needle),
        "docs/cli.md missing expected content: {needle:?}"
    );
}

#[test]
fn cli_docs_include_tui_entrypoints() {
    assert_contains("ralph tui");
    assert_contains("ralph run one -i");
    assert_contains("ralph run loop -i");
    assert_contains("--read-only");
}

#[test]
fn cli_docs_include_tui_keybindings() {
    assert_contains("Keybindings");
    assert_contains("Help overlay");
    assert_contains("`?` or `h`");
    assert_contains("`Up`/`Down`");
    assert_contains("Enter");
    assert_contains("`l`: toggle loop mode");
    assert_contains("`a`: archive done/rejected tasks");
    assert_contains("`d`: delete selected task");
    assert_contains("`e`: edit task fields");
    assert_contains("`n`: create a new task");
    assert_contains("`c`: edit project config");
    assert_contains("`g`: scan repository");
    assert_contains("`r`: reload queue from disk");
    assert_contains("`q` (or `Esc`");
    assert_contains("Filters & Search");
    assert_contains("`/`: search tasks");
    assert_contains("`t`: filter by tags");
    assert_contains("`f`: cycle status filter");
    assert_contains("`x`: clear filters");
    assert_contains("Quick Changes");
    assert_contains("`s`: cycle task status");
    assert_contains("`p`: cycle priority");
    assert_contains("Command Palette");
    assert_contains("`:`: open palette");
    assert_contains("Execution View");
    assert_contains("`Esc`: return to task list");
    assert_contains("`PgUp`/`PgDn`");
    assert_contains("`a`: toggle auto-scroll");
    assert_contains("`l`: stop loop mode");
}
