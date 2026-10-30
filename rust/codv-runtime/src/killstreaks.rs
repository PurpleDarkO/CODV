//! F5c: MW2's killstreaks for the GTA player, as maps/mp/killstreaks/
//! _killstreaks.gsc keeps them: every kill in a row adds to the streak, a
//! chosen reward is earned when the streak reaches its kill count exactly,
//! earned rewards wait on a stack (the last earned is used first) and
//! survive death, and dying resets the streak.
//!
//! CODV's choices on top: only hostile kills count (the host decides), the
//! player picks up to three rewards among the implemented ones, and the
//! EMP's kill count can be lowered for testing.

/// The rewards by their ABI id (CV_KS_*): MW2's killstreakTable references.
pub const REWARDS: [(u32, &str); 10] = [
    (1, "uav"),
    (2, "predator_missile"),
    (3, "helicopter"),
    (4, "emp"),
    (5, "airdrop"),
    (6, "airdrop_sentry_minigun"),
    (7, "precision_airstrike"),
    (8, "ac130"),
    (9, "helicopter_minigun"),
    (10, "nuke"),
];

/// The rewards CODV can call in (F5c stages 1 to 4: all ten).
pub const IMPLEMENTED: [u32; 10] =
    [UAV, PREDATOR, HELICOPTER, EMP, CARE_PACKAGE, SENTRY, AIRSTRIKE, AC130, CHOPPER_GUNNER, NUKE];
pub const UAV: u32 = 1;
pub const PREDATOR: u32 = 2;
pub const HELICOPTER: u32 = 3;
pub const EMP: u32 = 4;
pub const CARE_PACKAGE: u32 = 5;
pub const SENTRY: u32 = 6;
pub const AIRSTRIKE: u32 = 7;
pub const AC130: u32 = 8;
pub const CHOPPER_GUNNER: u32 = 9;
pub const NUKE: u32 = 10;

pub fn name_of(id: u32) -> Option<&'static str> {
    REWARDS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

pub fn id_of(name: &str) -> Option<u32> {
    REWARDS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(i, _)| *i)
}

/// MW2's kill counts (mp/killstreakTable.csv) when the import has none.
const DEFAULT_KILLS: [(u32, i32); 10] =
    [(1, 3), (2, 5), (3, 7), (4, 15), (5, 4), (6, 5), (7, 6), (8, 11), (9, 11), (10, 25)];

/// CODV's spaced kill counts (CV_OPT_KS_SPACED), MW2's order kept, every reward on its own kill,
/// the gaps widening up to the Tactical Nuke at 35.
pub const SPACED_KILLS: [(u32, i32); 10] = [
    (UAV, 3),
    (CARE_PACKAGE, 6),
    (PREDATOR, 9),
    (SENTRY, 12),
    (AIRSTRIKE, 15),
    (HELICOPTER, 18),
    (AC130, 22),
    (CHOPPER_GUNNER, 26),
    (EMP, 30),
    (NUKE, 35),
];

pub struct Streaks {
    /// Kill counts by reward id (the import's table, else MW2's).
    kills: Vec<(u32, i32)>,
    pub enabled: bool,
    /// The player's three picks (reward ids, 0 none).
    pub slots: [u32; 3],
    /// CODV's "all killstreaks" (CV_OPT_KS_ALL): every implemented reward comes at its kill count,
    /// not just the three picks (MW2's rule, the default here; the host turns it on).
    pub all: bool,
    /// CODV's spaced kill counts (SPACED_KILLS) over MW2's table; the EMP override is MW2's table only.
    pub spaced: bool,
    /// CODV's EMP kill count (testing), None: the table's.
    pub emp_kills: Option<i32>,
    pub streak: i32,
    held: Vec<u32>,
}

impl Streaks {
    /// `table`: (reference, kills) from the import's HUD file (empty: MW2's defaults).
    pub fn new(table: &[(String, i32)]) -> Streaks {
        let mut kills: Vec<(u32, i32)> =
            table.iter().filter_map(|(n, k)| Some((id_of(n)?, *k))).filter(|(_, k)| *k > 0).collect();
        for (id, k) in DEFAULT_KILLS {
            if !kills.iter().any(|(i, _)| *i == id) {
                kills.push((id, k));
            }
        }
        // MW2's default picks (CODV's choice): UAV, Predator Missile, Attack Helicopter.
        Streaks {
            kills,
            enabled: true,
            slots: [UAV, PREDATOR, HELICOPTER],
            all: false,
            spaced: false,
            emp_kills: None,
            streak: 0,
            held: Vec::new(),
        }
    }

