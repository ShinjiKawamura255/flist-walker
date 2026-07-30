use clap::{Parser, ValueEnum};
use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use flist_walker::actions::AuthorizedActionMode;
use flist_walker::command_exec::CommandTemplate;
use flist_walker::search::SearchSortMode as RuntimeSortMode;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(super) enum CliEntryType {
    #[default]
    All,
    File,
    Folder,
}

impl CliEntryType {
    pub(super) fn include_flags(self) -> (bool, bool) {
        match self {
            Self::All => (true, true),
            Self::File => (true, false),
            Self::Folder => (false, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(super) enum CliIndexSource {
    #[default]
    Auto,
    Filelist,
    Walker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(super) enum CliSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
    SizeDesc,
    SizeAsc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(super) enum CliAction {
    #[default]
    Print,
    Open,
    Reveal,
}

impl CliAction {
    pub(super) fn authorized_mode(self) -> Option<AuthorizedActionMode> {
        match self {
            Self::Print => None,
            Self::Open => Some(AuthorizedActionMode::ExecuteOrOpen),
            Self::Reveal => Some(AuthorizedActionMode::Reveal),
        }
    }
}

impl From<CliSortMode> for RuntimeSortMode {
    fn from(value: CliSortMode) -> Self {
        match value {
            CliSortMode::Score => Self::Score,
            CliSortMode::NameAsc => Self::NameAsc,
            CliSortMode::NameDesc => Self::NameDesc,
            CliSortMode::ModifiedDesc => Self::ModifiedDesc,
            CliSortMode::ModifiedAsc => Self::ModifiedAsc,
            CliSortMode::CreatedDesc => Self::CreatedDesc,
            CliSortMode::CreatedAsc => Self::CreatedAsc,
            CliSortMode::SizeDesc => Self::SizeDesc,
            CliSortMode::SizeAsc => Self::SizeAsc,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "flistwalker")]
#[command(about = "Find files and folders with fuzzy search")]
#[command(version)]
pub(crate) struct Args {
    /// Query using fuzzy matching and the supported fzf-style operators.
    #[arg(default_value = "", value_name = "QUERY")]
    pub(super) query: String,

    /// Check for a newer release without installing it.
    #[arg(long, default_value_t = false, exclusive = true)]
    pub(super) check_update: bool,

    /// Check for and install the latest supported release.
    #[arg(long, default_value_t = false, exclusive = true)]
    pub(super) update: bool,

    /// Root directory to search (defaults to the current directory).
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with_all = ["use_default_root", "saved_root", "list_saved_roots"]
    )]
    pub(super) root: Option<PathBuf>,

    /// Search using the persisted default root.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["root", "saved_root", "list_saved_roots"]
    )]
    pub(super) use_default_root: bool,

    /// Search using a one-based index from the persisted saved roots.
    #[arg(
        long,
        value_name = "INDEX",
        requires = "cli",
        conflicts_with_all = ["root", "use_default_root", "list_saved_roots"]
    )]
    pub(super) saved_root: Option<usize>,

    /// Maximum number of paths to return.
    #[arg(long, default_value_t = 1000)]
    pub(super) limit: usize,

    /// Print paths without opening the GUI.
    #[arg(long, default_value_t = false)]
    pub(super) cli: bool,

    /// Run the interactive terminal selector.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) interactive: bool,

    /// Print absolute paths instead of paths relative to the root.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) absolute: bool,

    /// Terminate each output path with NUL instead of a newline.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) print0: bool,

    /// Exit with status 1 when no path matches (batch CLI only).
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with = "interactive"
    )]
    pub(super) fail_no_match: bool,

    /// Select files, folders, or both.
    #[arg(
        long = "type",
        value_enum,
        default_value_t = CliEntryType::All,
        requires = "cli"
    )]
    pub(super) entry_type: CliEntryType,

    /// Interpret QUERY as a regular expression.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) regex: bool,

    /// Match QUERY and ignore terms case-sensitively.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) case_sensitive: bool,

    /// Choose automatic FileList preference, FileList only, or walker only.
    #[arg(long, value_enum, default_value_t = CliIndexSource::Auto, requires = "cli")]
    pub(super) source: CliIndexSource,

    /// Read ignore terms from PATH instead of the executable-side ignore file.
    #[arg(
        long,
        value_name = "PATH",
        requires = "cli",
        conflicts_with = "no_ignore"
    )]
    pub(super) ignore_file: Option<PathBuf>,

    /// Disable ignore-list filtering.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) no_ignore: bool,

    /// Write indexing progress to standard error (batch CLI only).
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with = "interactive"
    )]
    pub(super) progress: bool,

    /// Sort the complete match set before applying --limit.
    #[arg(
        long,
        value_enum,
        default_value_t = CliSortMode::Score,
        requires = "cli"
    )]
    pub(super) sort: CliSortMode,

    /// Print matches, open a match, or reveal its containing folder.
    #[arg(
        long,
        value_enum,
        default_value_t = CliAction::Print,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots"]
    )]
    pub(super) action: CliAction,

    /// Allow an open or reveal action to target every post-limit match.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots"]
    )]
    pub(super) action_all: bool,

    /// Execute COMMAND with all result paths expanded at one standalone {} argument.
    #[arg(
        short = 'x',
        long = "exec",
        value_name = "COMMAND... {} ...",
        num_args = 1..,
        allow_hyphen_values = true,
        requires = "cli",
        conflicts_with_all = ["action_all", "absolute", "print0", "list_saved_roots", "create_filelist"]
    )]
    pub(super) exec_command: Option<Vec<OsString>>,

    /// Cap the number of result paths placed in each external-command batch.
    #[arg(
        long,
        value_name = "N",
        requires = "exec_command",
        conflicts_with_all = ["action_all", "absolute", "print0"]
    )]
    pub(super) exec_max_args: Option<NonZeroUsize>,

    /// Report external-command batch counts without starting the command.
    #[arg(
        long,
        default_value_t = false,
        requires = "exec_command",
        conflicts_with_all = ["action_all", "absolute", "print0"]
    )]
    pub(super) dry_run: bool,

    /// Create the root FileList from a fresh walker index without prompting.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots", "action", "action_all"]
    )]
    pub(super) create_filelist: bool,

    /// Permit replacing an existing root FileList during --create-filelist.
    #[arg(long, default_value_t = false, requires = "create_filelist")]
    pub(super) overwrite_filelist: bool,

    /// Update pre-existing ancestor FileLists during --create-filelist.
    #[arg(long, default_value_t = false, requires = "create_filelist")]
    pub(super) propagate_ancestors: bool,

    /// List persisted saved roots without indexing or selecting paths.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = [
            "root",
            "use_default_root",
            "saved_root",
            "interactive",
            "action",
            "action_all"
        ]
    )]
    pub(super) list_saved_roots: bool,
}

