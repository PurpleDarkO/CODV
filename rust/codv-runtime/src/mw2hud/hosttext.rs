//! The host's own lines in MW2's HUD fonts (cv_mw2_text): a game mode's wave, what is left and its time,
//! drawn centred near the top of the screen with MW2's text style. The host sends them every frame; lines
//! not sent again for a moment are gone (the mode ended, the host stopped).
use super::*;

/// Lines not sent again within this long are dropped (ms).
const STALE_MS: i32 = 500;

/// One line: centred, `y` virtual units from the top (640 x 480), MW2's font `font` (9: the big title
/// font, 10: the normal one) at `scale`, with an optional glow under the letters.
#[derive(Clone, Debug, PartialEq)]
pub struct HostLine {
    pub text: String,
    pub y: f32,
    pub scale: f32,
    pub font: i32,
    pub color: [f32; 4],
    pub glow: Option<[f32; 4]>,
}

impl Mw2Hud {
    /// The host's lines for the frames to come (all of them, replacing the last ones).
    pub fn set_host_text(&mut self, lines: Vec<HostLine>) {
        self.host_lines = lines;
        self.host_lines_at = self.time_ms;
    }

    pub(super) fn host_text(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        if self.host_lines.is_empty() || self.time_ms.wrapping_sub(self.host_lines_at) > STALE_MS {
            return;
        }
        let lines = self.host_lines.clone();
        for line in &lines {
            let item = Item {
                rect: [0.0, line.y, 0.0, 0.0],
                align: [h::ALIGN_CENTER, h::ALIGN_VIEWABLE],
                color: line.color,
                text_scale: line.scale,
                font: line.font,
                text_align: 1,
                text_style: 3,
                ..Default::default()
            };
            self.item_text_glow(out, sp, &item, &line.text, line.color, line.glow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_lines_show_while_sent_and_go_when_stale() {
        let mut hud = Mw2Hud::new(Hud::default());
        let line =
            HostLine { text: "WAVE 3".into(), y: 20.0, scale: 0.5, font: 9, color: [0.8, 0.1, 0.1, 1.0], glow: None };
        hud.set_host_text(vec![line.clone()]);
        let sp = h::ScreenPlacement::setup_fullscreen(1920.0, 1080.0, 1.0);
        let mut out = Vec::new();
        hud.host_text(&mut out, &sp);
        assert_eq!(hud.host_lines, vec![line], "kept for the next frames");
        hud.time_ms += STALE_MS + 1;
        let mut out = Vec::new();
        hud.host_text(&mut out, &sp);
        assert!(out.is_empty(), "not sent again: gone");
    }
}
