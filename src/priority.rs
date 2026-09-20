//! Priority-session grouping, display retention and activity ranks.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attention {
    Blocked,
    UnreadDone,
    Working,
    None,
}

impl Attention {
    pub fn of(status: Option<&str>, unread: bool) -> Self {
        match status {
            Some("blocked") => Self::Blocked,
            Some("done") if unread => Self::UnreadDone,
            Some("working") => Self::Working,
            _ => Self::None,
        }
    }

    pub fn is_priority(self) -> bool {
        matches!(self, Self::Blocked | Self::UnreadDone)
    }
}

#[derive(Clone, Debug)]
pub struct OwnedSession {
    pub device: String,
    pub pane: String,
    pub status: Option<String>,
    pub unread: bool,
    pub seq: Option<u64>,
    pub launch_pending: bool,
}

impl OwnedSession {
    pub fn info(&self) -> SessionInfo<'_> {
        SessionInfo {
            device: &self.device,
            pane: &self.pane,
            status: self.status.as_deref(),
            unread: self.unread,
            seq: self.seq,
            launch_pending: self.launch_pending,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SessionInfo<'a> {
    pub device: &'a str,
    pub pane: &'a str,
    pub status: Option<&'a str>,
    pub unread: bool,
    pub seq: Option<u64>,
    pub launch_pending: bool,
}

impl SessionInfo<'_> {
    pub fn attention(self) -> Attention {
        Attention::of(self.status, self.unread)
    }

    fn key(self) -> (String, String) {
        (self.device.to_owned(), self.pane.to_owned())
    }
}

#[derive(Clone, Debug, Default)]
pub struct PriorityState {
    hold: Option<((String, String), Attention)>,
    handled: Vec<(String, String)>,
    loading_order: HashMap<(String, String), u64>,
    next_loading_order: u64,
    baseline: HashMap<String, HashMap<String, (String, Option<u64>)>>,
    starting: HashSet<(String, String)>,
}

impl PriorityState {
    pub fn is_starting(&self, device: &str, pane: &str) -> bool {
        self.starting
            .contains(&(device.to_owned(), pane.to_owned()))
    }

    pub fn note_starting(&mut self, device: &str, pane: &str) {
        self.starting.insert((device.to_owned(), pane.to_owned()));
        self.handled.retain(|(d, p)| d != device || p != pane);
        if self
            .hold
            .as_ref()
            .is_some_and(|(key, _)| key.0 == device && key.1 == pane)
        {
            self.hold = None;
        }
    }

    pub fn is_loading(&self, info: SessionInfo<'_>) -> bool {
        self.is_starting(info.device, info.pane)
            || info.status == Some("working")
            || info.launch_pending
    }

    pub fn group_attention(&self, info: SessionInfo<'_>) -> Attention {
        let attention = info.attention();
        if attention == Attention::Blocked {
            return attention;
        }
        if self.is_loading(info) {
            return Attention::None;
        }
        if let Some((key, held)) = &self.hold {
            if key.0 == info.device && key.1 == info.pane {
                return *held;
            }
        }
        attention
    }

    pub fn is_priority_group(&self, info: SessionInfo<'_>) -> bool {
        self.group_attention(info).is_priority()
    }

    pub fn note_selection(
        &mut self,
        enabled: bool,
        device: &str,
        previous: Option<&str>,
        selected: Option<&str>,
        sessions: &[OwnedSession],
    ) {
        if let Some((key, _)) = &self.hold {
            if selected.is_some_and(|pane| key.0 == device && key.1 == pane) {
                return;
            }
        }
        if previous != selected {
            let hold_pane = self
                .hold
                .as_ref()
                .filter(|(key, _)| key.0 == device)
                .map(|(key, _)| key.1.clone());
            let was_held = hold_pane.is_some();
            let pane = hold_pane.or_else(|| previous.map(str::to_owned));
            if self.hold.as_ref().is_some_and(|(key, _)| key.0 == device) {
                self.hold = None;
            }
            if let Some(pane) = pane {
                if let Some(info) = sessions
                    .iter()
                    .find(|session| session.device == device && session.pane == pane)
                    .map(OwnedSession::info)
                {
                    if was_held || (enabled && info.attention().is_priority()) {
                        if info.attention() != Attention::Blocked && !self.is_loading(info) {
                            let key = info.key();
                            self.handled.retain(|item| item != &key);
                            self.handled.insert(0, key);
                        }
                    }
                }
            }
        }
        let Some(pane) = selected else {
            return;
        };
        if !enabled {
            return;
        }
        let Some(info) = sessions
            .iter()
            .find(|session| session.device == device && session.pane == pane)
            .map(OwnedSession::info)
        else {
            return;
        };
        if !info.attention().is_priority() || self.is_loading(info) {
            return;
        }
        self.hold = Some((info.key(), info.attention()));
    }

