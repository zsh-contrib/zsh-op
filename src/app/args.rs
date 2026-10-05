use clap::{Args, Parser, Subcommand, ValueEnum};
use core::fmt::Display;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Program is the main entry point for the zsh-op CLI.
#[derive(Debug, Parser)]
#[command(
    name = "zsh-op",
    about = "1Password secrets for your shell.",
    long_about = "Fetch secrets from 1Password, cache them in the system keychain, export them into your shell, and add SSH keys to ssh-agent.",
    version
)]
pub struct Program {
    /// Command specifies the subcommand to execute.
    #[command(subcommand)]
    pub command: ProgramCommand,
}

/// ProgramArgs holds the shared global flags available to every subcommand.
#[derive(Debug, Args)]
pub struct ProgramArgs {
    /// Path to the zsh-op configuration file.
    #[arg(
        help = "Config file path.",
        env = "ZSH_OP_CONFIG_FILE",
        default_value_os_t = home_dir().join(".config/op/config.yml"),
        long,
        short
    )]
    pub config: PathBuf,

    /// Path to the directory holding the cached profile metadata.
    #[arg(
        help = "Cache directory path.",
        env = "ZSH_OP_CACHE_DIR",
        default_value_os_t = home_dir().join(".cache/op"),
        long
    )]
    pub cache_dir: PathBuf,
}

impl Default for ProgramArgs {
    fn default() -> Self {
        Self {
            config: home_dir().join(".config/op/config.yml"),
            cache_dir: home_dir().join(".cache/op"),
        }
    }
}

/// Returns the home directory of the current user.
fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Top-level subcommand dispatched by [`Program`].
#[derive(Debug, Subcommand)]
pub enum ProgramCommand {
    /// Inspect and display the configured profiles, their secrets and their cache state.
    #[command(
        name = "inspect",
        about = "Inspect and display the configured profiles, their secrets and their cache state.",
        long_about = "Parse the configuration and print every profile with its 1Password account, its secrets and whether it has been loaded into the keychain cache. Secret values are never printed.",
        next_display_order = 1
    )]
    Inspect(InspectCommandArgs),

    /// List profile names, or the secret names of a profile, one per line.
    #[command(
        name = "list",
        about = "List profile names, or the secret names of a profile, one per line.",
        long_about = "Print the configured profile names, or the secret names of the given profile, one per line. Intended for shell completions and scripts.",
        next_display_order = 2
    )]
    List(ListCommandArgs),

    /// Set up the shell environment with all secrets from a profile.
    #[command(
        name = "shell",
        about = "Set up the shell environment with all secrets from a profile.",
        long_about = "Load every secret of a profile: emit shell-ready export statements for environment and file secrets, add SSH keys to ssh-agent, and record the profile so its cached secrets are exported on shell startup.",
        next_display_order = 3
    )]
    Shell(ShellCommandArgs),

    /// Load an individual secret on demand.
    #[command(
        name = "secret",
        about = "Load an individual secret on demand.",
        long_about = "Load a single secret: print an environment secret's value, write a file secret and print its path, or add an SSH key to ssh-agent. With --export, emit a shell-ready export statement instead.",
        next_display_order = 4
    )]
    Secret(SecretCommandArgs),

    /// Export the environment and file secrets of a profile as shell statements.
    #[command(
        name = "export",
        about = "Export the environment and file secrets of a profile as shell statements.",
        long_about = "Emit shell-ready export statements (or JSON) for the environment and file secrets of a profile. With --cached, only secrets of previously loaded profiles are read from the keychain and 1Password is never contacted.",
        next_display_order = 5
    )]
    Export(ExportCommandArgs),

    /// Execute a command with the secrets of a profile in its environment.
    #[command(
        name = "exec",
        about = "Execute a command with the secrets of a profile in its environment.",
        long_about = "Resolve the environment and file secrets of a profile, then run the given command with them in its environment. File secrets are removed once the command exits.",
        next_display_order = 6
    )]
    Exec(ExecCommandArgs),

    /// Clear the cached secrets of a profile.
    #[command(
        name = "clear",
        about = "Clear the cached secrets of a profile.",
        long_about = "Delete every cached secret of a profile from the keychain and forget that the profile was loaded.",
        next_display_order = 7
    )]
    Clear(ClearCommandArgs),
}

