//! Argument grammar for the three Brain data commands.
use crate::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};

pub(super) enum Parsed {
    /// Canonical command arguments without the program name.
    Arguments(Vec<String>),
    /// Help or parser diagnostics have already been written to the proper stream.
    Exit(i32),
}

fn help_requested(args: &[String]) -> Option<usize> {
    let end = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    args[..end]
        .iter()
        .position(|arg| matches!(arg.as_str(), "-h" | "--help"))
}

fn print_help(tool: &str, args: &[String]) -> bool {
    let Some(index) = help_requested(args) else {
        return false;
    };
    if tool == "nacre-brain-sync" {
        print!(
            "usage: nacre-brain-sync [-h] [--exchange] [--force] [--config CONFIG]\n\nThree-way private vault synchronization; never propagate deletions.\n\noptions:\n  -h, --help       show this help message and exit\n  --exchange\n  --force          retry immediately despite offline backoff\n  --config CONFIG\n"
        );
        return true;
    }
    if tool == "nacre-brain-maintain" {
        if index == 0 {
            print!(
                "usage: nacre-brain-maintain [-h]\n                            {{check,backup,setup,read,update,restore,resolve}} ...\n\nCheck, version, update and restore the private brain without model calls.\n\npositional arguments:\n  {{check,backup,setup,read,update,restore,resolve}}\n\noptions:\n  -h, --help            show this help message and exit\n"
            );
        } else {
            match args.first().map(String::as_str) {
                Some("check" | "backup" | "setup") => {
                    let command = &args[0];
                    print!(
                        "usage: nacre-brain-maintain {command} [-h]\n\noptions:\n  -h, --help  show this help message and exit\n"
                    );
                }
                Some("read") => print!(
                    "usage: nacre-brain-maintain read [-h] path\n\npositional arguments:\n  path\n\noptions:\n  -h, --help  show this help message and exit\n"
                ),
                Some("update") => print!(
                    "usage: nacre-brain-maintain update [-h] --expected-sha256 EXPECTED_SHA256\n                                   path\n\npositional arguments:\n  path\n\noptions:\n  -h, --help            show this help message and exit\n  --expected-sha256 EXPECTED_SHA256\n                        Hash from read, or new for a new page\n"
                ),
                Some("restore") => print!(
                    "usage: nacre-brain-maintain restore [-h] archive destination\n\npositional arguments:\n  archive\n  destination\n\noptions:\n  -h, --help  show this help message and exit\n"
                ),
                Some("resolve") => print!(
                    "usage: nacre-brain-maintain resolve [-h] conflict_id\n\npositional arguments:\n  conflict_id\n\noptions:\n  -h, --help  show this help message and exit\n"
                ),
                _ => println!("usage: nacre-brain-maintain [command] [-h]"),
            }
        }
        return true;
    }

    if index == 0 {
        print!(
            "usage: nacre-brain [-h]\n                    {{init,path,search,note,store-credential,run-with-credential}} ...\n\nSmall, account-independent Markdown knowledge vault and credential references.\n\npositional arguments:\n  {{init,path,search,note,store-credential,run-with-credential}}\n    note                Read a Markdown note from stdin; write a unique dated file\n    store-credential    Prompt privately and store a credential in the OS keyring\n    run-with-credential\n                        Supply a keyring secret directly to a child environment\n\noptions:\n  -h, --help            show this help message and exit\n"
        );
    } else {
        match args.first().map(String::as_str) {
            Some("init" | "path") => {
                let command = &args[0];
                print!(
                    "usage: nacre-brain {command} [-h]\n\noptions:\n  -h, --help  show this help message and exit\n"
                );
            }
            Some("search") => print!(
                "usage: nacre-brain search [-h] query [query ...]\n\npositional arguments:\n  query\n\noptions:\n  -h, --help  show this help message and exit\n"
            ),
            Some("note") => print!(
                "usage: nacre-brain note [-h] [--kind {{inbox,personal,projects,infrastructure,devices,runbooks,decisions,credentials,checkpoints}}]\n                       --title TITLE --source SOURCE\n                       [--confidence {{verified,reported,unverified}}]\n\noptions:\n  -h, --help            show this help message and exit\n  --kind {{inbox,personal,projects,infrastructure,devices,runbooks,decisions,credentials,checkpoints}}\n  --title TITLE\n  --source SOURCE\n  --confidence {{verified,reported,unverified}}\n"
            ),
            Some("run-with-credential") => print!(
                "usage: nacre-brain run-with-credential [-h] --env ENV name ...\n\npositional arguments:\n  name\n  program\n\noptions:\n  -h, --help  show this help message and exit\n  --env ENV\n"
            ),
            Some("store-credential") => {
                print!(
                    "usage: nacre-brain store-credential [-h] name\n\npositional arguments:\n  name\n\noptions:\n  -h, --help  show this help message and exit\n"
                )
            }
            Some(command) => println!("usage: nacre-brain {command} [-h]"),
            None => println!("usage: nacre-brain [-h]"),
        }
    }
    true
}

