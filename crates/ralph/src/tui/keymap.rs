//! Canonical TUI keymap definitions shared by footer, help overlay, and docs sync tests.
//!
//! Responsibilities:
//! - Define the authoritative keybindings for Normal, Executing, and Help modes.
//! - Provide structured access to keybinding sections for rendering and tests.
//!
//! Not handled here:
//! - Event handling or TUI state mutations.
//! - Rendering styling beyond lightweight footer hint metadata.
//!
//! Invariants/assumptions:
//! - Key labels here must stay aligned with `tui/events` handlers.
//! - `keys_display` strings are the canonical human-readable strings for help/docs.

use super::AppMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FooterHint {
    pub(crate) keys: &'static str,
    pub(crate) label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    pub(crate) keys: &'static [&'static str],
    pub(crate) keys_display: &'static str,
    pub(crate) description: &'static str,
    pub(crate) footer_hint: Option<FooterHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeymapSection {
    pub(crate) title: &'static str,
    pub(crate) bindings: &'static [KeyBinding],
}

const HELP_CLOSE_KEYS: &[&str] = &["Esc", "?/h"];

const HELP_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: HELP_CLOSE_KEYS,
        keys_display: "Esc or ?/h",
        description: "close help overlay",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["Up", "Down", "j", "k"],
        keys_display: "Up/Down or j/k",
        description: "scroll help",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["PgUp/PgDn"],
        keys_display: "PgUp/PgDn",
        description: "page help",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["Home", "End", "g", "G"],
        keys_display: "Home/End or g/G",
        description: "jump to top/bottom",
        footer_hint: None,
    },
];

const NORMAL_NAV_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["Up", "Down", "j", "k"],
        keys_display: "Up/Down or j/k",
        description: "move selection/scroll details (focused panel)",
        footer_hint: Some(FooterHint {
            keys: "↑↓",
            label: "nav",
        }),
    },
    KeyBinding {
        keys: &["PgUp/PgDn"],
        keys_display: "PgUp/PgDn",
        description: "page list/details (focused panel)",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["Home/End"],
        keys_display: "Home/End",
        description: "jump to top/bottom (focused panel)",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["Tab", "Shift+Tab"],
        keys_display: "Tab/Shift+Tab",
        description: "switch focus between list/details",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["K", "J"],
        keys_display: "K/J",
        description: "move selected task up/down",
        footer_hint: Some(FooterHint {
            keys: "K/J",
            label: "reorder",
        }),
    },
    KeyBinding {
        keys: &["Enter"],
        keys_display: "Enter",
        description: "run selected task",
        footer_hint: Some(FooterHint {
            keys: "Enter",
            label: "run",
        }),
    },
    KeyBinding {
        keys: &["G"],
        keys_display: "G",
        description: "jump to task by ID",
        footer_hint: Some(FooterHint {
            keys: "G",
            label: "jump",
        }),
    },
];

