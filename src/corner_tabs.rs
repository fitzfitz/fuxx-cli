//! The corner tabs' shared words (chore 34, note 93): a tab whose work is all finished (🤖
//! subagents, ▣ shells, ☰ plan) shows only its icon, in green; its tooltip says what the missing
//! count would: "10 of 10 subagents done".

/// The tooltip of a finished tab: `all` of `all` `one`/`many` done.
pub fn done_tip(all: usize, one: &str, many: &str) -> String {
    let noun = if all == 1 { one } else { many };
    format!("{all} of {all} {noun} done")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_tab_says_how_many_are_done() {
        assert_eq!(
            done_tip(10, "subagent", "subagents"),
            "10 of 10 subagents done"
        );
        assert_eq!(done_tip(3, "shell", "shells"), "3 of 3 shells done");
        assert_eq!(done_tip(1, "task", "tasks"), "1 of 1 task done");
    }
}
