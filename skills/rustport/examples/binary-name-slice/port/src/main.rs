use openmmd_name_slice::validate_name;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(name) = args.next() else {
        eprintln!("Usage: openmmd-name-slice NAME [--allow-empty]");
        return ExitCode::from(2);
    };
    let allow_empty = match (args.next().as_deref(), args.next()) {
        (None, None) => false,
        (Some("--allow-empty"), None) => true,
        _ => {
            eprintln!("Usage: openmmd-name-slice NAME [--allow-empty]");
            return ExitCode::from(2);
        }
    };
    match validate_name(&name, allow_empty) {
        Ok(()) => {
            println!("valid");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
