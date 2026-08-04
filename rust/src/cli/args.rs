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
pub(super) enum CliColorMode {
    Auto,
    Always,
    #[default]
    Never,
}

impl CliColorMode {
    pub(super) fn enabled(self, no_color_is_set: bool) -> bool {
        match self {
            Self::Auto => !no_color_is_set,
            Self::Always => true,
            Self::Never => false,
        }
    }
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

#[derive(Clone, Parser, Debug)]
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
        conflicts_with_all = ["use_default_root", "saved_root", "named_root", "preset", "list_saved_roots"]
    )]
    pub(super) root: Option<PathBuf>,

    /// Search using the persisted default root.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["root", "saved_root", "named_root", "preset", "list_saved_roots"]
    )]
    pub(super) use_default_root: bool,

    /// Search using a one-based index from the persisted saved roots.
    #[arg(
        long,
        value_name = "INDEX",
        requires = "cli",
        conflicts_with_all = ["root", "use_default_root", "named_root", "preset", "list_saved_roots"]
    )]
    pub(super) saved_root: Option<usize>,

    /// Search using a configured named root.
    #[arg(
        long,
        value_name = "NAME",
        requires = "cli",
        conflicts_with_all = ["root", "use_default_root", "saved_root", "preset", "list_saved_roots"]
    )]
    pub(super) named_root: Option<String>,

    /// Apply a saved pure-search preset in batch or interactive CLI mode.
    #[arg(
        long,
        value_name = "NAME",
        requires = "cli",
        conflicts_with_all = [
            "root", "use_default_root", "saved_root", "named_root", "entry_type", "regex",
            "case_sensitive", "source", "ignore_file", "no_ignore", "sort", "create_filelist",
            "list_saved_roots"
        ]
    )]
    pub(super) preset: Option<String>,

    /// Maximum number of paths to return.
    #[arg(long, default_value_t = 1000)]
    pub(super) limit: usize,

    /// Print paths without opening the GUI.
    #[arg(long, default_value_t = false)]
    pub(super) cli: bool,

    /// Run the interactive terminal selector.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) interactive: bool,

    /// Highlight matched text in CLI output (auto, always, or never).
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "auto",
        requires = "cli"
    )]
    pub(super) color: Option<CliColorMode>,

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

    /// List configured named roots without indexing.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) list_named_roots: bool,

    /// Add a named root as NAME=PATH and exit without indexing.
    #[arg(long, value_name = "NAME=PATH", requires = "cli")]
    pub(super) add_named_root: Option<String>,

    /// Remove a named root and exit without indexing.
    #[arg(long, value_name = "NAME", requires = "cli")]
    pub(super) remove_named_root: Option<String>,

    /// List configured search preset names without indexing.
    #[arg(long, default_value_t = false, requires = "cli")]
    pub(super) list_presets: bool,

    /// Save the current pure search options as NAME and exit without indexing.
    #[arg(long, value_name = "NAME", requires = "cli")]
    pub(super) save_preset: Option<String>,

    /// Remove a search preset and exit without indexing.
    #[arg(long, value_name = "NAME", requires = "cli")]
    pub(super) remove_preset: Option<String>,
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

    pub(super) fn color_mode(&self) -> CliColorMode {
        self.color.unwrap_or_default()
    }
}

pub(crate) fn parse_args() -> Args {
    Args::parse_from(normalize_update_args(std::env::args_os()))
}

fn normalize_update_args(arguments: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let is_update_command = arguments
        .iter()
        .any(|argument| argument == "--check-update" || argument == "--update");
    if !is_update_command {
        return arguments;
    }
    arguments
        .into_iter()
        .filter(|argument| argument != "--cli")
        .collect()
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
    validate_catalog_args(args)?;
    if args.preset.is_some() && !args.query.is_empty() {
        return Err("--preset cannot be combined with an explicit QUERY".to_string());
    }
    Ok(())
}