    pub fn note_snapshot(&mut self, device: &str, agents: &[SessionInfo<'_>]) {
        let baseline = self.baseline.get(device).cloned();
        let reset = agents.iter().any(|agent| {
            let Some(old_seq) = baseline
                .as_ref()
                .and_then(|b| b.get(agent.pane))
                .and_then(|e| e.1)
            else {
                return false;
            };
            agent.seq.is_some_and(|seq| seq < old_seq)
        });
        if reset {
            self.loading_order.retain(|(d, _), _| d != device);
        }
        if let Some(baseline) = baseline.filter(|_| !reset) {
            let changes: Vec<&SessionInfo<'_>> = agents
                .iter()
                .filter(|agent| {
                    if self.is_starting(agent.device, agent.pane) && agent.status == Some("working")
                    {
                        return false;
                    }
                    let Some(old) = baseline.get(agent.pane) else {
                        return false;
                    };
                    old.0 != agent.status.unwrap_or("")
                        || old.1.is_some_and(|seq| agent.seq.unwrap_or(0) > seq)
                })
                .collect();
            let mut sequences: Vec<u64> = changes.iter().map(|a| a.seq.unwrap_or(0)).collect();
            sequences.sort_unstable();
            sequences.dedup();
            for seq in sequences {
                self.next_loading_order += 1;
                for agent in changes.iter().filter(|a| a.seq.unwrap_or(0) == seq) {
                    let key = agent.key();
                    self.loading_order
                        .insert(key.clone(), self.next_loading_order);
                    self.handled.retain(|item| item != &key);
                }
            }
        }
        self.baseline.insert(
            device.to_owned(),
            agents
                .iter()
                .map(|agent| {
                    (
                        agent.pane.to_owned(),
                        (agent.status.unwrap_or("").to_owned(), agent.seq),
                    )
                })
                .collect(),
        );
        for agent in agents {
            if agent.status == Some("working")
                && self
                    .hold
                    .as_ref()
                    .is_some_and(|(key, _)| key.0 == agent.device && key.1 == agent.pane)
            {
                self.hold = None;
            }
            if !self.is_loading(*agent) {
                self.starting
                    .remove(&(agent.device.to_owned(), agent.pane.to_owned()));
            }
        }
    }

    pub fn prune(&mut self, device: &str, live: &HashSet<String>) {
        self.handled
            .retain(|(d, p)| d != device || live.contains(p));
        self.loading_order
            .retain(|(d, p), _| d != device || live.contains(p));
        self.starting
            .retain(|(d, p)| d != device || live.contains(p));
        if let Some(baseline) = self.baseline.get_mut(device) {
            baseline.retain(|pane, _| live.contains(pane));
        }
        if let Some((key, _)) = &self.hold {
            if key.0 == device && !live.contains(&key.1) {
                self.hold = None;
            }
        }
    }

    pub fn order(&self, enabled: bool, items: &[SessionInfo<'_>]) -> Vec<usize> {
        if !enabled {
            return recent_indices(items, &self.loading_order);
        }
        let mut blocked = Vec::new();
        let mut unread = Vec::new();
        let mut other = Vec::new();
        for (index, info) in items.iter().enumerate() {
            match self.group_attention(*info) {
                Attention::Blocked => blocked.push(index),
                Attention::UnreadDone => unread.push(index),
                _ => other.push(index),
            }
        }
        let mut loading: Vec<usize> = other
            .iter()
            .copied()
            .filter(|&index| self.is_loading(items[index]))
            .collect();
        loading.sort_by(|&lhs, &rhs| {
            let left = self
                .loading_order
                .get(&(items[lhs].device.to_owned(), items[lhs].pane.to_owned()))
                .copied()
                .unwrap_or(0);
            let right = self
                .loading_order
                .get(&(items[rhs].device.to_owned(), items[rhs].pane.to_owned()))
                .copied()
                .unwrap_or(0);
            right.cmp(&left).then(lhs.cmp(&rhs))
        });
        let remaining: Vec<usize> = other
            .iter()
            .copied()
            .filter(|&index| !self.is_loading(items[index]))
            .collect();
        let mut handled = Vec::new();
        for key in &self.handled {
            if let Some(index) = remaining
                .iter()
                .copied()
                .find(|&index| items[index].device == key.0 && items[index].pane == key.1)
            {
                handled.push(index);
            }
        }
        let rest: Vec<usize> = remaining
            .into_iter()
            .filter(|&index| {
                !self
                    .handled
                    .iter()
                    .any(|key| items[index].device == key.0 && items[index].pane == key.1)
            })
            .collect();
        let mut order = blocked;
        order.extend(unread);
        order.extend(loading);
        order.extend(handled);
        order.extend(rest);
        order
    }
}

fn recent_indices(
    items: &[SessionInfo<'_>],
    loading_order: &HashMap<(String, String), u64>,
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..items.len()).collect();
    indices.sort_by(|&lhs, &rhs| {
        let left = loading_order
            .get(&(items[lhs].device.to_owned(), items[lhs].pane.to_owned()))
            .copied()
            .unwrap_or(0);
        let right = loading_order
            .get(&(items[rhs].device.to_owned(), items[rhs].pane.to_owned()))
            .copied()
            .unwrap_or(0);
        right.cmp(&left).then(lhs.cmp(&rhs))
    });
    indices
}

pub fn extra_seq(extra: &std::collections::BTreeMap<String, Value>) -> Option<u64> {
    extra
        .get("state_change_seq")
        .or_else(|| extra.get("stateChangeSeq"))
        .and_then(json_u64)
}

pub fn extra_launch_pending(extra: &std::collections::BTreeMap<String, Value>) -> bool {
    extra
        .get("launch_pending")
        .or_else(|| extra.get("launchPending"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info<'a>(
        pane: &'a str,
        status: Option<&'a str>,
        unread: bool,
        seq: Option<u64>,
    ) -> SessionInfo<'a> {
        SessionInfo {
            device: "d",
            pane,
            status,
            unread,
            seq,
            launch_pending: false,
        }
    }

    fn owned(item: SessionInfo<'_>) -> OwnedSession {
        OwnedSession {
            device: item.device.to_owned(),
            pane: item.pane.to_owned(),
            status: item.status.map(str::to_owned),
            unread: item.unread,
            seq: item.seq,
            launch_pending: item.launch_pending,
        }
    }

    fn owned_list(items: &[SessionInfo<'_>]) -> Vec<OwnedSession> {
        items.iter().copied().map(owned).collect()
    }

    fn ordered<'a>(
        state: &PriorityState,
        enabled: bool,
        items: &'a [SessionInfo<'a>],
    ) -> Vec<&'a str> {
        state
            .order(enabled, items)
            .into_iter()
            .map(|index| items[index].pane)
            .collect()
    }

