// Copyright 2020 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io::Write;
use std::slice;
use std::time::{Duration, SystemTime};

use clap::Subcommand;
use clap_complete::Shell;
use jj_lib::repo::Repo;
use tracing::instrument;

use crate::cli_util::{user_error, CommandError, CommandHelper};
use crate::ui::Ui;

/// Infrequently used commands such as for generating shell completions
#[derive(Subcommand, Clone, Debug)]
pub(crate) enum UtilCommand {
    Completion(UtilCompletionArgs),
    Gc(UtilGcArgs),
    Mangen(UtilMangenArgs),
    MarkdownHelp(UtilMarkdownHelp),
    ConfigSchema(UtilConfigSchemaArgs),
}

// Using an explicit `doc` attribute prevents rustfmt from mangling the list
// formatting without disabling rustfmt for the entire struct.
#[doc = r#"Print a command-line-completion script

Apply it by running one of these:

- **bash**: `source <(jj util completion)`
- **fish**: `jj util completion --fish | source`
- **zsh**:
     ```shell
     autoload -U compinit
     compinit
     source <(jj util completion --zsh)
     ```
"#]
#[derive(clap::Args, Clone, Debug)]
#[command(verbatim_doc_comment)]
pub(crate) struct UtilCompletionArgs {
    shell: Option<Shell>,
    /// Deprecated. Use the SHELL positional argument instead.
    #[arg(long, hide = true)]
    bash: bool,
    /// Deprecated. Use the SHELL positional argument instead.
    #[arg(long, hide = true)]
    fish: bool,
    /// Deprecated. Use the SHELL positional argument instead.
    #[arg(long, hide = true)]
    zsh: bool,
}

/// Run backend-dependent garbage collection.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct UtilGcArgs {
    /// Time threshold
    ///
    /// By default, only obsolete objects and operations older than 2 weeks are
    /// pruned.
    ///
    /// Only the string "now" can be passed to this parameter. Support for
    /// arbitrary absolute and relative timestamps will come in a subsequent
    /// release.
    #[arg(long)]
    expire: Option<String>,
}

/// Print a ROFF (manpage)
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct UtilMangenArgs {}

/// Print the CLI help for all subcommands in Markdown
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct UtilMarkdownHelp {}

/// Print the JSON schema for the jj TOML config format.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct UtilConfigSchemaArgs {}

#[instrument(skip_all)]
pub(crate) fn cmd_util(
    ui: &mut Ui,
    command: &CommandHelper,
    subcommand: &UtilCommand,
) -> Result<(), CommandError> {
    match subcommand {
        UtilCommand::Completion(args) => cmd_util_completion(ui, command, args),
        UtilCommand::Gc(args) => cmd_util_gc(ui, command, args),
        UtilCommand::Mangen(args) => cmd_util_mangen(ui, command, args),
        UtilCommand::MarkdownHelp(args) => cmd_util_markdownhelp(ui, command, args),
        UtilCommand::ConfigSchema(args) => cmd_util_config_schema(ui, command, args),
    }
}

fn cmd_util_completion(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &UtilCompletionArgs,
) -> Result<(), CommandError> {
    let mut app = command.app().clone();
    let warn = |shell| {
        writeln!(
            ui.warning(),
            "Warning: `jj util completion --{shell}` will be removed in a future version, and \
             this will be a hard error"
        )?;
        writeln!(ui.hint(), "Hint: Use `jj util completion {shell}` instead")
    };
    let mut buf = vec![];
    let shell = match (args.shell, args.fish, args.zsh, args.bash) {
        (Some(s), false, false, false) => s,
        // allow `--fish` and `--zsh` for back-compat, but don't allow them to be combined
        (None, true, false, false) => {
            warn("fish")?;
            Shell::Fish
        }
        (None, false, true, false) => {
            warn("zsh")?;
            Shell::Zsh
        }
        // default to bash for back-compat. TODO: consider making `shell` a required argument
        (None, false, false, _) => {
            warn("bash")?;
            Shell::Bash
        }
        _ => {
            return Err(user_error(
                "cannot generate completion for multiple shells at once",
            ))
        }
    };
    clap_complete::generate(shell, &mut app, "jj", &mut buf);
    ui.stdout_formatter().write_all(&buf)?;
    Ok(())
}

fn cmd_util_gc(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &UtilGcArgs,
) -> Result<(), CommandError> {
    if command.global_args().at_operation != "@" {
        return Err(user_error(
            "Cannot garbage collect from a non-head operation",
        ));
    }
    let keep_newer = match args.expire.as_deref() {
        None => SystemTime::now() - Duration::from_secs(14 * 86400),
        Some("now") => SystemTime::now() - Duration::ZERO,
        _ => return Err(user_error("--expire only accepts 'now'")),
    };
    let workspace_command = command.workspace_helper(ui)?;

    let repo = workspace_command.repo();
    repo.op_store()
        .gc(slice::from_ref(repo.op_id()), keep_newer)?;
    repo.store().gc(repo.index(), keep_newer)?;
    Ok(())
}

fn cmd_util_mangen(
    ui: &mut Ui,
    command: &CommandHelper,
    _args: &UtilMangenArgs,
) -> Result<(), CommandError> {
    let mut buf = vec![];
    let man = clap_mangen::Man::new(command.app().clone());
    man.render(&mut buf)?;
    ui.stdout_formatter().write_all(&buf)?;
    Ok(())
}

fn cmd_util_markdownhelp(
    ui: &mut Ui,
    command: &CommandHelper,
    _args: &UtilMarkdownHelp,
) -> Result<(), CommandError> {
    // If we ever need more flexibility, the code of `clap_markdown` is simple and
    // readable. We could reimplement the parts we need without trouble.
    let markdown = clap_markdown::help_markdown_command(command.app()).into_bytes();
    ui.stdout_formatter().write_all(&markdown)?;
    Ok(())
}

fn cmd_util_config_schema(
    ui: &mut Ui,
    _command: &CommandHelper,
    _args: &UtilConfigSchemaArgs,
) -> Result<(), CommandError> {
    // TODO(#879): Consider generating entire schema dynamically vs. static file.
    let buf = include_bytes!("../config-schema.json");
    ui.stdout_formatter().write_all(buf)?;
    Ok(())
}