fn validate_catalog_args(args: &Args) -> std::result::Result<(), String> {
    let management_count = usize::from(args.list_named_roots)
        + usize::from(args.add_named_root.is_some())
        + usize::from(args.remove_named_root.is_some())
        + usize::from(args.list_presets)
        + usize::from(args.save_preset.is_some())
        + usize::from(args.remove_preset.is_some());
    if management_count > 1 {
        return Err("catalog management options are mutually exclusive".to_string());
    }
    if args.preset.is_some() && management_count > 0 {
        return Err("--preset cannot be combined with catalog management options".to_string());
    }
    if let Some(spec) = args.add_named_root.as_deref() {
        let Some((name, path)) = spec.split_once('=') else {
            return Err("--add-named-root requires NAME=PATH".to_string());
        };
        if name.trim().is_empty() || path.trim().is_empty() {
            return Err("--add-named-root requires non-empty NAME and PATH".to_string());
        }
    }
    if management_count > 0 && args.interactive {
        return Err("catalog management options cannot be combined with --interactive".to_string());
    }
    if management_count > 0
        && (args.exec_command.is_some()
            || args.create_filelist
            || args.list_saved_roots
            || args.action != CliAction::Print
            || args.action_all
            || args.absolute
            || args.print0
            || args.progress
            || args.fail_no_match)
    {
        return Err(
            "catalog management options cannot be combined with output, action, or FileList operations"
                .to_string(),
        );
    }
    if management_count > 0 && args.save_preset.is_none() && !args.query.is_empty() {
        return Err("this catalog management option cannot be combined with QUERY".to_string());
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
    use std::ffi::OsString;

    use super::{normalize_update_args, Args, CliColorMode, CliEntryType, CliIndexSource};

    #[test]
    fn tc_169_cli_alias_flag_is_inert_for_update_commands() {
        let normalized = normalize_update_args([
            OsString::from("flistwalker"),
            OsString::from("--cli"),
            OsString::from("--update"),
        ]);

        assert_eq!(
            normalized,
            [OsString::from("flistwalker"), OsString::from("--update")]
        );
        Args::try_parse_from(normalized).expect("parse update command after alias normalization");
    }

    #[test]
    fn default_gui_args_do_not_trigger_cli_option_requirements() {
        let args = Args::try_parse_from(["flistwalker"]).expect("parse default GUI arguments");

        assert!(!args.cli);
        assert!(!args.interactive);
        assert!(matches!(args.entry_type, CliEntryType::All));
        assert!(matches!(args.source, CliIndexSource::Auto));
    }

    #[test]
    fn tc_172_color_option_defaults_to_never_and_accepts_grep_style_auto() {
        let default_args =
            Args::try_parse_from(["flistwalker", "--cli"]).expect("parse default color mode");
        assert!(matches!(default_args.color_mode(), CliColorMode::Never));

        let auto = Args::try_parse_from(["flistwalker", "--cli", "--color"])
            .expect("parse bare color option");
        assert!(matches!(auto.color_mode(), CliColorMode::Auto));

        let always = Args::try_parse_from(["flistwalker", "--cli", "--color", "always"])
            .expect("parse always color mode");
        assert!(matches!(always.color_mode(), CliColorMode::Always));

        let never = Args::try_parse_from(["flistwalker", "--cli", "--color", "never"])
            .expect("parse never color mode");
        assert!(matches!(never.color_mode(), CliColorMode::Never));

        let gui = Args::try_parse_from(["flistwalker", "--color", "never"])
            .expect_err("color option must require CLI mode");
        assert!(gui.to_string().contains("--cli"));
    }

    #[test]
    fn tc_172_color_mode_obeys_no_color_except_when_forced() {
        assert!(CliColorMode::Auto.enabled(false));
        assert!(!CliColorMode::Auto.enabled(true));
        assert!(CliColorMode::Always.enabled(true));
        assert!(!CliColorMode::Never.enabled(false));
    }
}