    #[test]
    fn attention_matches_rollup() {
        assert_eq!(Attention::of(Some("blocked"), true), Attention::Blocked);
        assert_eq!(Attention::of(Some("done"), true), Attention::UnreadDone);
        assert_eq!(Attention::of(Some("done"), false), Attention::None);
        assert_eq!(Attention::of(Some("idle"), true), Attention::None);
        assert_eq!(Attention::of(Some("working"), true), Attention::Working);
    }

    #[test]
    fn enabled_groups_blocked_then_unread_then_other() {
        let state = PriorityState::default();
        let items = [
            info("idle", Some("idle"), false, None),
            info("done", Some("done"), true, None),
            info("blocked", Some("blocked"), false, None),
            info("working", Some("working"), false, None),
        ];
        assert_eq!(
            ordered(&state, true, &items),
            ["blocked", "done", "working", "idle"]
        );
        assert!(state.is_priority_group(items[1]));
        assert!(state.is_priority_group(items[2]));
        assert!(!state.is_priority_group(items[0]));
        assert!(!state.is_priority_group(items[3]));
    }

    #[test]
    fn hold_keeps_viewed_unread_in_priority() {
        let mut state = PriorityState::default();
        let live = [info("a", Some("done"), true, None)];
        let sessions = owned_list(&live);
        state.note_selection(true, "d", None, Some("a"), &sessions);
        let viewed = info("a", Some("done"), false, None);
        assert!(state.is_priority_group(viewed));
        assert_eq!(state.group_attention(viewed), Attention::UnreadDone);
        let after = [owned(viewed)];
        state.note_selection(true, "d", Some("a"), Some("b"), &after);
        assert!(!state.is_priority_group(viewed));
        let items = [viewed, info("b", Some("idle"), false, None)];
        assert_eq!(ordered(&state, true, &items), ["a", "b"]);
    }

