//! Clap CLI introspection utilities for emitting a deterministic CLI spec JSON contract.
//!
//! Purpose:
//! - Clap CLI introspection utilities for emitting a deterministic CLI spec JSON contract.
//!
//! Responsibilities:
//! - Convert an in-memory `clap::Command` (including hidden/internal commands and args) into the
//!   versioned `CliSpec` contract model.
//! - Produce deterministic output by sorting commands/args and by emitting a stable JSON shape.
//!
//! Not handled here:
//! - Adding a user-facing CLI command that prints the spec (it is currently exposed only as the
//!   hidden/internal `ralph __cli-spec` command).
//! - File IO, stdout/stderr printing, or schema generation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The caller provides the fully constructed clap command (e.g. `Cli::command()`).
//! - Output ordering is deterministic: args are sorted by `id`, subcommands by `name`.

use anyhow::Result;
use clap::{Arg, ArgAction, Command};
use std::any::TypeId;

use crate::contracts::{ArgSpec, CLI_SPEC_VERSION, CliSpec, CommandSpec};

/// Convert a clap command tree into the versioned `CliSpec` model.
pub fn cli_spec_from_command(command: &Command) -> CliSpec {
    let root_name = command.get_name().to_owned();
    CliSpec {
        version: CLI_SPEC_VERSION,
        root: command_spec_from_command(command, vec![root_name]),
    }
}

/// Convert a clap command tree into deterministic pretty JSON.
pub fn cli_spec_json_pretty_from_command(command: &Command) -> Result<String> {
    let spec = cli_spec_from_command(command);
    Ok(serde_json::to_string_pretty(&spec)?)
}

fn command_spec_from_command(command: &Command, path: Vec<String>) -> CommandSpec {
    let name = command.get_name().to_owned();

    let mut args: Vec<ArgSpec> = command.get_arguments().map(arg_spec_from_arg).collect();
    args.sort_by(|a, b| a.id.cmp(&b.id));

    let mut subcommands: Vec<CommandSpec> = command
        .get_subcommands()
        .map(|subcommand| {
            let mut sub_path = path.clone();
            sub_path.push(subcommand.get_name().to_owned());
            command_spec_from_command(subcommand, sub_path)
        })
        .collect();
    subcommands.sort_by(|a, b| a.name.cmp(&b.name));

    CommandSpec {
        name,
        path,
        about: command.get_about().map(ToString::to_string),
        long_about: command.get_long_about().map(ToString::to_string),
        after_long_help: command.get_after_long_help().map(ToString::to_string),
        hidden: command.is_hide_set(),
        args,
        subcommands,
    }
}

fn arg_spec_from_arg(arg: &Arg) -> ArgSpec {
    let id = arg.get_id().to_string();
    let index = arg.get_index();

    let effective_range = arg
        .get_num_args()
        .unwrap_or_else(|| match arg.get_action() {
            ArgAction::SetTrue
            | ArgAction::SetFalse
            | ArgAction::Count
            | ArgAction::Help
            | ArgAction::Version => 0.into(),
            ArgAction::Set | ArgAction::Append => 1.into(),
            &_ => 1.into(),
        });
    let num_args_min = effective_range.min_values();
    let num_args_max = match effective_range.max_values() {
        usize::MAX => None,
        max => Some(max),
    };

    let takes_value = num_args_max != Some(0);

    // For flags (no values), clap may expose internal "possible values" that are not meaningful
    // for a UI; suppress those to keep the contract intuitive.
    let default_values: Vec<String> = if takes_value {
        arg.get_default_values()
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect()
    } else {
        Vec::new()
    };

    let mut possible_values: Vec<String> = if takes_value {
        arg.get_possible_values()
            .into_iter()
            .map(|value| value.get_name().to_string())
            .collect()
    } else {
        Vec::new()
    };
    possible_values.sort();

    // Heuristic: clap's `ValueEnum` derives parse into the enum type, while raw
    // `PossibleValuesParser` parses into `String`. We intentionally do not classify `bool`
    // as a `ValueEnum` even though it has enumerated possible values.
    let value_type_id = arg.get_value_parser().type_id();
    let value_enum = takes_value
        && !possible_values.is_empty()
        && value_type_id != TypeId::of::<String>()
        && value_type_id != TypeId::of::<std::ffi::OsString>()
        && value_type_id != TypeId::of::<std::path::PathBuf>()
        && value_type_id != TypeId::of::<bool>();

    ArgSpec {
        id,
        long: arg.get_long().map(ToOwned::to_owned),
        short: arg.get_short(),
        help: arg.get_help().map(ToString::to_string),
        long_help: arg.get_long_help().map(ToString::to_string),
        required: arg.is_required_set(),
        default_values,
        possible_values,
        value_enum,
        num_args_min,
        num_args_max,
        global: arg.is_global_set(),
        hidden: arg.is_hide_set(),
        positional: index.is_some(),
        index,
        action: format!("{:?}", arg.get_action()),
    }
}

