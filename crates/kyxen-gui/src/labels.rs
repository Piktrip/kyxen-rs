//! Key legends for the on-screen keyboard and combo chips.

/// `(main legend, shifted legend)` for a keymap key name on `layout`.
#[allow(clippy::too_many_lines)] // one flat lookup table
pub fn legend(name: &str, layout: &str) -> (String, String) {
    let german = matches!(layout, "de" | "at");
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    let one = |a: &str| (a.to_owned(), String::new());

    if german {
        let de = match name {
            "BACKTICK" => Some(pair("^", "°")),
            "1" => Some(pair("1", "!")),
            "2" => Some(pair("2", "\"")),
            "3" => Some(pair("3", "§")),
            "4" => Some(pair("4", "$")),
            "5" => Some(pair("5", "%")),
            "6" => Some(pair("6", "&")),
            "7" => Some(pair("7", "/")),
            "8" => Some(pair("8", "(")),
            "9" => Some(pair("9", ")")),
            "0" => Some(pair("0", "=")),
            "MINUS" => Some(pair("ß", "?")),
            "EQUALS" => Some(pair("´", "`")),
            "L_BRACKET" => Some(one("Ü")),
            "R_BRACKET" => Some(pair("+", "*")),
            "SEMICOLON" => Some(one("Ö")),
            "APOSTROPHE" => Some(one("Ä")),
            "HASH" | "ANSI_BACKSLASH" => Some(pair("#", "'")),
            "BACKSLASH" => Some(pair("<", ">")),
            "Z" => Some(one("Y")),
            "Y" => Some(one("Z")),
            "COMMA" => Some(pair(",", ";")),
            "PERIOD" => Some(pair(".", ":")),
            "R_SLASH" => Some(pair("-", "_")),
            "L_CTRL" | "R_CTRL" => Some(one("Strg")),
            "R_ALT" => Some(one("Alt Gr")),
            "CAPS_LOCK" => Some(one("Feststell")),
            "INSERT" => Some(one("Einfg")),
            "DELETE" => Some(one("Entf")),
            "HOME" => Some(one("Pos1")),
            "END" => Some(one("Ende")),
            "PAGE_UP" => Some(one("Bild↑")),
            "PAGE_DOWN" => Some(one("Bild↓")),
            "PRINT_SCREEN" => Some(one("Druck")),
            "SCROLL_LOCK" => Some(one("Rollen")),
            "NUM_PERIOD" => Some(one(",")),
            _ => None,
        };
        if let Some(l) = de {
            return l;
        }
    }

    if layout == "gb" {
        let gb = match name {
            "BACKTICK" => Some(pair("`", "¬")),
            "2" => Some(pair("2", "\"")),
            "3" => Some(pair("3", "£")),
            "APOSTROPHE" => Some(pair("'", "@")),
            "HASH" | "ANSI_BACKSLASH" => Some(pair("#", "~")),
            _ => None,
        };
        if let Some(l) = gb {
            return l;
        }
    }

    match name {
        "BACKTICK" => pair("`", "~"),
        "1" => pair("1", "!"),
        "2" => pair("2", "@"),
        "3" => pair("3", "#"),
        "4" => pair("4", "$"),
        "5" => pair("5", "%"),
        "6" => pair("6", "^"),
        "7" => pair("7", "&"),
        "8" => pair("8", "*"),
        "9" => pair("9", "("),
        "0" => pair("0", ")"),
        "MINUS" => pair("-", "_"),
        "EQUALS" => pair("=", "+"),
        "L_BRACKET" => pair("[", "{"),
        "R_BRACKET" => pair("]", "}"),
        "SEMICOLON" => pair(";", ":"),
        "APOSTROPHE" => pair("'", "\""),
        "HASH" | "BACKSLASH" | "ANSI_BACKSLASH" => pair("\\", "|"),
        "COMMA" => pair(",", "<"),
        "PERIOD" => pair(".", ">"),
        "R_SLASH" => pair("/", "?"),
        "ESC" => one("Esc"),
        "BACKSPACE" => one("⟵"),
        "TAB" => one("Tab"),
        "RETURN" | "NUM_ENTER" => one("⏎"),
        "CAPS_LOCK" => one("Caps"),
        "L_SHIFT" | "R_SHIFT" => one("⇧"),
        "L_CTRL" | "R_CTRL" => one("Ctrl"),
        "L_SUPER" | "R_SUPER" => one("❖"),
        "L_ALT" => one("Alt"),
        "R_ALT" => one("AltGr"),
        "CONTEXT" => one("☰"),
        "SPACE" => one(""),
        "INSERT" => one("Ins"),
        "DELETE" => one("Del"),
        "HOME" => one("Home"),
        "END" => one("End"),
        "PAGE_UP" => one("PgUp"),
        "PAGE_DOWN" => one("PgDn"),
        "PRINT_SCREEN" => one("Print"),
        "SCROLL_LOCK" => one("Scroll"),
        "PAUSE" => one("Pause"),
        "UP" => one("↑"),
        "DOWN" => one("↓"),
        "LEFT" => one("←"),
        "RIGHT" => one("→"),
        "NUM_LOCK" => one("Num"),
        "NUM_SLASH" => one("/"),
        "NUM_STAR" => one("*"),
        "NUM_MINUS" => one("−"),
        "NUM_PLUS" => one("+"),
        "NUM_PERIOD" => one("."),
        "MEDIA_PREV" => one("⏮"),
        "PLAY_PAUSE" => one("⏯"),
        "MEDIA_NEXT" => one("⏭"),
        "MUTE" => one("🔇"),
        "LOGO" => one("G"),
        "GAME_MODE" => one("GM"),
        "ILLUMINATION" => one("☀"),
        n if n.starts_with("NUM_") => one(&n[4..]),
        n => one(n),
    }
}

