//! The command-line options of a feature run, as a [`quench_args::Args`] group to flatten into your own CLI.

use quench_args::Args;
use std::path::PathBuf;

/// Which scenarios to run and how.
#[derive(Args, Debug, Clone, Default)]
pub struct Options {
    /// Run only scenarios whose tags satisfy this expression (`@fast and not @slow`)
    #[arg(long, short = 't', value_name = "EXPR")]
    pub tags: Option<String>,
    /// Run only scenarios whose name matches this regex
    #[arg(long, short = 'n', value_name = "REGEX")]
    pub name: Option<String>,
    /// How many scenarios run at once (default 64)
    #[arg(long, value_name = "N")]
    pub concurrency: Option<usize>,
    /// Skip the scenarios not yet started after the first failure
    #[arg(long)]
    pub fail_fast: bool,
    /// Write the JSON and HTML reports here instead of `target/<profile>/quench-test`
    #[arg(long, value_name = "DIR")]
    pub report_dir: Option<PathBuf>,
    /// Print only failures and the summary
    #[arg(long, short = 'q')]
    pub quiet: bool,
}
