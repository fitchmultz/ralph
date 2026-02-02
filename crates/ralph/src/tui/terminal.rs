//! Terminal capability detection and configuration.
//!
//! Responsibilities:
//! - Detect terminal color support (none, 16-color, 256-color, truecolor).
//! - Detect Unicode support.
//! - Detect mouse support availability.
//! - Provide terminal configuration based on environment and CLI flags.
//!
//! Not handled here:
//! - Actual terminal setup/teardown (see app.rs).
//! - Rendering decisions (see render module).
//!
//! Invariants/assumptions:
//! - Environment variables are read at TUI startup and cached.
//! - Detection is conservative: when in doubt, use simpler features.

use std::env;

/// Color support levels for terminal rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSupport {
    /// No color support (monochrome).
    None,
    /// 16-color ANSI support.
    Ansi16,
    /// 256-color ANSI support.
    Ansi256,
    /// Truecolor (24-bit) support.
    #[default]
    TrueColor,
}

impl ColorSupport {
    /// Detect color support from environment variables.
    ///
    /// Detection order:
    /// 1. Check `NO_COLOR` - if set, disable colors entirely.
    /// 2. Check `COLORTERM` for `truecolor` or `24bit`.
    /// 3. Check `TERM` for `-256color` suffix.
    /// 4. Check `TERM` for `xterm`, `screen`, `tmux` (assume 16-color minimum).
    /// 5. Default to TrueColor for unknown terminals (modern default).
    pub fn detect() -> Self {
        // NO_COLOR takes precedence - disable colors entirely.
        if env::var("NO_COLOR").is_ok() {
            return ColorSupport::None;
        }

        // Check COLORTERM for truecolor support.
        if let Ok(colorterm) = env::var("COLORTERM") {
            let colorterm = colorterm.to_lowercase();
            if colorterm.contains("truecolor") || colorterm.contains("24bit") {
                return ColorSupport::TrueColor;
            }
        }

        // Check TERM for color capabilities.
        if let Ok(term) = env::var("TERM") {
            let term_lower = term.to_lowercase();

            // 256-color support.
            if term_lower.contains("256color") {
                return ColorSupport::Ansi256;
            }

            // Basic ANSI color support for common terminals.
            if term_lower.starts_with("xterm")
                || term_lower.starts_with("screen")
                || term_lower.starts_with("tmux")
                || term_lower.starts_with("rxvt")
                || term_lower.starts_with("vt100")
                || term_lower.starts_with("linux")
            {
                return ColorSupport::Ansi16;
            }
        }

        // Check TERM_PROGRAM for specific terminals.
        if let Ok(term_program) = env::var("TERM_PROGRAM") {
            match term_program.as_str() {
                "Apple_Terminal" | "iTerm.app" | "WezTerm" | "Alacritty" | "kitty" => {
                    return ColorSupport::TrueColor;
                }
                "vscode" => {
                    // VS Code terminal typically supports truecolor.
                    return ColorSupport::TrueColor;
                }
                _ => {}
            }
        }

        // Default to truecolor for modern terminals when uncertain.
        ColorSupport::TrueColor
    }

    /// Returns true if any color is supported.
    pub fn has_color(self) -> bool {
        !matches!(self, ColorSupport::None)
    }

    /// Returns true if 256 colors or more are supported.
    pub fn has_256_colors(self) -> bool {
        matches!(self, ColorSupport::Ansi256 | ColorSupport::TrueColor)
    }

    /// Returns true if truecolor (24-bit) is supported.
    pub fn has_truecolor(self) -> bool {
        matches!(self, ColorSupport::TrueColor)
    }
}

/// Unicode support detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnicodeSupport {
    /// Unicode is not supported, use ASCII fallbacks.
    None,
    /// Basic Unicode support (BMP characters).
    Basic,
    /// Full Unicode support.
    #[default]
    Full,
}

impl UnicodeSupport {
    /// Detect Unicode support from environment.
    ///
    /// Detection:
    /// 1. Check `LANG` and `LC_ALL` for UTF-8 indicator.
    /// 2. Check `TERM` for known Unicode-capable terminals.
    /// 3. Default to Full for modern systems.
    pub fn detect() -> Self {
        // Check locale environment variables for UTF-8.
        let check_utf8 = |var: &str| {
            env::var(var)
                .ok()
                .map(|v| v.to_lowercase().contains("utf-8") || v.to_lowercase().contains("utf8"))
                .unwrap_or(false)
        };

        if check_utf8("LANG") || check_utf8("LC_ALL") || check_utf8("LC_CTYPE") {
            return UnicodeSupport::Full;
        }

        // Check TERM for known Unicode-capable terminals.
        if let Ok(term) = env::var("TERM") {
            let term_lower = term.to_lowercase();
            if term_lower.contains("utf8") || term_lower.contains("unicode") {
                return UnicodeSupport::Full;
            }
        }

        // Default to full Unicode for modern systems.
        UnicodeSupport::Full
    }