/// Display text for a combo key name as stored in profiles (`ctrl`, `z`, `f5` …).
pub fn chip(name: &str, layout: &str) -> String {
    let german = matches!(layout, "de" | "at");
    match name {
        "ctrl" | "lctrl" | "control" | "rctrl" => if german { "Strg" } else { "Ctrl" }.into(),
        "shift" | "lshift" | "rshift" => "Shift".into(),
        "alt" | "lalt" => "Alt".into(),
        "ralt" | "altgr" => "AltGr".into(),
        "super" | "win" | "meta" | "cmd" => "Super".into(),
        "enter" | "return" => "Enter".into(),
        "tab" => "Tab".into(),
        "esc" | "escape" => "Esc".into(),
        "space" => if german { "Leertaste" } else { "Space" }.into(),
        "backspace" => "⟵".into(),
        "delete" | "del" => if german { "Entf" } else { "Del" }.into(),
        "insert" | "ins" => if german { "Einfg" } else { "Ins" }.into(),
        "home" => if german { "Pos1" } else { "Home" }.into(),
        "end" => if german { "Ende" } else { "End" }.into(),
        "pageup" | "pgup" => if german { "Bild↑" } else { "PgUp" }.into(),
        "pagedown" | "pgdn" => if german { "Bild↓" } else { "PgDn" }.into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "menu" => "Menü".into(),
        "printscreen" => if german { "Druck" } else { "Print" }.into(),
        "pause" => "Pause".into(),
        "capslock" | "caps_lock" => "Caps".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwertz_legends() {
        assert_eq!(legend("Z", "at").0, "Y");
        assert_eq!(legend("Y", "de").0, "Z");
        assert_eq!(legend("MINUS", "at"), ("ß".into(), "?".into()));
        assert_eq!(legend("Z", "us").0, "Z");
        assert_eq!(legend("NUM_7", "us").0, "7");
        assert_eq!(legend("ANSI_BACKSLASH", "us").0, "\\");
        assert_eq!(legend("ANSI_BACKSLASH", "de").0, "#");
        assert_eq!(legend("3", "gb").1, "£");
        assert_eq!(chip("ctrl", "at"), "Strg");
        assert_eq!(chip("f13", "us"), "F13");
    }
}
