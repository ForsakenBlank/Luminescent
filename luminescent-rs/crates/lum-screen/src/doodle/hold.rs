//! The rules for the hold-to-draw key, which came from real bugs.
//!
//! Alt as the draw key: drawing only starts once the key has been held on its own for the solo delay, and any
//! other hotkey pressed meanwhile (Alt+N, Alt+T...) cancels it, so Alt shortcuts do not flash the overlay on.
//! Ctrl, Shift or Super already held when the delay runs out (Ctrl+Alt+... hotkeys) keep it off too. Alt
//! held a moment and then Alt+Tab gets the drawing out of the way until Alt is let go. Esc stops drawing at
//! any time, and drawing then stays off until the key is let go. The text tool keeps drawing on after the
//! key is let go, until the text is done.
//!
//! The module feeds this what happens and does what it answers; nothing here touches a window or a timer.

/// What the module has to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Start drawing: the overlays take the mouse.
    Activate,
    /// Stop drawing: the overlays let clicks through again.
    Deactivate,
    /// Call [`Hold::solo_elapsed`] after this many milliseconds.
    StartTimer(u64),
    StopTimer,
    /// Throw away the text being typed.
    DiscardText,
    /// Clear the drawing (clear on release, hold mode).
    Clear,
}

#[derive(Debug, Default)]
pub struct Hold {
    pub hold_mode: bool,
    pub solo_only: bool,
    pub solo_delay_ms: u64,
    pub clear_on_release: bool,
    /// Keys of the doodle's own shortcuts: they never count as "using an Alt shortcut".
    pub own_keys: Vec<String>,
    active: bool,
    /// Esc or panic while holding: stay off until the key is let go.
    wait_release: bool,
    /// Another key went down while the draw key was held.
    interrupted: bool,
    /// A text box is open (drawing stays on while you type).
    typing: bool,
    off_after_text: bool,
}

impl Hold {
    pub fn new(
        hold_mode: bool,
        solo_only: bool,
        solo_delay_ms: u64,
        clear_on_release: bool,
        own_keys: Vec<String>,
    ) -> Hold {
        Hold { hold_mode, solo_only, solo_delay_ms, clear_on_release, own_keys, ..Hold::default() }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_waiting_for_release(&self) -> bool {
        self.wait_release
    }

    /// The draw key went down.
    pub fn pressed(&mut self) -> Vec<Effect> {
        if self.wait_release {
            return Vec::new();
        }
        self.interrupted = false;
        let delay = if self.solo_only { self.solo_delay_ms } else { 0 };
        if delay == 0 { self.activate() } else { vec![Effect::StartTimer(delay)] }
    }

    /// The solo delay ran out. `held`: the key is still down as far as the system can tell; `other_modifier`:
    /// Ctrl, Shift or Super is down too (a Ctrl+Alt+... hotkey on its way, not a doodle).
    pub fn solo_elapsed(&mut self, held: bool, other_modifier: bool) -> Vec<Effect> {
        if self.interrupted || self.wait_release || !held || other_modifier {
            return Vec::new();
        }
        self.activate()
    }

    /// Another key went down while the draw key is held (`name` as the hotkeys report it). `drawing`: a stroke
    /// is under way, which a key press must not cut off.
    pub fn interrupted(&mut self, name: &str, drawing: bool) -> Vec<Effect> {
        if !self.solo_only {
            return Vec::new();
        }
        let name = name.to_lowercase();
        let name = name.strip_prefix("left ").or_else(|| name.strip_prefix("right ")).unwrap_or(&name);
        if self.own_keys.iter().any(|k| k == name) {
            return Vec::new();
        }
        self.interrupted = true;
        let mut out = vec![Effect::StopTimer];
        if self.active && !drawing {
            // Alt held a moment, then Alt+Tab / Alt+N: out of the way until Alt is let go
            out.extend(self.deactivate());
            self.wait_release = true;
        }
        out
    }

    /// The draw key came up.
    pub fn released(&mut self) -> Vec<Effect> {
        self.wait_release = false;
        let mut out = vec![Effect::StopTimer];
        out.extend(self.deactivate());
        out
    }

    /// Toggle mode's key press.
    pub fn toggle(&mut self) -> Vec<Effect> {
        if self.active { self.deactivate() } else { self.activate() }
    }

    pub fn activate(&mut self) -> Vec<Effect> {
        if self.active {
            return Vec::new();
        }
        self.active = true;
        vec![Effect::Activate]
    }

    pub fn deactivate(&mut self) -> Vec<Effect> {
        if !self.active {
            return Vec::new();
        }
        if self.typing {
            // the key was let go to type: stop once the text is done
            self.off_after_text = true;
            return Vec::new();
        }
        self.active = false;
        let mut out = vec![Effect::Deactivate];
        if self.hold_mode && self.clear_on_release {
            out.push(Effect::Clear);
        }
        out
    }

    /// Esc: the text being typed is thrown away and drawing stops (in hold mode until the key is let go).
    pub fn escape(&mut self) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.typing {
            self.off_after_text = false;
            self.typing = false;
            out.push(Effect::DiscardText);
        }
        if self.active {
            out.extend(self.deactivate());
            self.wait_release = self.hold_mode;
        }
        out
    }

