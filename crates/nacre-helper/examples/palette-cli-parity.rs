//! Developer-only historical argv[0] name for frozen module invocation output.
fn main() {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Missing historical program name");
        std::process::exit(1);
    }
    let program = args.remove(0);
    match nacre_helper::palette_cli::run_with_program(&args, &program, false) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
