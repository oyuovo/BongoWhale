//! Maps a virtual-key code to the hand that should hit the desk.
//!
//! Classic Bongo Cat: keys on the left half of the keyboard drive the left paw,
//! keys on the right half drive the right paw. Anything that has no clear side
//! (space, Enter, arrows, media keys) alternates.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
    /// No inherent side: alternate on every press.
    Any,
}

pub fn side_for(vk: u16) -> Side {
    use Side::*;
    match vk {
        // left-hand modifiers
        0xA0 | 0xA2 | 0xA4 | 0x5B => Left, // LShift, LCtrl, LAlt, LWin
        // right-hand modifiers
        0xA1 | 0xA3 | 0xA5 | 0x5C => Right, // RShift, RCtrl, RAlt, RWin

        // letters: QWERTY split between T/G/B and Y/H/N
        0x51 | 0x57 | 0x45 | 0x52 | 0x54 => Left,  // Q W E R T
        0x41 | 0x53 | 0x44 | 0x46 | 0x47 => Left,  // A S D F G
        0x5A | 0x58 | 0x43 | 0x56 | 0x42 => Left,  // Z X C V B
        0x59 | 0x55 | 0x49 | 0x4F | 0x50 => Right, // Y U I O P
        0x48 | 0x4A | 0x4B | 0x4C => Right,        // H J K L
        0x4E | 0x4D => Right,                      // N M

        // digits on the main row: 1-5 left, 6-0 right
        0x31..=0x35 => Left,
        0x30 | 0x36..=0x39 => Right,

        // numpad sits on the right
        0x60..=0x6F => Right,

        // punctuation follows the row it sits on
        0xC0 | 0xDB | 0xDC | 0xDD | 0xBA | 0xDE => Left, // backtick [ ] ; ' .
        0xBF | 0xBB | 0xBC | 0xBD | 0xBE => Right,       // / = , - .

        // space, enter, backspace, tab, esc, arrows, function keys: alternate
        _ => Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_split_on_the_qwerty_middle() {
        assert_eq!(side_for(0x51), Side::Left); // Q
        assert_eq!(side_for(0x54), Side::Left); // T
        assert_eq!(side_for(0x42), Side::Left); // B
        assert_eq!(side_for(0x59), Side::Right); // Y
        assert_eq!(side_for(0x50), Side::Right); // P
        assert_eq!(side_for(0x4E), Side::Right); // N
    }

    #[test]
    fn digits_and_numpad() {
        assert_eq!(side_for(0x31), Side::Left); // 1
        assert_eq!(side_for(0x35), Side::Left); // 5
        assert_eq!(side_for(0x36), Side::Right); // 6
        assert_eq!(side_for(0x30), Side::Right); // 0
        assert_eq!(side_for(0x60), Side::Right); // numpad 0
    }

    #[test]
    fn modifiers_keep_their_side() {
        assert_eq!(side_for(0xA0), Side::Left); // LShift
        assert_eq!(side_for(0xA1), Side::Right); // RShift
        assert_eq!(side_for(0xA2), Side::Left); // LCtrl
        assert_eq!(side_for(0xA3), Side::Right); // RCtrl
    }

    #[test]
    fn neutral_keys_alternate() {
        assert_eq!(side_for(0x20), Side::Any); // Space
        assert_eq!(side_for(0x0D), Side::Any); // Enter
        assert_eq!(side_for(0x25), Side::Any); // Left arrow
    }
}
