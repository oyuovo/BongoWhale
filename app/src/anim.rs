use crate::sprite::{ClipId, Sprites};
use crate::util::{now_ms, Rng};

pub struct Animator {
    clip: ClipId,
    frame: usize,
    frame_start: u64,
    blink_at: u64,
    sleep_at: u64,
    last_tap: u64,
    next_left: bool,
    rng: Rng,
    asleep: bool,
    pub paused: bool,
    pub sleep_after_ms: u64,
}

impl Animator {
    pub fn new(sleep_after_secs: u64) -> Self {
        let t = now_ms();
        Animator {
            clip: ClipId::Idle,
            frame: 0,
            frame_start: t,
            blink_at: t + 4200,
            sleep_at: t + sleep_after_secs * 1000,
            last_tap: 0,
            next_left: true,
            rng: Rng::new(t ^ 0x9E37_79B9_7F4A_7C15),
            asleep: false,
            paused: false,
            sleep_after_ms: sleep_after_secs * 1000,
        }
    }

    pub fn current(&self) -> (ClipId, usize) {
        (self.clip, self.frame)
    }

    #[allow(dead_code)]
    pub fn is_asleep(&self) -> bool {
        self.asleep
    }

    fn clip_len(&self, sprites: &Sprites, id: ClipId) -> usize {
        sprites.get(id).map(|c| c.frames.len().max(1)).unwrap_or(1)
    }

    fn clip_fps(&self, sprites: &Sprites, id: ClipId) -> f32 {
        sprites.get(id).map(|c| c.fps.max(1.0)).unwrap_or(8.0)
    }

    fn clip_loops(&self, sprites: &Sprites, id: ClipId) -> bool {
        sprites.get(id).map(|c| c.looping).unwrap_or(false)
    }

    pub fn set_sleep_after(&mut self, secs: u64) {
        self.sleep_after_ms = secs * 1000;
        self.sleep_at = now_ms() + self.sleep_after_ms;
    }

    /// Advance the state machine. Returns the frame to draw if it changed.
    pub fn tick(&mut self, sprites: &Sprites) -> Option<(ClipId, usize)> {
        if self.paused {
            return None;
        }
        let now = now_ms();
        let mut changed = false;
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 8 {
                break;
            }
            let dur = (1000.0 / self.clip_fps(sprites, self.clip)) as u64;
            if now < self.frame_start + dur.max(1) {
                break;
            }
            let len = self.clip_len(sprites, self.clip);
            if self.frame + 1 < len {
                // skip ahead if we are far behind (e.g. after system sleep)
                let elapsed = now - self.frame_start;
                let steps = (elapsed / dur.max(1)).max(1) as usize;
                self.frame = (self.frame + steps).min(len - 1);
                self.frame_start = now - (elapsed % dur.max(1));
                changed = true;
            } else if self.clip_loops(sprites, self.clip) {
                self.frame = 0;
                self.frame_start = now;
                changed = true;
            } else {
                self.clip = ClipId::Idle;
                self.frame = 0;
                self.frame_start = now;
                changed = true;
            }
        }

        // idle scheduling: blink + doze off
        if self.clip == ClipId::Idle {
            if now >= self.blink_at {
                self.blink_at = now + self.rng.range(3200, 9000) as u64;
                self.clip = ClipId::Blink;
                self.frame = 0;
                self.frame_start = now;
                changed = true;
            } else if !self.asleep && now >= self.sleep_at && self.sleep_after_ms > 0 {
                self.asleep = true;
                self.clip = ClipId::Sleep;
                self.frame = 0;
                self.frame_start = now;
                changed = true;
            }
        }

        if changed {
            Some((self.clip, self.frame))
        } else {
            None
        }
    }

    /// Absolute ms timestamp of the next thing that will change.
    pub fn next_deadline(&self, sprites: &Sprites) -> u64 {
        let dur = (1000.0 / self.clip_fps(sprites, self.clip)) as u64;
        let mut d = self.frame_start + dur.max(1);
        if self.clip == ClipId::Idle {
            d = d.min(self.blink_at);
            if !self.asleep && self.sleep_after_ms > 0 {
                d = d.min(self.sleep_at);
            }
        }
        let now = now_ms();
        if d <= now {
            now + 1
        } else {
            d
        }
    }

    /// A key or click happened. A side picks the hand; None alternates.
    pub fn tap(&mut self, side: Option<crate::keys::Side>) -> bool {
        use crate::keys::Side;
        let now = now_ms();
        self.sleep_at = now + self.sleep_after_ms;
        self.asleep = false;
        if self.paused {
            return false;
        }
        // one tap per ~45ms keeps fast typing readable but not machine-gun
        if now.saturating_sub(self.last_tap) < 45 {
            return false;
        }
        self.last_tap = now;
        let left = match side {
            Some(Side::Left) => true,
            Some(Side::Right) => false,
            _ => {
                self.next_left = !self.next_left;
                self.next_left
            }
        };
        self.clip = if left { ClipId::TapL } else { ClipId::TapR };
        self.frame = 0;
        self.frame_start = now;
        true
    }

    pub fn poke(&mut self) {
        self.sleep_at = now_ms() + self.sleep_after_ms;
    }
}