    /// The panic hotkey (and the tray's "Clear doodles & stop drawing"). `held`: the draw key is down right now.
    pub fn panic(&mut self, held: bool) -> Vec<Effect> {
        self.off_after_text = false;
        let mut out = Vec::new();
        if self.typing {
            self.typing = false;
            out.push(Effect::DiscardText);
        }
        out.extend(self.deactivate());
        self.wait_release = self.hold_mode && held;
        out
    }

    pub fn text_opened(&mut self) {
        self.typing = true;
    }

    /// The text box closed (kept or thrown away).
    pub fn text_closed(&mut self) -> Vec<Effect> {
        self.typing = false;
        if std::mem::take(&mut self.off_after_text) { self.deactivate() } else { Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effect::*;

    fn alt() -> Hold {
        Hold::new(true, true, 120, false, vec!["esc".into(), "x".into(), "f".into(), "shift".into()])
    }

    #[test]
    fn held_alone_past_the_delay_draws_until_let_go() {
        let mut h = alt();
        assert_eq!(h.pressed(), [StartTimer(120)]);
        assert_eq!(h.solo_elapsed(true, false), [Activate]);
        assert!(h.is_active());
        assert_eq!(h.released(), [StopTimer, Deactivate]);
        assert!(!h.is_active());
    }

    #[test]
    fn an_alt_shortcut_during_the_delay_never_flashes_the_overlay() {
        let mut h = alt();
        h.pressed();
        assert_eq!(h.interrupted("n", false), [StopTimer]);
        assert!(h.solo_elapsed(true, false).is_empty());
        assert!(!h.is_active());
        // the next press starts afresh
        h.released();
        h.pressed();
        assert_eq!(h.solo_elapsed(true, false), [Activate]);
    }

    #[test]
    fn ctrl_alt_hotkeys_and_a_key_already_up_keep_it_off() {
        let mut h = alt();
        h.pressed();
        assert!(h.solo_elapsed(true, true).is_empty());
        h.pressed();
        assert!(h.solo_elapsed(false, false).is_empty());
    }

    #[test]
    fn alt_tab_after_a_moment_gets_out_of_the_way_until_alt_is_let_go() {
        let mut h = alt();
        h.pressed();
        h.solo_elapsed(true, false);
        assert_eq!(h.interrupted("Tab", false), [StopTimer, Deactivate]);
        assert!(h.is_waiting_for_release());
        // pressing again before letting go does nothing
        assert!(h.pressed().is_empty());
        h.released();
        assert!(!h.is_waiting_for_release());
        assert_eq!(h.pressed(), [StartTimer(120)]);
    }

    #[test]
    fn a_key_mid_stroke_and_the_doodle_s_own_keys_do_not_stop_it() {
        let mut h = alt();
        h.pressed();
        h.solo_elapsed(true, false);
        assert_eq!(h.interrupted("left shift", false), []);
        assert_eq!(h.interrupted("F", false), []);
        assert_eq!(h.interrupted("n", true), [StopTimer]);
        assert!(h.is_active());
    }

    #[test]
    fn escape_stops_drawing_until_the_key_is_let_go() {
        let mut h = alt();
        h.pressed();
        h.solo_elapsed(true, false);
        assert_eq!(h.escape(), [Deactivate]);
        assert!(h.pressed().is_empty());
        h.released();
        assert_eq!(h.pressed(), [StartTimer(120)]);
    }

    #[test]
    fn typing_keeps_drawing_on_after_the_key_is_let_go() {
        let mut h = alt();
        h.pressed();
        h.solo_elapsed(true, false);
        h.text_opened();
        assert_eq!(h.released(), [StopTimer]);
        assert!(h.is_active());
        assert_eq!(h.text_closed(), [Deactivate]);
        assert!(!h.is_active());
        // Esc while typing throws the text away and stops
        h.pressed();
        h.solo_elapsed(true, false);
        h.text_opened();
        assert_eq!(h.escape(), [DiscardText, Deactivate]);
    }

    #[test]
    fn no_delay_or_not_solo_draws_at_once_and_toggle_mode_toggles() {
        let mut h = Hold::new(true, false, 120, true, Vec::new());
        assert_eq!(h.pressed(), [Activate]);
        assert!(h.interrupted("n", false).is_empty());
        assert_eq!(h.released(), [StopTimer, Deactivate, Clear]);
        let mut t = Hold::new(false, true, 120, true, Vec::new());
        assert_eq!(t.toggle(), [Activate]);
        assert_eq!(t.toggle(), [Deactivate]);
    }

    #[test]
    fn panic_while_holding_waits_for_the_release() {
        let mut h = alt();
        h.pressed();
        h.solo_elapsed(true, false);
        assert_eq!(h.panic(true), [Deactivate]);
        assert!(h.is_waiting_for_release());
        let mut h = alt();
        h.panic(false);
        assert!(!h.is_waiting_for_release());
    }
}
