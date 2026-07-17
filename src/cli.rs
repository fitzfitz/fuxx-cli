#[derive(Debug)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

pub fn parse<I: IntoIterator<Item = String>>(raw: I) -> Result<Launch, String> {
    let mut it = raw.into_iter();
    it.next(); // skip argv[0] (the fuxx binary path)
    let program = it
        .next()
        .ok_or_else(|| "usage: fuxx <command> [args...]".to_string())?;
    let args: Vec<String> = it.collect();
    Ok(Launch { program, args })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_and_args() {
        let launch = parse(["fuxx", "echo", "hello"].map(String::from)).unwrap();
        assert_eq!(launch.program, "echo");
        assert_eq!(launch.args, vec!["hello".to_string()]);
    }

    #[test]
    fn no_command_is_usage_error() {
        let err = parse(["fuxx"].map(String::from)).unwrap_err();
        assert!(err.contains("usage"), "message was: {err:?}");
    }
}
