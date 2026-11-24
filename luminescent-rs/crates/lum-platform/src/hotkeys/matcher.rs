//! Turning raw key events into hotkeys and hold keys, for the Windows hook. Plain logic over a [`KeyState`]
//! (the system's own idea of which keys are down), so it is tested on every system.
//!
//! Stuck keys: a combo matches when exactly its keys are held, and the hook only learns that a key went up
//! from the hook itself. Windows drops key-up events in several common cases (an admin window has focus,
//! Win+L, Ctrl+Alt+Del, UAC, the hook being briefly too slow). A lost Alt key-up meant every later R looked
//! like Alt+R, and a lost key-up of any other key meant no combo matched exactly any more (so Esc and Alt+X
//! stopped working while drawing). So before a key press is matched, every key Windows itself says is no
//! longer held is thrown out, and while anything is held a 100 ms check does the same clean-up and lets go of
//! hold keys whose release was never seen.

#![cfg_attr(not(windows), allow(dead_code))]

use super::{Signal, keys, vk};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Whether the system says a key is down right now.
pub trait KeyState {
    fn is_down(&self, vk: u16) -> bool;
}

/// One event from the hook.
#[derive(Debug, Clone, Copy)]
pub struct Raw {
    pub vk: u16,
    pub extended: bool,
    pub down: bool,
    /// What the system said about this key when the event happened, before it counted the event itself: a
    /// key down that was already down is auto-repeat, one that was up is a fresh press (perhaps after a lost
    /// key-up).
    pub was_down: bool,
    pub at: Instant,
}

struct Entry {
    keys: BTreeSet<String>,
    main: String,
    mods: Vec<String>,
    always: bool,
}

struct HoldState {
    down: bool,
    since: Instant,
}

/// The keys a hotkey can not be cut short by: a key goes stale only after this long without a check.
const STALE_AFTER: Duration = Duration::from_millis(300);
/// A hold key is trusted down for this long after it went down, before the check may release it.
const HOLD_GRACE: Duration = Duration::from_millis(150);

pub struct Matcher<K: KeyState> {
    state: K,
    pressed: BTreeMap<String, Instant>,
    combos: BTreeMap<String, Entry>,
    holds: BTreeMap<String, HoldState>,
    suspended: bool,
}

impl<K: KeyState> Matcher<K> {
    pub fn new(state: K) -> Matcher<K> {
        Matcher {
            state,
            pressed: BTreeMap::new(),
            combos: BTreeMap::new(),
            holds: BTreeMap::new(),
            suspended: false,
        }
    }

    pub fn add(&mut self, combo: &str, always: bool) {
        let keys: BTreeSet<String> = combo.split('+').map(str::to_owned).collect();
        let (mods, main) = keys::split(combo);
        self.combos.insert(combo.to_owned(), Entry { keys, main, mods, always });
    }

    pub fn remove(&mut self, combo: &str) {
        self.combos.remove(combo);
    }

    pub fn add_hold(&mut self, key: &str) {
        self.holds.insert(key.to_owned(), HoldState { down: false, since: Instant::now() });
    }

    pub fn remove_hold(&mut self, key: &str) {
        self.holds.remove(key);
    }

    pub fn suspend(&mut self, on: bool) {
        self.suspended = on;
    }

    /// Forgets every key thought to be held; held hold keys report going up.
    pub fn reset(&mut self) -> Vec<Signal> {
        self.pressed.clear();
        let mut out = Vec::new();
        for (key, h) in &mut self.holds {
            if std::mem::take(&mut h.down) {
                out.push(Signal::HoldUp { key: key.clone(), missed: false });
            }
        }
        out
    }

    /// Something is held, so the 100 ms check should run.
    pub fn busy(&self) -> bool {
        !self.pressed.is_empty() || self.holds.values().any(|h| h.down)
    }

    fn physically_down(&self, name: &str) -> bool {
        let codes = vk::codes(name);
        // a key we can not ask about is assumed held, so nothing is let go of wrongly
        codes.is_empty() || codes.iter().any(|&c| self.state.is_down(c))
    }