impl Args {
    pub(crate) fn requests_update_command(&self) -> bool {
        self.check_update || self.update
    }

    pub(crate) fn update_requested(&self) -> bool {
        self.update
    }

    pub(crate) fn is_cli(&self) -> bool {
        self.cli
    }

    pub(crate) fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub(crate) fn query(&self) -> String {
        self.query.clone()
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit
    }
}

pub(crate) fn parse_args() -> Args {
    Args::parse()
}

pub(super) fn parse_exec_template(
    args: &Args,
) -> std::result::Result<Option<CommandTemplate>, String> {
    args.exec_command
        .as_deref()
        .map(CommandTemplate::parse)
        .transpose()
        .map_err(|error| error.to_string())
}

pub(crate) fn validate_args(args: &Args) -> std::result::Result<(), String> {
    if args.cli {
        validate_exec_args(args)?;
    }
    if args.cli && !args.interactive {
        validate_batch_action_args(args).map_err(str::to_owned)?;
        validate_create_filelist_args(args).map_err(str::to_owned)?;
        if args.list_saved_roots {
            validate_list_saved_roots_args(args).map_err(str::to_owned)?;
        }
    }
    Ok(())
}

pub(super) fn validate_list_saved_roots_args(args: &Args) -> std::result::Result<(), &'static str> {
    if !args.query.is_empty()
        || args.limit != 1000
        || args.absolute
        || args.fail_no_match
        || !matches!(args.entry_type, CliEntryType::All)
        || args.regex
        || args.case_sensitive
        || !matches!(args.source, CliIndexSource::Auto)
        || args.ignore_file.is_some()
        || args.no_ignore
        || args.progress
        || !matches!(args.sort, CliSortMode::Score)
        || !matches!(args.action, CliAction::Print)
        || args.action_all
    {
        return Err("--list-saved-roots cannot be combined with search options");
    }
    Ok(())
}

fn validate_batch_action_args(args: &Args) -> std::result::Result<(), &'static str> {
    if args.action_all && args.action == CliAction::Print {
        return Err("--action-all requires --action open or --action reveal");
    }
    if args.action != CliAction::Print && (args.absolute || args.print0) {
        return Err("--absolute and --print0 are only valid with --action print");
    }
    Ok(())
}

fn validate_exec_args(args: &Args) -> std::result::Result<(), String> {
    let template = parse_exec_template(args)?;
    if template.is_some() && args.action != CliAction::Print {
        return Err("--exec cannot be combined with --action open or --action reveal".into());
    }
    Ok(())
}

fn validate_create_filelist_args(args: &Args) -> std::result::Result<(), &'static str> {
    if !args.create_filelist {
        return Ok(());
    }
    if !args.query.is_empty()
        || args.limit != 1000
        || args.absolute
        || args.print0
        || args.fail_no_match
        || !matches!(args.entry_type, CliEntryType::All)
        || args.regex
        || args.case_sensitive
        || !matches!(args.source, CliIndexSource::Auto)
        || args.ignore_file.is_some()
        || args.no_ignore
        || !matches!(args.sort, CliSortMode::Score)
        || !matches!(args.action, CliAction::Print)
        || args.action_all
    {
        return Err("--create-filelist cannot be combined with search, output, or action options");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Args, CliEntryType, CliIndexSource};

    #[test]
    fn default_gui_args_do_not_trigger_cli_option_requirements() {
        let args = Args::try_parse_from(["flistwalker"]).expect("parse default GUI arguments");

        assert!(!args.cli);
        assert!(!args.interactive);
        assert!(matches!(args.entry_type, CliEntryType::All));
        assert!(matches!(args.source, CliIndexSource::Auto));
    }
}