/// ExportFormat specifies the output format for exported secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ExportFormat {
    /// Zsh format.
    #[default]
    Zsh,

    /// Bash format.
    Bash,

    /// Json format.
    Json,
}

impl FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "zsh" => Ok(Self::Zsh),
            "bash" => Ok(Self::Bash),
            "json" => Ok(Self::Json),
            _ => Err(format!("unknown export format: {}", s)),
        }
    }
}

impl Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zsh => write!(f, "zsh"),
            Self::Bash => write!(f, "bash"),
            Self::Json => write!(f, "json"),
        }
    }
}

/// OutputArgs holds the flags shared by subcommands that emit secrets.
#[derive(Debug, Default, Args)]
pub struct OutputArgs {
    /// Output format for the exported secrets.
    /// Auto-detected from $SHELL if not provided.
    #[arg(
        help = "Output format (auto-detected from $SHELL if not provided).",
        long,
        short
    )]
    pub format: Option<ExportFormat>,

    /// Directory where file secrets are written.
    /// A new private temporary directory is created if not provided.
    #[arg(
        help = "Directory for file secrets (a new temporary one if not provided).",
        long
    )]
    pub runtime_dir: Option<PathBuf>,
}

impl OutputArgs {
    /// Get the export format, using $SHELL detection if not explicitly provided.
    pub fn export_format(&self) -> ExportFormat {
        self.format.unwrap_or_else(|| {
            let shell_path = env::var("SHELL").unwrap_or_default();
            let shell_name = Path::new(&shell_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("zsh");

            <ExportFormat as FromStr>::from_str(shell_name).unwrap_or(ExportFormat::Zsh)
        })
    }
}

/// InspectCommandArgs defines the arguments for the InspectCommand.
#[derive(Debug, Args)]
pub struct InspectCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile to inspect. Every profile is inspected if not provided.
    #[arg(help = "Profile name (all profiles if not provided).", long, short)]
    pub profile: Option<String>,
}

/// ListCommandArgs defines the arguments for the ListCommand.
#[derive(Debug, Args)]
pub struct ListCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile whose secret names are listed. Profile names are listed if not provided.
    #[arg(help = "Profile name (list profiles if not provided).", long, short)]
    pub profile: Option<String>,
}

/// ShellCommandArgs defines the arguments for the ShellCommand.
#[derive(Debug, Args)]
pub struct ShellCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile to load.
    #[arg(
        help = "Profile name.",
        env = "ZSH_OP_DEFAULT_PROFILE",
        default_value = "personal"
    )]
    pub profile: String,

    /// Lifetime of the SSH keys added to ssh-agent.
    #[arg(
        help = "SSH key expiration time (e.g. 30m, 1h, 8h).",
        default_value = "1h",
        long,
        short
    )]
    pub expiration: String,

    /// Bypass the keychain cache and fetch every secret from 1Password.
    #[arg(help = "Force refresh from 1Password.", long, short)]
    pub refresh: bool,

    /// Output flags.
    #[command(flatten)]
    pub output: OutputArgs,
}

/// SecretCommandArgs defines the arguments for the SecretCommand.
#[derive(Debug, Args)]
pub struct SecretCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Name of the secret to load.
    #[arg(help = "Secret name.")]
    pub name: String,

    /// Profile the secret belongs to.
    #[arg(
        help = "Profile name.",
        env = "ZSH_OP_DEFAULT_PROFILE",
        default_value = "personal",
        long,
        short
    )]
    pub profile: String,

    /// Emit an export statement instead of printing the value.
    #[arg(
        help = "Emit an export statement for environment and file secrets.",
        long,
        short = 'x'
    )]
    pub export: bool,

    /// Lifetime of the SSH key added to ssh-agent.
    #[arg(
        help = "SSH key expiration time (e.g. 30m, 1h, 8h).",
        default_value = "1h",
        long,
        short
    )]
    pub expiration: String,

    /// Bypass the keychain cache and fetch the secret from 1Password.
    #[arg(help = "Force refresh from 1Password.", long, short)]
    pub refresh: bool,

    /// Output flags.
    #[command(flatten)]
    pub output: OutputArgs,
}