    /// The kills reward `id` needs (CODV's EMP override applied).
    pub fn kills_for(&self, id: u32) -> Option<i32> {
        if self.spaced {
            return SPACED_KILLS.iter().find(|(i, _)| *i == id).map(|(_, k)| *k);
        }
        if id == EMP
            && let Some(k) = self.emp_kills
        {
            return Some(k.max(1));
        }
        self.kills.iter().find(|(i, _)| *i == id).map(|(_, k)| *k)
    }

    /// Sets slot `slot` (0..3) to `id` (0 off); only implemented rewards are taken.
    pub fn set_slot(&mut self, slot: usize, id: u32) -> bool {
        if slot >= 3 || (id != 0 && !IMPLEMENTED.contains(&id)) {
            return false;
        }
        self.slots[slot] = id;
        true
    }

    /// The rewards in play: every implemented one with "all killstreaks", else the picks.
    pub fn active(&self) -> Vec<u32> {
        if self.all { IMPLEMENTED.to_vec() } else { self.slots.iter().copied().filter(|&id| id != 0).collect() }
    }

    /// One more kill in a row: the rewards it earns (usually none).
    pub fn kill(&mut self) -> Vec<u32> {
        if !self.enabled {
            return Vec::new();
        }
        self.streak += 1;
        let mut earned = Vec::new();
        for id in self.active() {
            if !earned.contains(&id) && self.kills_for(id) == Some(self.streak) {
                earned.push(id);
            }
        }
        self.held.extend(&earned);
        earned
    }

    /// The kills the next reward picked needs (None: past the last one, or none picked).
    pub fn next_kills(&self) -> Option<i32> {
        self.active().into_iter().filter_map(|id| self.kills_for(id)).filter(|&k| k > self.streak).min()
    }

    /// Death or the mode switched off: the streak starts again; earned rewards stay.
    pub fn reset_streak(&mut self) {
        self.streak = 0;
    }

    /// The reward the action slot would use (the last earned).
    pub fn top(&self) -> Option<u32> {
        self.held.last().copied()
    }

    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// A reward from elsewhere (a care package): on top of the stack, as MW2's giveKillstreak.
    pub fn give(&mut self, id: u32) -> bool {
        if !self.enabled || name_of(id).is_none() {
            return false;
        }
        self.held.push(id);
        true
    }

    /// A death without CODV's keep option: the rewards not used yet are lost (MW2 keeps them; in GTA a
    /// hospital respawn is free, so they would pile up).
    pub fn drop_held(&mut self) -> usize {
        std::mem::take(&mut self.held).len()
    }

    /// The Tactical Nuke called in ends the run (MW2 ends the match on it): the streak starts again
    /// from zero and the rewards not used yet are gone. How many were dropped.
    pub fn start_over(&mut self) -> usize {
        self.streak = 0;
        self.drop_held()
    }