fn brain_command() -> Command {
    let help = Arg::new("help")
        .short('h')
        .long("help")
        .action(ArgAction::Help);
    let note = Command::new("note")
        .about("Read a Markdown note from stdin; write a unique dated file")
        .arg(help.clone())
        .arg(
            Arg::new("kind")
                .long("kind")
                .value_parser([
                    "inbox",
                    "personal",
                    "projects",
                    "infrastructure",
                    "devices",
                    "runbooks",
                    "decisions",
                    "credentials",
                    "checkpoints",
                ])
                .default_value("inbox"),
        )
        .arg(Arg::new("title").long("title").required(true))
        .arg(Arg::new("source").long("source").required(true))
        .arg(
            Arg::new("confidence")
                .long("confidence")
                .value_parser(["verified", "reported", "unverified"])
                .default_value("reported"),
        );
    let store = Command::new("store-credential")
        .about("Prompt privately and store a credential in the OS keyring")
        .arg(help.clone())
        .arg(Arg::new("name").required(true));
    let run = Command::new("run-with-credential")
        .about("Supply a keyring secret directly to a child environment")
        .arg(help.clone())
        .arg(Arg::new("name").required(true))
        .arg(Arg::new("env").long("env").required(true).num_args(1))
        .arg(
            Arg::new("program")
                .num_args(0..)
                .trailing_var_arg(true)
                .allow_hyphen_values(true),
        );
    Command::new("nacre-brain")
        .disable_help_flag(true)
        .arg(help)
        .subcommand(
            Command::new("init").arg(
                Arg::new("help")
                    .short('h')
                    .long("help")
                    .action(ArgAction::Help),
            ),
        )
        .subcommand(
            Command::new("path").arg(
                Arg::new("help")
                    .short('h')
                    .long("help")
                    .action(ArgAction::Help),
            ),
        )
        .subcommand(
            Command::new("search")
                .arg(
                    Arg::new("help")
                        .short('h')
                        .long("help")
                        .action(ArgAction::Help),
                )
                .arg(Arg::new("query").required(true).num_args(1..)),
        )
        .subcommand(note)
        .subcommand(store)
        .subcommand(run)
}

fn sync_command() -> Command {
    Command::new("nacre-brain-sync")
        .disable_help_flag(true)
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .action(ArgAction::Help),
        )
        .arg(
            Arg::new("exchange")
                .long("exchange")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
        .arg(
            Arg::new("config")
                .long("config")
                .num_args(1)
                .value_name("CONFIG"),
        )
}

fn maintain_command() -> Command {
    let help = || {
        Arg::new("help")
            .short('h')
            .long("help")
            .action(ArgAction::Help)
    };
    let update = Command::new("update")
        .arg(help())
        .arg(Arg::new("path").required(true))
        .arg(
            Arg::new("expected-sha256")
                .long("expected-sha256")
                .required(true)
                .help("Hash from read, or new for a new page"),
        );
    let read = Command::new("read")
        .arg(help())
        .arg(Arg::new("path").required(true));
    let restore = Command::new("restore")
        .arg(help())
        .arg(Arg::new("archive").required(true))
        .arg(Arg::new("destination").required(true));
    let resolve = Command::new("resolve")
        .arg(help())
        .arg(Arg::new("conflict_id").required(true));
    Command::new("nacre-brain-maintain")
        .subcommand_required(true)
        .disable_help_flag(true)
        .arg(help())
        .subcommand(Command::new("check").arg(help()))
        .subcommand(Command::new("backup").arg(help()))
        .subcommand(Command::new("setup").arg(help()))
        .subcommand(read)
        .subcommand(update)
        .subcommand(restore)
        .subcommand(resolve)
}