/// ExportCommandArgs defines the arguments for the ExportCommand.
#[derive(Debug, Args)]
pub struct ExportCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile to export.
    #[arg(
        help = "Profile name.",
        env = "ZSH_OP_DEFAULT_PROFILE",
        default_value = "personal",
        long,
        short
    )]
    pub profile: String,

    /// Export every profile instead of a single one.
    #[arg(help = "Export every profile.", long, short)]
    pub all: bool,

    /// Read previously loaded secrets from the keychain only, never contacting 1Password.
    #[arg(
        help = "Only export cached secrets of loaded profiles.",
        long,
        conflicts_with = "refresh"
    )]
    pub cached: bool,

    /// Bypass the keychain cache and fetch every secret from 1Password.
    #[arg(help = "Force refresh from 1Password.", long, short)]
    pub refresh: bool,

    /// Output flags.
    #[command(flatten)]
    pub output: OutputArgs,
}

/// ExecCommandArgs defines the arguments for the ExecCommand.
#[derive(Debug, Args)]
pub struct ExecCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile whose secrets are applied.
    #[arg(
        help = "Profile name.",
        env = "ZSH_OP_DEFAULT_PROFILE",
        default_value = "personal",
        long,
        short
    )]
    pub profile: String,

    /// Bypass the keychain cache and fetch every secret from 1Password.
    #[arg(help = "Force refresh from 1Password.", long, short)]
    pub refresh: bool,

    /// Command and arguments to execute.
    #[arg(
        help = "Command and arguments to execute.",
        required = true,
        trailing_var_arg = true
    )]
    pub command: Vec<String>,
}

/// ClearCommandArgs defines the arguments for the ClearCommand.
#[derive(Debug, Args)]
pub struct ClearCommandArgs {
    /// Shared global flags.
    #[command(flatten)]
    pub parent: ProgramArgs,

    /// Profile whose cached secrets are deleted.
    #[arg(
        help = "Profile name.",
        env = "ZSH_OP_DEFAULT_PROFILE",
        default_value = "personal",
        long,
        short
    )]
    pub profile: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn program_definition_is_valid() {
        Program::command().debug_assert();
    }

    #[test]
    fn export_format_displays_as_lowercase_name() {
        assert_eq!(ExportFormat::Zsh.to_string(), "zsh");
        assert_eq!(ExportFormat::Bash.to_string(), "bash");
        assert_eq!(ExportFormat::Json.to_string(), "json");
    }

    #[test]
    fn export_format_parses_case_insensitively() {
        assert_eq!(
            <ExportFormat as FromStr>::from_str("ZSH"),
            Ok(ExportFormat::Zsh)
        );
        assert!(<ExportFormat as FromStr>::from_str("fish").is_err());
    }

    #[test]
    fn export_format_prefers_explicit_format() {
        let args = OutputArgs {
            format: Some(ExportFormat::Json),
            runtime_dir: None,
        };
        assert_eq!(args.export_format(), ExportFormat::Json);
    }

    #[test]
    fn export_rejects_cached_with_refresh() {
        let result = Program::try_parse_from(["zsh-op", "export", "--cached", "--refresh"]);
        assert!(result.is_err());
    }

    #[test]
    fn secret_parses_op_secret_style_flags() {
        let program = Program::try_parse_from([
            "zsh-op", "secret", "-p", "work", "-x", "-e", "8h", "API_KEY",
        ])
        .unwrap();
        let ProgramCommand::Secret(args) = program.command else {
            panic!("expected the secret command");
        };
        assert_eq!(args.name, "API_KEY");
        assert_eq!(args.profile, "work");
        assert_eq!(args.expiration, "8h");
        assert!(args.export);
    }
}