const NORMAL_ACTION_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["?", "h"],
        keys_display: "?/h",
        description: "open help overlay",
        footer_hint: Some(FooterHint {
            keys: "?/h",
            label: "help",
        }),
    },
    KeyBinding {
        keys: &["l"],
        keys_display: "l",
        description: "switch to list view",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["b"],
        keys_display: "b",
        description: "switch to board view",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["L"],
        keys_display: "L",
        description: "toggle loop mode",
        footer_hint: Some(FooterHint {
            keys: "L",
            label: "loop",
        }),
    },
    KeyBinding {
        keys: &["a"],
        keys_display: "a",
        description: "archive done/rejected tasks",
        footer_hint: Some(FooterHint {
            keys: "a",
            label: "archive",
        }),
    },
    KeyBinding {
        keys: &["d"],
        keys_display: "d",
        description: "delete selected task",
        footer_hint: Some(FooterHint {
            keys: "d",
            label: "del",
        }),
    },
    KeyBinding {
        keys: &["e"],
        keys_display: "e",
        description: "edit task fields",
        footer_hint: Some(FooterHint {
            keys: "e",
            label: "edit",
        }),
    },
    KeyBinding {
        keys: &["n"],
        keys_display: "n",
        description: "create a new task (title only)",
        footer_hint: Some(FooterHint {
            keys: "n",
            label: "new",
        }),
    },
    KeyBinding {
        keys: &["N"],
        keys_display: "N",
        description: "build task with agent (full structure)",
        footer_hint: None,
    },
    KeyBinding {
        keys: &["c"],
        keys_display: "c",
        description: "edit project config",
        footer_hint: Some(FooterHint {
            keys: "c",
            label: "config",
        }),
    },
    KeyBinding {
        keys: &["g"],
        keys_display: "g",
        description: "scan repository",
        footer_hint: Some(FooterHint {
            keys: "g",
            label: "scan",
        }),
    },
    KeyBinding {
        keys: &["v"],
        keys_display: "v",
        description: "view dependency graph",
        footer_hint: Some(FooterHint {
            keys: "v",
            label: "graph",
        }),
    },
    KeyBinding {
        keys: &["r"],
        keys_display: "r",
        description: "reload queue from disk",
        footer_hint: Some(FooterHint {
            keys: "r",
            label: "refresh",
        }),
    },
    KeyBinding {
        keys: &["q", "Esc"],
        keys_display: "q/Esc",
        description: "quit (may prompt if runner active)",
        footer_hint: Some(FooterHint {
            keys: "q",
            label: "quit",
        }),
    },
    KeyBinding {
        keys: &["Ctrl+C", "Ctrl+Q"],
        keys_display: "Ctrl+C/Ctrl+Q",
        description: "quit (same as q/Esc)",
        footer_hint: None,
    },
];

const NORMAL_FILTER_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["/"],
        keys_display: "/",
        description: "search tasks",
        footer_hint: Some(FooterHint {
            keys: "/",
            label: "search",
        }),
    },
    KeyBinding {
        keys: &["Ctrl+F"],
        keys_display: "Ctrl+F",
        description: "search tasks (shortcut)",
        footer_hint: Some(FooterHint {
            keys: "Ctrl+F",
            label: "search",
        }),
    },
    KeyBinding {
        keys: &["t"],
        keys_display: "t",
        description: "filter by tags",
        footer_hint: Some(FooterHint {
            keys: "t",
            label: "tags",
        }),
    },
    KeyBinding {
        keys: &["o"],
        keys_display: "o",
        description: "filter by scope",
        footer_hint: Some(FooterHint {
            keys: "o",
            label: "scope",
        }),
    },
    KeyBinding {
        keys: &["f"],
        keys_display: "f",
        description: "cycle status filter",
        footer_hint: Some(FooterHint {
            keys: "f",
            label: "filter",
        }),
    },
    KeyBinding {
        keys: &["x"],
        keys_display: "x",
        description: "clear filters",
        footer_hint: Some(FooterHint {
            keys: "x",
            label: "clear",
        }),
    },
    KeyBinding {
        keys: &["C"],
        keys_display: "C",
        description: "toggle case-sensitive search",
        footer_hint: Some(FooterHint {
            keys: "C",
            label: "case",
        }),
    },
    KeyBinding {
        keys: &["R"],
        keys_display: "R",
        description: "toggle regex search",
        footer_hint: Some(FooterHint {
            keys: "R",
            label: "regex",
        }),
    },
];

const NORMAL_QUICK_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["s"],
        keys_display: "s",
        description: "cycle task status",
        footer_hint: Some(FooterHint {
            keys: "s",
            label: "cycle",
        }),
    },
    KeyBinding {
        keys: &["p"],
        keys_display: "p",
        description: "cycle priority",
        footer_hint: Some(FooterHint {
            keys: "p",
            label: "priority",
        }),
    },
];

const COMMAND_PALETTE_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &[":"],
        keys_display: ":",
        description: "open command palette (type to filter, Enter to run, Esc to cancel)",
        footer_hint: Some(FooterHint {
            keys: ":",
            label: "cmd",
        }),
    },
    KeyBinding {
        keys: &["Ctrl+P"],
        keys_display: "Ctrl+P",
        description: "command palette (shortcut)",
        footer_hint: Some(FooterHint {
            keys: "Ctrl+P",
            label: "palette",
        }),
    },
];

