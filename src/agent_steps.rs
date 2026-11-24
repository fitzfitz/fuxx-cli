//! The step model (the agent activity chore): a narration line and the run of tool calls after
//! it, as the activity view draws it. Pure and GPUI-free.

use std::collections::HashMap;
use std::time::Instant;

/// Each tool call's live start and finish (None while it runs), by its id. History never
/// enters it, so restored calls have no times.
pub type ToolTimes = HashMap<String, (Instant, Option<Instant>)>;
