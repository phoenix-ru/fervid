use std::{
    borrow::Cow,
    env,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use fervid::{CompileOptions, compile};

const USAGE: &str = concat!(
    "Usage: fervid [--mode <dev|prod>] <INPUT>\n",
    "\n",
    "Options:\n",
    "  --mode <dev|prod>  Compilation mode [default: dev]\n",
    "  -h, --help         Print help\n",
    "  -V, --version      Print version",
);

#[derive(Clone, Copy)]
enum Mode {
    Dev,
    Prod,
}

struct CliOptions {
    input: PathBuf,
    mode: Mode,
}

enum Action {
    Compile(CliOptions),
    Help,
    Version,
}

fn main() -> ExitCode {
    let action = match parse_args(env::args_os().skip(1)) {
        Ok(action) => action,
        Err(message) => {
            eprintln!("fervid: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match action {
        Action::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("fervid {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Action::Compile(options) => match compile_file(options) {
            Ok(code) => {
                if let Err(error) = io::stdout().lock().write_all(code.as_bytes()) {
                    eprintln!("fervid: cannot write output: {error}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("fervid: {message}");
                ExitCode::FAILURE
            }
        },
    }
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Action, String> {
    let mut mode = Mode::Dev;
    let mut input = None;
    let mut positional_only = false;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        if !positional_only {
            if argument == "--" {
                positional_only = true;
                continue;
            }
            if argument == "-h" || argument == "--help" {
                return Ok(Action::Help);
            }
            if argument == "-V" || argument == "--version" {
                return Ok(Action::Version);
            }
            if argument == "--mode" {
                let Some(value) = args.next() else {
                    return Err("missing value for '--mode'".into());
                };
                mode = parse_mode(&value)?;
                continue;
            }
            if let Some(argument) = argument.to_str()
                && let Some(value) = argument.strip_prefix("--mode=")
            {
                mode = parse_mode(&OsString::from(value))?;
                continue;
            }
            if argument.to_string_lossy().starts_with('-') {
                return Err(format!("unknown option '{}'", argument.to_string_lossy()));
            }
        }

        if input.replace(PathBuf::from(argument)).is_some() {
            return Err("expected exactly one input file".into());
        }
    }

    let Some(input) = input else {
        return Err("missing input file".into());
    };

    Ok(Action::Compile(CliOptions { input, mode }))
}

fn parse_mode(value: &OsString) -> Result<Mode, String> {
    match value.to_str() {
        Some("dev") => Ok(Mode::Dev),
        Some("prod") => Ok(Mode::Prod),
        _ => Err(format!(
            "invalid mode '{}'; expected 'dev' or 'prod'",
            value.to_string_lossy()
        )),
    }
}

fn compile_file(options: CliOptions) -> Result<String, String> {
    let source = fs::read_to_string(&options.input)
        .map_err(|error| format!("cannot read '{}': {error}", options.input.to_string_lossy()))?;
    let filename = options
        .input
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
        .unwrap_or_else(|| options.input.to_string_lossy().into_owned());
    let input_path = options.input.to_string_lossy().into_owned();

    let result = compile(
        &source,
        CompileOptions {
            filename: Cow::Owned(filename),
            id: Cow::Owned(input_path.clone()),
            is_prod: Some(matches!(options.mode, Mode::Prod)),
            is_custom_element: Some(false),
            ssr: Some(false),
            props_destructure: None,
            transform_asset_urls: None,
            gen_default_as: None,
            source_map: Some(false),
        },
    )
    .map_err(|error| format!("{input_path}: {error}"))?;

    if !result.errors.is_empty() {
        let diagnostics = result
            .errors
            .iter()
            .map(|error| format!("{input_path}: {error}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(diagnostics);
    }

    Ok(result.code)
}