    /// Returns true if Unicode is supported.
    pub fn has_unicode(self) -> bool {
        !matches!(self, UnicodeSupport::None)
    }
}

/// Mouse support detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseSupport {
    /// Mouse is not supported.
    None,
    /// Basic mouse support (clicks only).
    Basic,
    /// Full mouse support (clicks, scroll, drag).
    #[default]
    Full,
}

impl MouseSupport {
    /// Detect mouse support from environment.
    ///
    /// Most modern terminals support mouse, but some environments
    /// like screen or certain SSH clients may have issues.
    pub fn detect() -> Self {
        // Check for terminals known to have mouse issues.
        if let Ok(term) = env::var("TERM") {
            let term_lower = term.to_lowercase();

            // Linux console doesn't support mouse.
            if term_lower == "linux" {
                return MouseSupport::None;
            }
        }

        // Check if running in a multiplexer that might have mouse issues.
        if env::var("TMUX").is_ok() {
            // tmux supports mouse but may need configuration.
            return MouseSupport::Full;
        }

        if env::var("STY").is_ok() {
            // GNU screen - mouse support varies by version/config.
            return MouseSupport::Basic;
        }

        // Default to full mouse support for modern terminals.
        MouseSupport::Full
    }

    /// Returns true if mouse is supported.
    pub fn has_mouse(self) -> bool {
        !matches!(self, MouseSupport::None)
    }
}

/// Terminal capabilities detected at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalCapabilities {
    /// Color support level.
    pub colors: ColorSupport,
    /// Unicode support level.
    pub unicode: UnicodeSupport,
    /// Mouse support level.
    pub mouse: MouseSupport,
}

impl TerminalCapabilities {
    /// Detect all terminal capabilities from environment.
    pub fn detect() -> Self {
        Self {
            colors: ColorSupport::detect(),
            unicode: UnicodeSupport::detect(),
            mouse: MouseSupport::detect(),
        }
    }

    /// Returns true if the terminal supports colors.
    pub fn has_colors(self) -> bool {
        self.colors.has_color()
    }

    /// Returns true if the terminal supports Unicode.
    pub fn has_unicode(self) -> bool {
        self.unicode.has_unicode()
    }

    /// Returns true if the terminal supports mouse input.
    pub fn has_mouse(self) -> bool {
        self.mouse.has_mouse()
    }
}

/// Border style options for TUI rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    /// Use Unicode box-drawing characters.
    #[default]
    Unicode,
    /// Use ASCII characters (+, -, |).
    Ascii,
}

impl BorderStyle {
    /// Get the appropriate border style based on capabilities and user preference.
    pub fn for_capabilities(caps: TerminalCapabilities, force_ascii: bool) -> Self {
        if force_ascii || !caps.has_unicode() {
            BorderStyle::Ascii
        } else {
            BorderStyle::Unicode
        }
    }

