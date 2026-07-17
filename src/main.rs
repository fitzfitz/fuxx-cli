mod cli;

fn main() {
    match cli::parse(std::env::args()) {
        Ok(launch) => println!("would launch: {} {:?}", launch.program, launch.args),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