    /// Uses the top reward (action slot 4).
    pub fn activate(&mut self) -> Option<u32> {
        if !self.enabled {
            return None;
        }
        self.held.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nuke_starts_the_streak_over_without_the_rewards_held() {
        let mut s = Streaks::new(&[]);
        s.all = true;
        s.spaced = true;
        for _ in 0..35 {
            s.kill();
        }
        assert_eq!(s.top(), Some(NUKE));
        assert_eq!(s.activate(), Some(NUKE));
        assert!(s.held() > 0, "the rewards earned before it wait on the stack");
        assert!(s.start_over() > 0);
        assert_eq!((s.streak, s.held(), s.top()), (0, 0, None));
        assert_eq!(s.next_kills(), Some(3), "back to the first reward");
    }

    #[test]
    fn rewards_come_at_their_kill_counts_and_wait_on_a_stack() {
        let mut s = Streaks::new(&[("uav".into(), 3), ("emp".into(), 15)]);
        s.slots = [UAV, 0, EMP];
        s.emp_kills = Some(5);
        assert!(s.kill().is_empty() && s.kill().is_empty());
        assert_eq!(s.kill(), [UAV]);
        assert_eq!(s.kill(), Vec::<u32>::new());
        assert_eq!(s.kill(), [EMP]);
        // The last earned is used first; the stack outlives the streak.
        s.reset_streak();
        assert_eq!((s.streak, s.held(), s.top()), (0, 2, Some(EMP)));
        assert_eq!(s.activate(), Some(EMP));
        assert_eq!(s.activate(), Some(UAV));
        assert_eq!(s.activate(), None);
        // Past the highest reward the streak goes on without repeats.
        for _ in 0..10 {
            s.kill();
        }
        assert_eq!(s.held(), 2);
    }

    /// UAV, EMP and the Tactical Nuke picked; the UAV used at 3 kills, the EMP at 15: the nuke still
    /// comes at 25 kills in that life (using a reward never restarts the streak; a death does).
    #[test]
    fn the_nuke_comes_at_25_kills_after_the_rewards_before_it_were_used() {
        let mut s = Streaks::new(&[("uav".into(), 3), ("emp".into(), 15), ("nuke".into(), 25)]);
        s.slots = [UAV, EMP, NUKE];
        let mut earned = Vec::new();
        for k in 1..=25 {
            for id in s.kill() {
                earned.push((k, id));
            }
            if k == 3 || k == 15 {
                assert!(s.activate().is_some(), "used at {k}");
            }
        }
        assert_eq!(earned, [(3, UAV), (15, EMP), (25, NUKE)]);
        assert_eq!(s.top(), Some(NUKE));
        // A death on the way: the count starts again.
        let mut t = Streaks::new(&[]);
        t.slots = [UAV, EMP, NUKE];
        for _ in 0..20 {
            t.kill();
        }
        t.reset_streak();
        assert!((0..20).flat_map(|_| t.kill()).all(|id| id != NUKE));
    }

    #[test]
    fn a_death_can_drop_the_rewards_not_used() {
        let mut s = Streaks::new(&[]);
        s.slots = [UAV, 0, EMP];
        for _ in 0..3 {
            s.kill();
        }
        assert!(s.give(AC130));
        assert_eq!(s.drop_held(), 2);
        assert_eq!((s.held(), s.top()), (0, None));
    }

    #[test]
    fn rewards_can_be_handed_over() {
        let mut s = Streaks::new(&[]);
        assert!(s.give(AC130) && s.top() == Some(AC130) && !s.give(99));
    }

    #[test]
    fn only_implemented_rewards_can_be_picked() {
        let mut s = Streaks::new(&[]);
        assert!(!s.set_slot(1, 11));
        assert!(s.set_slot(1, UAV) && s.set_slot(0, 0) && !s.set_slot(3, UAV));
        assert_eq!(s.kills_for(EMP), Some(15));
        assert_eq!(name_of(EMP), Some("emp"));
        s.enabled = false;
        assert!(s.kill().is_empty() && s.streak == 0);
    }

    #[test]
    fn all_killstreaks_brings_every_reward_at_its_kill_count() {
        let mut s = Streaks::new(&[]);
        s.all = true;
        s.slots = [UAV, 0, EMP];
        let mut earned = Vec::new();
        for _ in 0..25 {
            earned.extend(s.kill());
        }
        for id in IMPLEMENTED {
            assert!(earned.contains(&id), "{id} missing from {earned:?}");
        }
        // In MW2's kill-count order, the nuke last (so 4 calls it in first).
        assert_eq!(earned.first(), Some(&UAV));
        assert_eq!(earned.last(), Some(&NUKE));
        assert_eq!(s.top(), Some(NUKE));
        assert_eq!(s.next_kills(), None);
        s.all = false;
        s.reset_streak();
        assert_eq!(s.next_kills(), Some(3));
    }

    #[test]
    fn spaced_killstreaks_come_one_per_kill_count_with_the_nuke_at_35() {
        let mut s = Streaks::new(&[]);
        s.all = true;
        s.spaced = true;
        s.emp_kills = Some(15); // the EMP override is MW2's table only
        let mut at = Vec::new();
        for kill in 1..=40 {
            for id in s.kill() {
                at.push((id, kill));
            }
        }
        assert_eq!(at, SPACED_KILLS.to_vec());
        // Spaced on its own also moves the three picks.
        let mut p = Streaks::new(&[]);
        p.spaced = true;
        p.slots = [UAV, EMP, NUKE];
        assert_eq!(p.next_kills(), Some(3));
        assert_eq!(p.kills_for(NUKE), Some(35));
        p.spaced = false;
        assert_eq!(p.kills_for(NUKE), Some(25));
    }
}
