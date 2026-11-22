//! The agent tab's subagents (M11ar): following a subagent's transcript (agy writes each
//! subagent's steps to its own file), and — from Task 4 on — the pill strip and the lane view.

use super::*;

/// How often a followed transcript is read.
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

impl AgentTab {
    /// `Event::Follow`: read `path`'s new lines as lane `lane`'s, every `FOLLOW_EVERY`, off the
    /// UI thread, through the translator. It lives as long as the process (a woken subagent
    /// writes again after its report); a file that shrinks ends it.
    pub(super) fn follow(&mut self, lane: String, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.spawn({
            let lane = lane.clone();
            async move |this, cx| {
                let mut tail = crate::lane_follow::Tail::default();
                loop {
                    cx.background_executor().timer(FOLLOW_EVERY).await;
                    let (at, file) = (tail.offset, path.clone());
                    let read = cx
                        .background_executor()
                        .spawn(async move {
                            crate::lane_follow::read_new(&file, at, crate::lane_follow::MAX_READ)
                        })
                        .await;
                    let Ok(bytes) = read else { break };
                    let lines = tail.feed(&bytes);
                    if lines.is_empty() {
                        continue;
                    }
                    let alive = this
                        .update(cx, |this, cx| this.on_followed(&lane, lines, cx))
                        .is_ok();
                    if !alive {
                        break;
                    }
                }
            }
        });
        self.followers.insert(lane, task);
    }

    /// A followed file's new lines for `lane`, through the translator, as events.
    pub(super) fn on_followed(&mut self, lane: &str, lines: Vec<String>, cx: &mut Context<Self>) {
        let Some(translator) = self.translator.as_mut() else {
            return;
        };
        let events: Vec<Event> = lines
            .iter()
            .flat_map(|l| translator.decode_followed(lane, l))
            .collect();
        if !events.is_empty() {
            self.on_events(events, cx);
        }
    }
}