const NORMAL_SECTIONS: &[KeymapSection] = &[
    KeymapSection {
        title: "Navigation",
        bindings: NORMAL_NAV_BINDINGS,
    },
    KeymapSection {
        title: "Actions",
        bindings: NORMAL_ACTION_BINDINGS,
    },
    KeymapSection {
        title: "Command Palette",
        bindings: COMMAND_PALETTE_BINDINGS,
    },
    KeymapSection {
        title: "Filters & Search",
        bindings: NORMAL_FILTER_BINDINGS,
    },
    KeymapSection {
        title: "Quick Changes",
        bindings: NORMAL_QUICK_BINDINGS,
    },
];

const EXECUTING_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["Esc"],
        keys_display: "Esc",
        description: "return to task list",
        footer_hint: Some(FooterHint {
            keys: "Esc",
            label: "return",
        }),
    },
    KeyBinding {
        keys: &["Up", "Down", "j", "k"],
        keys_display: "Up/Down or j/k",
        description: "scroll logs",
        footer_hint: Some(FooterHint {
            keys: "↑↓",
            label: "scroll",
        }),
    },
    KeyBinding {
        keys: &["PgUp/PgDn"],
        keys_display: "PgUp/PgDn",
        description: "page logs",
        footer_hint: Some(FooterHint {
            keys: "PgUp/PgDn",
            label: "page",
        }),
    },
    KeyBinding {
        keys: &["a"],
        keys_display: "a",
        description: "toggle auto-scroll",
        footer_hint: Some(FooterHint {
            keys: "a",
            label: "autoscroll",
        }),
    },
    KeyBinding {
        keys: &["L"],
        keys_display: "L",
        description: "stop loop mode",
        footer_hint: Some(FooterHint {
            keys: "L",
            label: "stop loop",
        }),
    },
    KeyBinding {
        keys: &["f"],
        keys_display: "f",
        description: "toggle flowchart overlay",
        footer_hint: Some(FooterHint {
            keys: "f",
            label: "flowchart",
        }),
    },
];

const EXECUTING_SECTIONS: &[KeymapSection] = &[KeymapSection {
    title: "Execution View",
    bindings: EXECUTING_BINDINGS,
}];

const BOARD_NAV_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["Left", "Right"],
        keys_display: "Left/Right",
        description: "move between columns (board view)",
        footer_hint: Some(FooterHint {
            keys: "←→",
            label: "column",
        }),
    },
    KeyBinding {
        keys: &["Up", "Down", "j", "k"],
        keys_display: "Up/Down or j/k",
        description: "navigate tasks in column (board view)",
        footer_hint: Some(FooterHint {
            keys: "↑↓",
            label: "task",
        }),
    },
];

const BOARD_SECTIONS: &[KeymapSection] = &[KeymapSection {
    title: "Board Navigation",
    bindings: BOARD_NAV_BINDINGS,
}];

const HELP_SECTIONS: &[KeymapSection] = &[KeymapSection {
    title: "Help Overlay",
    bindings: HELP_BINDINGS,
}];

const MULTI_SELECT_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["m"],
        keys_display: "m",
        description: "toggle multi-select mode",
        footer_hint: Some(FooterHint {
            keys: "m",
            label: "multi-sel",
        }),
    },
    KeyBinding {
        keys: &["Space"],
        keys_display: "Space",
        description: "toggle selection of current task (in multi-select mode)",
        footer_hint: Some(FooterHint {
            keys: "Space",
            label: "toggle",
        }),
    },
    KeyBinding {
        keys: &["d"],
        keys_display: "d",
        description: "batch delete selected tasks (in multi-select mode)",
        footer_hint: Some(FooterHint {
            keys: "d",
            label: "batch-del",
        }),
    },
    KeyBinding {
        keys: &["a"],
        keys_display: "a",
        description: "batch archive selected tasks (in multi-select mode)",
        footer_hint: Some(FooterHint {
            keys: "a",
            label: "batch-arch",
        }),
    },
    KeyBinding {
        keys: &["Esc"],
        keys_display: "Esc",
        description: "clear selection and exit multi-select mode",
        footer_hint: Some(FooterHint {
            keys: "Esc",
            label: "clear",
        }),
    },
];