#[cfg(test)]
mod tests {
    use super::{cli_spec_from_command, cli_spec_json_pretty_from_command};
    use crate::contracts::CLI_SPEC_VERSION;
    use crate::contracts::{ArgSpec, CommandSpec};
    use clap::{Arg, Command};

    fn find_command_by_path<'a>(cmd: &'a CommandSpec, path: &[&str]) -> Option<&'a CommandSpec> {
        if cmd.path.iter().map(String::as_str).eq(path.iter().copied()) {
            return Some(cmd);
        }
        for sub in &cmd.subcommands {
            if let Some(found) = find_command_by_path(sub, path) {
                return Some(found);
            }
        }
        None
    }

    fn find_arg<'a>(cmd: &'a CommandSpec, id: &str) -> Option<&'a ArgSpec> {
        cmd.args.iter().find(|a| a.id == id)
    }

    fn assert_sorted(cmd: &CommandSpec) {
        let sub_names: Vec<&str> = cmd.subcommands.iter().map(|c| c.name.as_str()).collect();
        let mut sorted_sub_names = sub_names.clone();
        sorted_sub_names.sort();
        assert_eq!(
            sub_names, sorted_sub_names,
            "subcommands not sorted for {:?}",
            cmd.path
        );

        let arg_ids: Vec<&str> = cmd.args.iter().map(|a| a.id.as_str()).collect();
        let mut sorted_arg_ids = arg_ids.clone();
        sorted_arg_ids.sort();
        assert_eq!(
            arg_ids, sorted_arg_ids,
            "args not sorted for {:?}",
            cmd.path
        );

        for arg in &cmd.args {
            let mut sorted_possible_values = arg.possible_values.clone();
            sorted_possible_values.sort();
            assert_eq!(
                arg.possible_values, sorted_possible_values,
                "possible_values not sorted for arg {:?} in {:?}",
                arg.id, cmd.path
            );
            if let Some(max) = arg.num_args_max {
                assert!(
                    arg.num_args_min <= max,
                    "num_args_min must be <= num_args_max for arg {:?} in {:?}",
                    arg.id,
                    cmd.path
                );
            }
        }

        for sub in &cmd.subcommands {
            assert_sorted(sub);
        }
    }

    #[test]
    fn cli_spec_json_is_deterministic_for_ralph_cli() -> anyhow::Result<()> {
        use clap::CommandFactory;

        let cmd1 = crate::cli::Cli::command();
        let json1 = cli_spec_json_pretty_from_command(&cmd1)?;

        let cmd2 = crate::cli::Cli::command();
        let json2 = cli_spec_json_pretty_from_command(&cmd2)?;

        assert_eq!(json1, json2);
        Ok(())
    }

    #[test]
    fn cli_spec_is_sorted_and_has_required_root_fields() {
        use clap::CommandFactory;

        let command = crate::cli::Cli::command();
        let spec = cli_spec_from_command(&command);

        assert_eq!(spec.version, CLI_SPEC_VERSION);
        assert_eq!(spec.root.name, "ralph");
        assert_eq!(spec.root.path, vec!["ralph".to_string()]);

        assert_sorted(&spec.root);
    }

    #[test]
    fn cli_spec_includes_hidden_internal_command_and_marks_it_hidden() {
        use clap::CommandFactory;

        let command = crate::cli::Cli::command();
        let spec = cli_spec_from_command(&command);

        let serve = find_command_by_path(&spec.root, &["ralph", "daemon", "serve"])
            .expect("expected hidden daemon serve command to exist in spec");
        assert!(serve.hidden, "expected daemon serve to be marked hidden");
    }

    #[test]
    fn cli_spec_includes_hidden_internal_arg_and_marks_it_hidden() {
        use clap::CommandFactory;

        let command = crate::cli::Cli::command();
        let spec = cli_spec_from_command(&command);

        let run_one = find_command_by_path(&spec.root, &["ralph", "run", "one"])
            .expect("expected run one command to exist in spec");
        let arg = find_arg(run_one, "parallel_worker")
            .expect("expected parallel_worker arg to exist in spec");
        assert!(arg.hidden, "expected parallel_worker to be marked hidden");
    }

    #[test]
    fn cli_spec_includes_defaults_possible_values_num_args_and_value_enum() {
        use clap::CommandFactory;

        let command = crate::cli::Cli::command();
        let spec = cli_spec_from_command(&command);

        let color = find_arg(&spec.root, "color").expect("expected color arg to exist");
        assert_eq!(color.default_values, vec!["auto".to_string()]);
        assert_eq!(
            color.possible_values,
            vec![
                "always".to_string(),
                "auto".to_string(),
                "never".to_string()
            ]
        );
        assert!(
            color.value_enum,
            "expected --color to be detected as a ValueEnum"
        );
        assert_eq!(color.num_args_min, 1);
        assert_eq!(color.num_args_max, Some(1));
        assert!(!color.required);
        assert_eq!(color.help.as_deref(), Some("Color output control"));

        let verbose = find_arg(&spec.root, "verbose").expect("expected verbose arg to exist");
        assert!(verbose.default_values.is_empty());
        assert!(verbose.possible_values.is_empty());
        assert!(
            !verbose.value_enum,
            "expected --verbose to not be detected as a ValueEnum"
        );
        assert_eq!(verbose.num_args_min, 0);
        assert_eq!(verbose.num_args_max, Some(0));
        assert!(!verbose.required);
    }

    #[test]
    fn cli_spec_reflects_unbounded_num_args_and_non_value_enum_possible_values() {
        let root = Command::new("root").arg(
            Arg::new("mode")
                .long("mode")
                .value_parser(["a", "b"])
                .default_value("a"),
        );
        let root = root.arg(Arg::new("items").long("item").num_args(1..));

        let spec = cli_spec_from_command(&root);
        let mode = find_arg(&spec.root, "mode").expect("expected mode arg");
        assert_eq!(mode.default_values, vec!["a".to_string()]);
        assert_eq!(mode.possible_values, vec!["a".to_string(), "b".to_string()]);
        assert!(!mode.value_enum, "expected mode to not be a ValueEnum");
        assert_eq!(mode.num_args_min, 1);
        assert_eq!(mode.num_args_max, Some(1));

        let items = find_arg(&spec.root, "items").expect("expected items arg");
        assert_eq!(items.num_args_min, 1);
        assert_eq!(items.num_args_max, None);
    }

    #[test]
    fn cli_spec_is_deterministic_even_when_builder_insertion_order_differs() -> anyhow::Result<()> {
        fn build(order: u8) -> Command {
            let mut root = Command::new("root");

            let a = Arg::new("alpha").long("alpha");
            let z = Arg::new("zeta").long("zeta");

            let sub_a = Command::new("a").arg(Arg::new("x").long("x"));
            let sub_b = Command::new("b").arg(Arg::new("y").long("y").hide(true));

            if order == 0 {
                root = root.arg(z).arg(a);
                root = root.subcommand(sub_b).subcommand(sub_a);
            } else {
                root = root.arg(a).arg(z);
                root = root.subcommand(sub_a).subcommand(sub_b);
            }

            root
        }

        let json1 = cli_spec_json_pretty_from_command(&build(0))?;
        let json2 = cli_spec_json_pretty_from_command(&build(1))?;

        assert_eq!(json1, json2);
        Ok(())
    }
}