    /// Drops keys thought to be held that the system says are up. Gives back their names.
    fn prune(&mut self, skip: Option<&str>, min_age: Duration, now: Instant) -> Vec<String> {
        let stale: Vec<String> = self
            .pressed
            .iter()
            .filter(|(name, since)| {
                Some(name.as_str()) != skip
                    && now.saturating_duration_since(**since) >= min_age
                    && !self.physically_down(name)
            })
            .map(|(name, _)| name.clone())
            .collect();
        for name in &stale {
            self.pressed.remove(name);
        }
        stale
    }

    pub fn key(&mut self, raw: Raw) -> Vec<Signal> {
        let Some(name) = vk::name(raw.vk, raw.extended) else { return Vec::new() };
        let mut out = Vec::new();
        if !raw.down {
            self.pressed.remove(&name);
            if let Some(h) = self.holds.get_mut(&name)
                && std::mem::take(&mut h.down)
                && !self.suspended
            {
                out.push(Signal::HoldUp { key: name, missed: false });
            }
            return out;
        }
        self.prune(Some(&name), Duration::ZERO, raw.at);
        let repeat = self.pressed.contains_key(&name) && raw.was_down;
        self.pressed.insert(name.clone(), raw.at);
        if self.suspended {
            return out;
        }
        if !repeat {
            for (hold, h) in &self.holds {
                if *hold != name && h.down && self.pressed.contains_key(hold) {
                    out.push(Signal::Interrupt { hold: hold.clone(), by: name.clone(), by_combo: None });
                }
            }
        }
        if let Some(h) = self.holds.get_mut(&name)
            && !(repeat && h.down)
        {
            h.down = true;
            h.since = raw.at;
            out.push(Signal::HoldDown { key: name.clone() });
        }
        // holding the keys down fires once
        if repeat {
            return out;
        }
        let held: BTreeSet<String> = self.pressed.keys().cloned().collect();
        for (combo, e) in &self.combos {
            let fires = if e.always {
                e.main == name && e.mods.iter().all(|m| held.contains(m))
            } else {
                e.keys == held
            };
            if fires {
                out.push(Signal::Pressed { combo: combo.clone(), at: raw.at });
            }
        }
        out
    }