fn brain_arguments(matches: &ArgMatches) -> Vec<String> {
    let Some((command, values)) = matches.subcommand() else {
        return Vec::new();
    };
    let mut result = vec![command.to_owned()];
    match command {
        "search" => result.extend(
            values
                .get_many::<String>("query")
                .into_iter()
                .flatten()
                .cloned(),
        ),
        "note" => {
            for (key, flag) in [
                ("kind", "--kind"),
                ("title", "--title"),
                ("source", "--source"),
                ("confidence", "--confidence"),
            ] {
                if let Some(value) = values.get_one::<String>(key) {
                    // Preserve parser defaults because the command implementation
                    // uses the same defaults when options are omitted.
                    if !matches!(
                        (key, value.as_str()),
                        ("kind", "inbox") | ("confidence", "reported")
                    ) {
                        result.extend([flag.to_owned(), value.clone()]);
                    }
                }
            }
        }
        "store-credential" => {
            if let Some(name) = values.get_one::<String>("name") {
                result.push(name.clone());
            }
        }
        "run-with-credential" => {
            if let Some(name) = values.get_one::<String>("name") {
                result.push(name.clone());
            }
            if let Some(env) = values.get_one::<String>("env") {
                result.extend(["--env".to_owned(), env.clone()]);
            }
            if let Some(program) = values.get_many::<String>("program") {
                result.push("--".into());
                result.extend(program.cloned());
            }
        }
        _ => {}
    }
    result
}

fn sync_arguments(matches: &ArgMatches) -> Vec<String> {
    let mut result = Vec::new();
    if matches.get_flag("exchange") {
        result.push("--exchange".into());
    }
    if matches.get_flag("force") {
        result.push("--force".into());
    }
    if let Some(path) = matches.get_one::<String>("config") {
        result.extend(["--config".into(), path.clone()]);
    }
    result
}

fn maintain_arguments(matches: &ArgMatches) -> Vec<String> {
    let Some((command, values)) = matches.subcommand() else {
        return Vec::new();
    };
    let mut result = vec![command.to_owned()];
    match command {
        "read" => result.extend(values.get_one::<String>("path").cloned()),
        "update" => {
            result.extend(values.get_one::<String>("path").cloned());
            if let Some(hash) = values.get_one::<String>("expected-sha256") {
                result.extend(["--expected-sha256".into(), hash.clone()]);
            }
        }
        "restore" => {
            result.extend(values.get_one::<String>("archive").cloned());
            result.extend(values.get_one::<String>("destination").cloned());
        }
        "resolve" => result.extend(values.get_one::<String>("conflict_id").cloned()),
        _ => {}
    }
    result
}

fn parse_command(tool: &str, args: &[String]) -> Result<(Command, Vec<String>)> {
    let command = match tool {
        "nacre-brain" => brain_command(),
        "nacre-brain-sync" => sync_command(),
        "nacre-brain-maintain" => maintain_command(),
        _ => return Err(format!("Unknown Brain command: {tool}").into()),
    };
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(tool.to_owned());
    argv.extend(args.iter().cloned());
    Ok((command, argv))
}

pub(super) fn parse(tool: &str, args: &[String]) -> Result<Parsed> {
    let (command, argv) = parse_command(tool, args)?;
    match command.try_get_matches_from(argv) {
        Ok(matches) => {
            let arguments = match tool {
                "nacre-brain" => brain_arguments(&matches),
                "nacre-brain-sync" => sync_arguments(&matches),
                "nacre-brain-maintain" => maintain_arguments(&matches),
                _ => unreachable!(),
            };
            Ok(Parsed::Arguments(arguments))
        }
        Err(error) => {
            if error.kind() == clap::error::ErrorKind::DisplayHelp && print_help(tool, args) {
                return Ok(Parsed::Exit(0));
            }
            let code = if error.use_stderr() { 2 } else { 0 };
            error.print()?;
            Ok(Parsed::Exit(code))
        }
    }
}
