//! CLI specification contract emitted as deterministic JSON.
//!
//! Responsibilities:
//! - Define the versioned, serialized data model (`CliSpec`, `CommandSpec`, `ArgSpec`) for emitting
//!   a machine-readable description of Ralph's clap CLI.
//! - Provide a stable contract suitable for tooling (docs generation, wrappers, completions).
//!
//! Not handled here:
//! - Extracting data from `clap::Command` (see `crate::cli_spec`).
//! - CLI command wiring, IO, or printing (see `crate::commands` when integrated).
//!
//! Invariants/assumptions:
//! - `CliSpec.version` is bumped only for breaking JSON changes.
//! - `CommandSpec.path` is the full command path from the root (e.g. `["ralph","run","one"]`).
//! - `CommandSpec` and `ArgSpec` vectors are expected to be deterministically sorted by the emitter.
//! - `ArgSpec.possible_values` is expected to be deterministically sorted by the emitter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Current JSON format version for `CliSpec`.
pub const CLI_SPEC_VERSION: u32 = 2;

/// Root CLI spec document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CliSpec {
    /// JSON format version.
    pub version: u32,

    /// Root command and its full subcommand tree.
    pub root: CommandSpec,
}

/// A command/subcommand and its arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// Command name (the last segment of `path`).
    pub name: String,

    /// Full path from the root command, inclusive.
    pub path: Vec<String>,

    /// Short description shown in `--help`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,

    /// Long description shown in `--help`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_about: Option<String>,

    /// Extra help appended after long help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_long_help: Option<String>,

    /// Whether the command is hidden from normal help output.
    pub hidden: bool,

    /// Arguments available at this command level (including hidden and generated help/version args).
    pub args: Vec<ArgSpec>,

    /// Nested subcommands (including hidden/internal subcommands).
    pub subcommands: Vec<CommandSpec>,
}

/// A single CLI argument/flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArgSpec {
    /// Clap argument id (stable identifier used for conflict groups, etc.).
    pub id: String,

    /// Long flag name without leading `--`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,

    /// Short flag letter (without leading `-`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<char>,

    /// Help text shown in `--help`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,

    /// Long help text shown in `--help`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_help: Option<String>,

    /// Whether the argument is required.
    pub required: bool,

    /// Default values (as shown by clap and used when the argument is absent).
    ///
    /// This is always present; an empty list means there is no configured default.
    pub default_values: Vec<String>,

    /// Enumerated possible values for the argument value parser (if known).
    ///
    /// This is always present; an empty list means clap does not advertise a finite set of
    /// possible values for this argument.
    pub possible_values: Vec<String>,

    /// Whether the argument's value is parsed as a clap `ValueEnum` type.
    ///
    /// This is intended for tooling (e.g., rendering dropdowns) and is a best-effort reflection of
    /// the clap configuration.
    pub value_enum: bool,

    /// Minimum number of values this argument accepts per occurrence.
    pub num_args_min: usize,

    /// Maximum number of values this argument accepts per occurrence (inclusive).
    ///
    /// `None` means unbounded.
    pub num_args_max: Option<usize>,

    /// Whether the argument is global (propagates to subcommands).
    pub global: bool,

    /// Whether the argument is hidden from normal help output.
    pub hidden: bool,

    /// Whether the argument is positional.
    pub positional: bool,

    /// For positional arguments, the 1-based index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,

    /// The clap action driving how values are applied (e.g. `Set`, `SetTrue`, `Append`).
    pub action: String,
}