    #[test]
    fn working_releases_hold_and_loading_is_other() {
        let mut state = PriorityState::default();
        let live = [info("a", Some("done"), true, None)];
        let sessions = owned_list(&live);
        state.note_selection(true, "d", None, Some("a"), &sessions);
        state.note_snapshot("d", &[info("a", Some("working"), false, Some(2))]);
        let working = info("a", Some("working"), false, Some(2));
        assert!(!state.is_priority_group(working));
        assert!(state.is_loading(working));
    }

    #[test]
    fn starting_working_does_not_rank() {
        let mut state = PriorityState::default();
        state.note_snapshot("d", &[info("a", Some("idle"), false, Some(1))]);
        state.note_starting("d", "a");
        state.note_snapshot("d", &[info("a", Some("working"), false, Some(2))]);
        assert_eq!(
            state.loading_order.get(&("d".into(), "a".into())).copied(),
            None
        );
    }

    #[test]
    fn reconnect_without_history_does_not_rank() {
        let mut state = PriorityState::default();
        state.note_snapshot("d", &[info("a", Some("working"), false, Some(4))]);
        assert!(state.loading_order.is_empty());
    }

    #[test]
    fn seq_regression_resets_device_ranks() {
        let mut state = PriorityState::default();
        state.note_snapshot("d", &[info("a", Some("idle"), false, Some(8))]);
        state.note_snapshot("d", &[info("a", Some("working"), false, Some(9))]);
        assert_eq!(
            state.loading_order.get(&("d".into(), "a".into())).copied(),
            Some(1)
        );
        state.note_snapshot("d", &[info("a", Some("working"), false, Some(2))]);
        assert!(state.loading_order.is_empty());
    }

    #[test]
    fn prune_drops_closed_panes() {
        let mut state = PriorityState::default();
        state.note_starting("d", "gone");
        state.handled.push(("d".into(), "gone".into()));
        state.loading_order.insert(("d".into(), "gone".into()), 3);
        state.hold = Some((("d".into(), "gone".into()), Attention::UnreadDone));
        state.prune("d", &["keep".into()].into_iter().collect());
        assert!(!state.is_starting("d", "gone"));
        assert!(state.handled.is_empty());
        assert!(state.loading_order.is_empty());
        assert!(state.hold.is_none());
    }

    #[test]
    fn disabled_mode_uses_recency() {
        let mut state = PriorityState::default();
        state.note_snapshot(
            "d",
            &[
                info("a", Some("idle"), false, Some(1)),
                info("b", Some("idle"), false, Some(1)),
            ],
        );
        state.note_snapshot(
            "d",
            &[
                info("a", Some("idle"), false, Some(1)),
                info("b", Some("working"), false, Some(2)),
            ],
        );
        let items = [
            info("a", Some("idle"), false, Some(1)),
            info("b", Some("working"), false, Some(2)),
        ];
        assert_eq!(ordered(&state, false, &items), ["b", "a"]);
        assert_eq!(items[0].pane, "a");
    }
}
