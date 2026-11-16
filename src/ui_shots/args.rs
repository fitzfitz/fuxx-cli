//! The screenshot run's command line: `fuxx __ui-shots [filter] [--png] [--all-themes]
//! [--shard I/N | --serial-only] [--changed <base>] [--update-refs]`.

/// What one run does, from its command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Run only the scenes whose name contains this.
    pub filter: Option<String>,
    /// Write each shot's PNG to `target/ui-shots/` (also `UI_SHOTS_PNG=1`).
    pub png: bool,
    /// Shoot every scene in both themes, `Themes::One` scenes too.
    pub all_themes: bool,
    /// Run the `I`th of `N` shares of the scenes that aren't `serial` (1-based).
    pub shard: Option<(usize, usize)>,
    /// Run the scenes whose sources changed since this git base.
    pub changed: Option<String>,
    /// Accept the shots' fingerprints as the new references.
    pub update_refs: bool,
    /// Run only the `serial` scenes (what `scripts/ui-shots.sh` runs after the shards).
    pub serial_only: bool,
}

/// The run's options from the arguments after `__ui-shots` (a leading `__ui-shots` is skipped).
/// The filter is the first argument that isn't a flag.
pub fn parse_args(args: &[String]) -> Result<RunOptions, String> {
    let mut o = RunOptions::default();
    let mut args = args.iter().map(String::as_str);
    let mut first = true;
    while let Some(arg) = args.next() {
        let leading = std::mem::replace(&mut first, false);
        match arg {
            "__ui-shots" if leading => {}
            "--png" => o.png = true,
            "--all-themes" => o.all_themes = true,
            "--update-refs" => o.update_refs = true,
            "--serial-only" => o.serial_only = true,
            "--shard" => {
                let v = args.next().ok_or("--shard needs I/N")?;
                o.shard = Some(shard(v)?);
            }
            "--changed" => {
                let v = args.next().ok_or("--changed needs a git base")?;
                o.changed = Some(v.to_string());
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            filter if o.filter.is_none() => o.filter = Some(filter.to_string()),
            extra => return Err(format!("one filter only (got a second: {extra})")),
        }
    }
    if o.serial_only && o.shard.is_some() {
        return Err("--serial-only and --shard: a shard never runs the serial scenes".into());
    }
    Ok(o)
}

/// `I/N` with 1 <= I <= N.
fn shard(v: &str) -> Result<(usize, usize), String> {
    let bad = || format!("--shard {v}: expected I/N with 1 <= I <= N");
    let (i, n) = v.split_once('/').ok_or_else(bad)?;
    let (i, n): (usize, usize) = (i.parse().map_err(|_| bad())?, n.parse().map_err(|_| bad())?);
    if i == 0 || i > n {
        return Err(bad());
    }
    Ok((i, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<RunOptions, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse_args(&args)
    }

    #[test]
    fn no_arguments_are_the_defaults() {
        assert_eq!(parse(&["__ui-shots"]), Ok(RunOptions::default()));
        assert_eq!(parse(&[]), Ok(RunOptions::default()));
    }

    #[test]
    fn a_filter_and_png() {
        let o = parse(&["file-tab", "--png"]).unwrap();
        assert_eq!(o.filter.as_deref(), Some("file-tab"));
        assert!(o.png);
        assert!(!o.all_themes);
        assert_eq!(parse(&["--png", "file-tab"]), Ok(o));
    }

    #[test]
    fn a_shard_is_i_of_n() {
        let o = parse(&["--shard", "3/8"]).unwrap();
        assert_eq!(o.shard, Some((3, 8)));
        assert_eq!(o.filter, None);
    }

    #[test]
    fn serial_only_combines_with_a_filter_but_not_a_shard() {
        let o = parse(&["--serial-only", "file-tab"]).unwrap();
        assert!(o.serial_only);
        assert_eq!(o.filter.as_deref(), Some("file-tab"));
        assert!(!parse(&[]).unwrap().serial_only);
        assert!(
            parse(&["--serial-only", "--shard", "1/2"]).is_err(),
            "a shard never runs the serial scenes"
        );
    }

    #[test]
    fn a_shard_out_of_range_or_malformed_is_an_error() {
        for bad in ["9/8", "0/8", "1/0", "3", "a/8", "3/"] {
            assert!(parse(&["--shard", bad]).is_err(), "--shard {bad}");
        }
        assert!(parse(&["--shard"]).is_err(), "--shard with no value");
    }

    #[test]
    fn changed_takes_a_base_and_combines_with_all_themes() {
        let o = parse(&["--changed", "master", "--all-themes"]).unwrap();
        assert_eq!(o.changed.as_deref(), Some("master"));
        assert!(o.all_themes);
        assert_eq!(o.filter, None);
        assert!(parse(&["--changed"]).is_err(), "--changed with no base");
    }

    #[test]
    fn update_refs_and_unknown_flags_and_a_second_filter() {
        assert!(parse(&["--update-refs"]).unwrap().update_refs);
        assert!(parse(&["--pngs"]).is_err(), "an unknown flag");
        assert!(parse(&["smoke", "tokens"]).is_err(), "two filters");
    }
}
