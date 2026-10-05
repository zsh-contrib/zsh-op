mod app;
mod log;
mod vault;

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{ExitCode, ExitStatus};

use crate::app::cli::*;
use crate::app::cmd::*;
use crate::vault::*;

use anyhow::Result;
use clap::Parser;

fn main() -> ExitCode {
    let program = Program::parse();
    // Process the correct command
    match run(program) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("zsh-op: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(program: Program) -> Result<ExitCode> {
    match program.command {
        ProgramCommand::Inspect(args) => {
            let writer = Box::new(std::io::stdout());
            let cache = cache(&args.parent.cache_dir);
            let mut command = InspectCommand { writer, cache };
            command.execute(&args)?
        }
        ProgramCommand::List(args) => {
            let writer = Box::new(std::io::stdout());
            let mut command = ListCommand { writer };
            command.execute(&args)?
        }
        ProgramCommand::Shell(args) => {
            let writer = Box::new(std::io::stdout());
            let loader = loader(&args.parent.cache_dir);
            let agent = Box::new(Agent::new());
            let mut command = ShellCommand {
                writer,
                loader,
                agent,
            };
            command.execute(&args)?
        }
        ProgramCommand::Secret(args) => {
            let writer = Box::new(std::io::stdout());
            let loader = loader(&args.parent.cache_dir);
            let agent = Box::new(Agent::new());
            let mut command = SecretCommand {
                writer,
                loader,
                agent,
            };
            command.execute(&args)?
        }
        ProgramCommand::Export(args) => {
            let writer = Box::new(std::io::stdout());
            let loader = loader(&args.parent.cache_dir);
            let mut command = ExportCommand { writer, loader };
            command.execute(&args)?
        }
        ProgramCommand::Exec(args) => {
            let loader = loader(&args.parent.cache_dir);
            let mut command = ExecCommand { loader };
            return Ok(exit_code(command.execute(&args)?));
        }
        ProgramCommand::Clear(args) => {
            let cache = cache(&args.parent.cache_dir);
            let mut command = ClearCommand { cache };
            command.execute(&args)?
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Returns the keychain-backed cache keeping its metadata in `dir`.
fn cache(dir: &Path) -> Cache {
    Cache::new(Box::new(Keychain::new()), dir)
}

/// Returns a loader reading from the keychain cache and the 1Password CLI.
fn loader(dir: &Path) -> Loader {
    Loader {
        client: Box::new(Client::new()),
        cache: cache(dir),
    }
}

/// Maps the exit status of a child process to our own, shell style.
fn exit_code(status: ExitStatus) -> ExitCode {
    match (status.code(), status.signal()) {
        (Some(code), _) => ExitCode::from(code as u8),
        (None, Some(signal)) => ExitCode::from(128 + signal as u8),
        (None, None) => ExitCode::FAILURE,
    }
}