    /// Get the horizontal line character.
    pub fn horizontal(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "─",
            BorderStyle::Ascii => "-",
        }
    }

    /// Get the vertical line character.
    pub fn vertical(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "│",
            BorderStyle::Ascii => "|",
        }
    }

    /// Get the top-left corner character.
    pub fn top_left(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┌",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the top-right corner character.
    pub fn top_right(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┐",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the bottom-left corner character.
    pub fn bottom_left(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "└",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the bottom-right corner character.
    pub fn bottom_right(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┘",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the left T-junction character.
    pub fn t_left(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "├",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the right T-junction character.
    pub fn t_right(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┤",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the top T-junction character.
    pub fn t_top(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┬",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the bottom T-junction character.
    pub fn t_bottom(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┴",
            BorderStyle::Ascii => "+",
        }
    }

    /// Get the cross junction character.
    pub fn cross(self) -> &'static str {
        match self {
            BorderStyle::Unicode => "┼",
            BorderStyle::Ascii => "+",
        }
    }
}

/// Color option for CLI argument parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorOption {
    /// Auto-detect based on terminal capabilities.
    #[default]
    Auto,
    /// Always use colors.
    Always,
    /// Never use colors.
    Never,
}

impl ColorOption {
    /// Resolve the color option to a color support level.
    pub fn resolve(self, detected: ColorSupport) -> ColorSupport {
        match self {
            ColorOption::Auto => detected,
            ColorOption::Always => ColorSupport::TrueColor,
            ColorOption::Never => ColorSupport::None,
        }
    }
}

impl std::str::FromStr for ColorOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ColorOption::Auto),
            "always" => Ok(ColorOption::Always),
            "never" => Ok(ColorOption::Never),
            _ => Err(format!("unknown color option: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_support_has_color() {
        assert!(!ColorSupport::None.has_color());
        assert!(ColorSupport::Ansi16.has_color());
        assert!(ColorSupport::Ansi256.has_color());
        assert!(ColorSupport::TrueColor.has_color());
    }

    #[test]
    fn color_support_has_256_colors() {
        assert!(!ColorSupport::None.has_256_colors());
        assert!(!ColorSupport::Ansi16.has_256_colors());
        assert!(ColorSupport::Ansi256.has_256_colors());
        assert!(ColorSupport::TrueColor.has_256_colors());
    }

    #[test]
    fn color_support_has_truecolor() {
        assert!(!ColorSupport::None.has_truecolor());
        assert!(!ColorSupport::Ansi16.has_truecolor());
        assert!(!ColorSupport::Ansi256.has_truecolor());
        assert!(ColorSupport::TrueColor.has_truecolor());
    }

    #[test]
    fn unicode_support_has_unicode() {
        assert!(!UnicodeSupport::None.has_unicode());
        assert!(UnicodeSupport::Basic.has_unicode());
        assert!(UnicodeSupport::Full.has_unicode());
    }

    #[test]
    fn mouse_support_has_mouse() {
        assert!(!MouseSupport::None.has_mouse());
        assert!(MouseSupport::Basic.has_mouse());
        assert!(MouseSupport::Full.has_mouse());
    }

    #[test]
    fn border_style_ascii() {
        let style = BorderStyle::Ascii;
        assert_eq!(style.horizontal(), "-");
        assert_eq!(style.vertical(), "|");
        assert_eq!(style.top_left(), "+");
        assert_eq!(style.top_right(), "+");
        assert_eq!(style.bottom_left(), "+");
        assert_eq!(style.bottom_right(), "+");
    }

    #[test]
    fn border_style_unicode() {
        let style = BorderStyle::Unicode;
        assert_eq!(style.horizontal(), "─");
        assert_eq!(style.vertical(), "│");
        assert_eq!(style.top_left(), "┌");
        assert_eq!(style.top_right(), "┐");
        assert_eq!(style.bottom_left(), "└");
        assert_eq!(style.bottom_right(), "┘");
    }

    #[test]
    fn border_style_for_capabilities() {
        let caps = TerminalCapabilities {
            colors: ColorSupport::TrueColor,
            unicode: UnicodeSupport::Full,
            mouse: MouseSupport::Full,
        };
        assert_eq!(
            BorderStyle::for_capabilities(caps, false),
            BorderStyle::Unicode
        );
        assert_eq!(
            BorderStyle::for_capabilities(caps, true),
            BorderStyle::Ascii
        );

        let caps_no_unicode = TerminalCapabilities {
            colors: ColorSupport::TrueColor,
            unicode: UnicodeSupport::None,
            mouse: MouseSupport::Full,
        };
        assert_eq!(
            BorderStyle::for_capabilities(caps_no_unicode, false),
            BorderStyle::Ascii
        );
    }

    #[test]
    fn color_option_parse() {
        use std::str::FromStr;
        assert_eq!(ColorOption::from_str("auto"), Ok(ColorOption::Auto));
        assert_eq!(ColorOption::from_str("Auto"), Ok(ColorOption::Auto));
        assert_eq!(ColorOption::from_str("AUTO"), Ok(ColorOption::Auto));
        assert_eq!(ColorOption::from_str("always"), Ok(ColorOption::Always));
        assert_eq!(ColorOption::from_str("Always"), Ok(ColorOption::Always));
        assert_eq!(ColorOption::from_str("never"), Ok(ColorOption::Never));
        assert_eq!(ColorOption::from_str("Never"), Ok(ColorOption::Never));
        assert!(ColorOption::from_str("invalid").is_err());
    }

    #[test]
    fn color_option_resolve() {
        assert_eq!(
            ColorOption::Auto.resolve(ColorSupport::Ansi256),
            ColorSupport::Ansi256
        );
        assert_eq!(
            ColorOption::Always.resolve(ColorSupport::None),
            ColorSupport::TrueColor
        );
        assert_eq!(
            ColorOption::Never.resolve(ColorSupport::TrueColor),
            ColorSupport::None
        );
    }
}