const MULTI_SELECT_SECTIONS: &[KeymapSection] = &[KeymapSection {
    title: "Multi-Select Mode",
    bindings: MULTI_SELECT_BINDINGS,
}];

pub(crate) fn normal_sections() -> &'static [KeymapSection] {
    NORMAL_SECTIONS
}

pub(crate) fn executing_sections() -> &'static [KeymapSection] {
    EXECUTING_SECTIONS
}

pub(crate) fn help_sections() -> &'static [KeymapSection] {
    HELP_SECTIONS
}

pub(crate) fn board_sections() -> &'static [KeymapSection] {
    BOARD_SECTIONS
}

pub(crate) fn multi_select_sections() -> &'static [KeymapSection] {
    MULTI_SELECT_SECTIONS
}

pub(crate) fn help_close_keys() -> &'static [&'static str] {
    HELP_CLOSE_KEYS
}

pub(crate) fn footer_hints_for_mode(mode: &AppMode) -> Vec<FooterHint> {
    match mode {
        AppMode::Normal => footer_hints_from_sections(normal_sections()),
        AppMode::Executing { .. } => footer_hints_from_sections(executing_sections()),
        AppMode::Help => HELP_CLOSE_KEYS
            .iter()
            .map(|key| FooterHint {
                keys: key,
                label: "close",
            })
            .collect(),
        AppMode::FlowchartOverlay { .. } => vec![FooterHint {
            keys: "f/Esc",
            label: "close",
        }],
        _ => Vec::new(),
    }
}

fn footer_hints_from_sections(sections: &[KeymapSection]) -> Vec<FooterHint> {
    sections
        .iter()
        .flat_map(|section| {
            section
                .bindings
                .iter()
                .filter_map(|binding| binding.footer_hint)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    fn collect_keys(sections: &[KeymapSection]) -> HashSet<&'static str> {
        sections
            .iter()
            .flat_map(|section| section.bindings.iter())
            .flat_map(|binding| binding.keys.iter().copied())
            .collect()
    }

    #[test]
    fn normal_keymap_includes_required_bindings() {
        let keys = collect_keys(normal_sections());
        for expected in [
            "o",
            "C",
            "R",
            "K",
            "J",
            "Ctrl+P",
            "Ctrl+F",
            "Tab",
            "PgUp/PgDn",
            "Home/End",
        ] {
            assert!(
                keys.contains(expected),
                "missing {expected} in Normal keymap"
            );
        }
    }

    #[test]
    fn help_close_keys_are_stable() {
        let keys: HashSet<&str> = HELP_CLOSE_KEYS.iter().copied().collect();
        assert!(keys.contains("Esc"));
        assert!(keys.contains("?/h"));
    }

    #[test]
    fn docs_key_reference_includes_keymap_shortcuts() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let doc_path = manifest_dir
            .join("..")
            .join("..")
            .join("docs")
            .join("tui-task-management.md");
        let doc_text = std::fs::read_to_string(&doc_path).expect("read tui-task-management.md");

        for expected in [
            "`K/J`: move selected task up/down",
            "`Tab/Shift+Tab`: switch focus between list/details",
            "`PgUp/PgDn`: page list/details (focused panel)",
            "`Home/End`: jump to top/bottom (focused panel)",
            "`C`: toggle case-sensitive search",
            "`R`: toggle regex search",
            "`Ctrl+P`: command palette (shortcut)",
            "`Ctrl+F`: search tasks (shortcut)",
            "`o`: filter by scope",
        ] {
            assert!(
                doc_text.contains(expected),
                "docs missing key reference: {expected}"
            );
        }
    }
}