    /// The 100 ms check: forget lost key-ups and release hold keys the system says are up.
    pub fn tick(&mut self, now: Instant) -> Vec<Signal> {
        let dropped = self.prune(None, STALE_AFTER, now);
        if !dropped.is_empty() {
            tracing::info!("cleared stuck key(s) the hook never saw released: {}", dropped.join(", "));
        }
        let mut out = Vec::new();
        let missed: Vec<String> = self
            .holds
            .iter()
            .filter(|(key, h)| {
                h.down && now.saturating_duration_since(h.since) > HOLD_GRACE && !self.physically_down(key)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in missed {
            if let Some(h) = self.holds.get_mut(&key) {
                h.down = false;
            }
            self.pressed.remove(&key);
            out.push(Signal::HoldUp { key, missed: true });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    /// The system's view, set by the test.
    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<HashSet<u16>>>);

    impl KeyState for Fake {
        fn is_down(&self, vk: u16) -> bool {
            self.0.borrow().contains(&vk)
        }
    }

    struct Keyboard {
        m: Matcher<Fake>,
        sys: Fake,
        now: Instant,
    }

    impl Keyboard {
        fn new() -> Keyboard {
            let sys = Fake::default();
            Keyboard { m: Matcher::new(sys.clone()), sys, now: Instant::now() }
        }

        fn code(name: &str) -> u16 {
            vk::of(name).unwrap_or_else(|| panic!("{name}"))
        }

        /// A real press: the system counts it after the hook has seen it.
        fn down(&mut self, name: &str) -> Vec<Signal> {
            let vk = Self::code(name);
            let was_down = self.sys.is_down(vk);
            let out = self.m.key(Raw { vk, extended: false, down: true, was_down, at: self.now });
            self.sys.0.borrow_mut().insert(vk);
            out
        }

        fn up(&mut self, name: &str) -> Vec<Signal> {
            let vk = Self::code(name);
            self.sys.0.borrow_mut().remove(&vk);
            self.m.key(Raw { vk, extended: false, down: false, was_down: true, at: self.now })
        }

        /// The key-up Windows never delivered.
        fn lose_up(&mut self, name: &str) {
            self.sys.0.borrow_mut().remove(&Self::code(name));
        }

        fn later(&mut self, ms: u64) -> Vec<Signal> {
            self.now += Duration::from_millis(ms);
            self.m.tick(self.now)
        }
    }

    fn fired(out: &[Signal]) -> Vec<&str> {
        out.iter()
            .filter_map(|s| match s {
                Signal::Pressed { combo, .. } => Some(combo.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_combo_fires_once_when_exactly_its_keys_are_down() {
        let mut k = Keyboard::new();
        k.m.add("ctrl+shift+space", false);
        k.down("ctrl");
        k.down("shift");
        assert_eq!(fired(&k.down("space")), ["ctrl+shift+space"]);
        // auto-repeat
        let vk = Keyboard::code("space");
        let again = k.m.key(Raw { vk, extended: false, down: true, was_down: true, at: k.now });
        assert!(fired(&again).is_empty());
        k.up("space");
        k.down("a");
        assert!(fired(&k.down("space")).is_empty(), "an extra key held: no match");
    }

    #[test]
    fn a_lost_alt_release_does_not_turn_r_into_alt_r() {
        let mut k = Keyboard::new();
        k.m.add("alt+r", false);
        k.m.add("r", false);
        k.down("alt");
        k.lose_up("alt");
        assert_eq!(fired(&k.down("r")), ["r"]);
    }

    #[test]
    fn a_fresh_press_after_a_lost_release_is_not_a_repeat() {
        let mut k = Keyboard::new();
        k.m.add("esc", false);
        assert_eq!(fired(&k.down("esc")), ["esc"]);
        k.lose_up("esc");
        assert_eq!(fired(&k.down("esc")), ["esc"]);
    }

    #[test]
    fn the_check_clears_stale_keys_and_missed_hold_releases() {
        let mut k = Keyboard::new();
        k.m.add_hold("alt");
        assert_eq!(k.down("alt"), [Signal::HoldDown { key: "alt".into() }]);
        assert!(k.m.busy());
        // not cut short while it really is held
        assert!(k.later(500).is_empty());
        k.lose_up("alt");
        assert_eq!(k.later(100), [Signal::HoldUp { key: "alt".into(), missed: true }]);
        assert!(!k.m.busy(), "nothing held: the check can stop");
    }

    #[test]
    fn hold_keys_report_both_ways_and_interrupts() {
        let mut k = Keyboard::new();
        k.m.add_hold("alt");
        k.down("alt");
        assert_eq!(k.down("n"), [Signal::Interrupt { hold: "alt".into(), by: "n".into(), by_combo: None }]);
        assert_eq!(k.up("alt"), [Signal::HoldUp { key: "alt".into(), missed: false }]);
    }

    #[test]
    fn always_fires_with_other_keys_held() {
        let mut k = Keyboard::new();
        k.m.add("ctrl+f", true);
        k.down("w");
        k.down("ctrl");
        assert_eq!(fired(&k.down("f")), ["ctrl+f"]);
        k.up("ctrl");
        k.up("f");
        assert!(fired(&k.down("f")).is_empty(), "its own modifier must be down");
    }

    #[test]
    fn suspended_and_reset() {
        let mut k = Keyboard::new();
        k.m.add("ctrl+k", false);
        k.m.add_hold("f8");
        k.m.suspend(true);
        k.down("ctrl");
        assert!(k.down("k").is_empty());
        k.m.suspend(false);
        k.up("k");
        k.up("ctrl");
        k.down("f8");
        assert_eq!(k.m.reset(), [Signal::HoldUp { key: "f8".into(), missed: false }]);
        assert!(!k.m.busy());
    }
}
