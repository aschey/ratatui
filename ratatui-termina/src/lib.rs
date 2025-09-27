// show the feature flags in the generated documentation
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/ratatui/ratatui/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/ratatui/ratatui/main/assets/favicon.ico"
)]
#![warn(missing_docs)]
//! This crate provides [`TerminaBackend`], an implementation of the [`Backend`] trait for the
//! [Ratatui] library. It uses the [termina] library for all terminal manipulation.
//!
//! ## termina Version and Re-export
//!
//! `ratatui-termina` requires you to specify a version of the [termina] library to be used.
//! This is managed via feature flags. The highest enabled feature flag of the available
//! `termina_0_xx` features (e.g., `termina_0_28`, `termina_0_29`) takes precedence. These
//! features determine which version of termina is compiled and used by the backend. Feature
//! unification may mean that any crate in your dependency graph that chooses to depend on a
//! specific version of termina may be affected by the feature flags you enable.
//!
//! Ratatui will support at least the two most recent versions of termina (though we may increase
//! this if termina release cadence increases). We will remove support for older versions in major
//! (0.x) releases of `ratatui-termina`, and we may add support for newer versions in minor
//! (0.x.y) releases.
//!
//! To promote interoperability within the [Ratatui] ecosystem, the selected termina crate is
//! re-exported as `ratatui_termina::termina`. This re-export is essential for authors of widget
//! libraries or any applications that need to perform direct termina operations while ensuring
//! compatibility with the version used by `ratatui-termina`. By using
//! `ratatui_termina::termina` for such operations, developers can avoid version conflicts and
//! ensure that all parts of their application use a consistent set of termina types and
//! functions.
//!
//! For example, if your application's `Cargo.toml` enables the `termina_0_29` feature for
//! `ratatui-termina`, then any code using `ratatui_termina::termina` will refer to the 0.29
//! version of termina.
//!
//! For more information on how to use the backend, see the documentation for the
//! [`TerminaBackend`] struct.
//!
//! [Ratatui]: https://ratatui.rs
//! [termina]: https://crates.io/crates/termina
//! [`Backend`]: ratatui_core::backend::Backend
//!
//! # Crate Organization
//!
//! `ratatui-termina` is part of the Ratatui workspace that was modularized in version 0.30.0.
//! This crate provides the [termina] backend implementation for Ratatui.
//!
//! **When to use `ratatui-termina`:**
//!
//! - You need fine-grained control over dependencies
//! - Building a widget library that needs backend functionality
//! - You want to use only the termina backend without other backends
//!
//! **When to use the main [`ratatui`] crate:**
//!
//! - Building applications (recommended - includes termina backend by default)
//! - You want the convenience of having everything available
//!
//! For detailed information about the workspace organization, see [ARCHITECTURE.md].
//!
//! [`ratatui`]: https://crates.io/crates/ratatui
//! [ARCHITECTURE.md]: https://github.com/ratatui/ratatui/blob/main/ARCHITECTURE.md
#![cfg_attr(feature = "document-features", doc = "\n## Features")]
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]

use std::io::{self, Write};

use ratatui_core::backend::{Backend, ClearType, WindowSize};
use ratatui_core::buffer::Cell;
use ratatui_core::layout::{Position, Size};
use ratatui_core::style::{Color, Modifier};
use termina::escape::csi::{self, Csi, SgrAttributes, SgrModifiers};
use termina::style::{ColorSpec, RgbColor, RgbaColor};
use termina::{Event, OneBased, PlatformTerminal, Terminal};

/// A [`Backend`] implementation that uses [termina] to render to the terminal.
///
/// The `TerminaBackend` struct is a wrapper around a writer implementing [`Write`], which is
/// used to send commands to the terminal. It provides methods for drawing content, manipulating
/// the cursor, and clearing the terminal screen.
///
/// Most applications should not call the methods on `TerminaBackend` directly, but will instead
/// use the [`Terminal`] struct, which provides a more ergonomic interface.
///
/// Usually applications will enable raw mode and switch to alternate screen mode after creating
/// a `TerminaBackend`. This is done by calling [`termina::terminal::enable_raw_mode`] and
/// [`termina::terminal::EnterAlternateScreen`] (and the corresponding disable/leave functions
/// when the application exits). This is not done automatically by the backend because it is
/// possible that the application may want to use the terminal for other purposes (like showing
/// help text) before entering alternate screen mode.
///
/// # Example
///
/// ```rust,ignore
/// use std::io::{stderr, stdout};
///
/// use termina::ExecutableCommand;
/// use termina::terminal::{
///     EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
/// };
/// use ratatui::Terminal;
/// use ratatui::backend::TerminaBackend;
///
/// let mut backend = TerminaBackend::new(stdout());
/// // or
/// let backend = TerminaBackend::new(stderr());
/// let mut terminal = Terminal::new(backend)?;
///
/// enable_raw_mode()?;
/// stdout().execute(EnterAlternateScreen)?;
///
/// terminal.clear()?;
/// terminal.draw(|frame| {
///     // -- snip --
/// })?;
///
/// stdout().execute(LeaveAlternateScreen)?;
/// disable_raw_mode()?;
///
/// # std::io::Result::Ok(())
/// ```
///
/// See the the [Examples] directory for more examples. See the [`backend`] module documentation
/// for more details on raw mode and alternate screen.
///
/// [`Write`]: std::io::Write
/// [`Terminal`]: https://docs.rs/ratatui/latest/ratatui/struct.Terminal.html
/// [`backend`]: ratatui_core::backend
/// [termina]: https://crates.io/crates/termina
/// [Examples]: https://github.com/ratatui/ratatui/tree/main/ratatui/examples/README.md
#[derive(Debug)]
pub struct TerminaBackend<W: Write> {
    /// The writer used to send commands to the terminal.
    terminal: PlatformTerminal,
    writer: W,
}

macro_rules! decset {
    ($mode:ident) => {
        Csi::Mode(csi::Mode::SetDecPrivateMode(csi::DecPrivateMode::Code(
            csi::DecPrivateModeCode::$mode,
        )))
    };
}
macro_rules! decreset {
    ($mode:ident) => {
        Csi::Mode(csi::Mode::ResetDecPrivateMode(csi::DecPrivateMode::Code(
            csi::DecPrivateModeCode::$mode,
        )))
    };
}

impl<W> TerminaBackend<W>
where
    W: Write,
{
    /// Creates a new `TerminaBackend` with the given writer.
    ///
    /// Most applications will use either [`stdout`](std::io::stdout) or
    /// [`stderr`](std::io::stderr) as writer. See the [FAQ] to determine which one to use.
    ///
    /// [FAQ]: https://ratatui.rs/faq/#should-i-use-stdout-or-stderr
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use std::io::stdout;
    ///
    /// use ratatui::backend::TerminaBackend;
    ///
    /// let backend = TerminaBackend::new(stdout());
    /// ```
    pub const fn new(terminal: PlatformTerminal, writer: W) -> Self {
        Self { terminal, writer }
    }

    /// Gets the writer.
    #[instability::unstable(
        feature = "backend-writer",
        issue = "https://github.com/ratatui/ratatui/pull/991"
    )]
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    pub const fn terminal(&self) -> &PlatformTerminal {
        &self.terminal
    }

    pub const fn terminal_mut(&mut self) -> &mut PlatformTerminal {
        &mut self.terminal
    }

    /// Gets the writer as a mutable reference.
    ///
    /// Note: writing to the writer may cause incorrect output after the write. This is due to the
    /// way that the Terminal implements diffing Buffers.
    #[instability::unstable(
        feature = "backend-writer",
        issue = "https://github.com/ratatui/ratatui/pull/991"
    )]
    pub const fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    fn erase_in_display(&mut self, erase_in_display: csi::EraseInDisplay) -> io::Result<()> {
        write!(
            self.writer,
            "{}",
            Csi::Edit(csi::Edit::EraseInDisplay(erase_in_display))
        )?;
        self.writer.flush()
    }

    fn erase_in_line(&mut self, erase_in_line: csi::EraseInLine) -> io::Result<()> {
        write!(
            self.writer,
            "{}",
            Csi::Edit(csi::Edit::EraseInLine(erase_in_line))
        )?;
        self.writer.flush()
    }
}

impl<W> Write for TerminaBackend<W>
where
    W: Write,
{
    /// Writes a buffer of bytes to the underlying buffer.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    /// Flushes the underlying buffer.
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<W> Backend for TerminaBackend<W>
where
    W: Write,
{
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        #[cfg(feature = "underline-color")]
        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        let mut last_pos: Option<Position> = None;
        for (x, y, cell) in content {
            // Move the cursor if the previous location was not (x - 1, y)
            if !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y) {
                write!(
                    self.writer,
                    "{}",
                    Csi::Cursor(csi::Cursor::Position {
                        col: OneBased::from_zero_based(x),
                        line: OneBased::from_zero_based(y),
                    })
                )?;
            }
            last_pos = Some(Position { x, y });

            let mut attributes = SgrAttributes::default();
            if cell.fg != fg {
                attributes.foreground = Some(cell.fg.into_termina());
                fg = cell.fg;
            }
            if cell.bg != bg {
                attributes.background = Some(cell.bg.into_termina());
                bg = cell.bg;
            }
            if cell.modifier != modifier {
                attributes.modifiers = diff_modifiers(modifier, cell.modifier);
                modifier = cell.modifier;
            }
            #[cfg(feature = "underline-color")]
            if cell.underline_color != underline_color {
                write!(
                    self.writer,
                    "{}",
                    Csi::Sgr(csi::Sgr::UnderlineColor(
                        cell.underline_color.into_termina()
                    ))
                )?;
                underline_color = cell.underline_color;
            }

            if !attributes.is_empty() {
                write!(
                    self.writer,
                    "{}",
                    Csi::Sgr(csi::Sgr::Attributes(attributes))
                )?;
            }

            write!(self.writer, "{}", &cell.symbol())?;
        }

        write!(self.writer, "{}", Csi::Sgr(csi::Sgr::Reset))
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        write!(self.writer, "{}", decreset!(ShowCursor))?;
        self.writer.flush()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        write!(self.writer, "{}", decset!(ShowCursor))?;
        self.writer.flush()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        write!(
            self.terminal,
            "{}",
            csi::Csi::Cursor(csi::Cursor::RequestActivePositionReport),
        )?;
        self.terminal.flush()?;
        let event = self.terminal.read(|event| {
            matches!(
                event,
                Event::Csi(Csi::Cursor(csi::Cursor::ActivePositionReport { .. }))
            )
        })?;
        let Event::Csi(Csi::Cursor(csi::Cursor::ActivePositionReport { line, col })) = event else {
            unreachable!();
        };
        Ok(Position {
            x: col.get_zero_based(),
            y: line.get_zero_based(),
        })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position: Position = position.into();
        let col = OneBased::from_zero_based(position.x);
        let line = OneBased::from_zero_based(position.y);
        write!(
            self.writer,
            "{}",
            Csi::Cursor(csi::Cursor::Position { line, col })
        )?;
        self.writer.flush()
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clear_region(ClearType::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        match clear_type {
            ClearType::All => self.erase_in_display(csi::EraseInDisplay::EraseDisplay),
            ClearType::AfterCursor => {
                self.erase_in_display(csi::EraseInDisplay::EraseToEndOfDisplay)
            }
            ClearType::BeforeCursor => {
                self.erase_in_display(csi::EraseInDisplay::EraseToStartOfDisplay)
            }
            ClearType::CurrentLine => self.erase_in_line(csi::EraseInLine::EraseLine),
            ClearType::UntilNewLine => self.erase_in_line(csi::EraseInLine::EraseToEndOfLine),
        }
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        for _ in 0..n {
            writeln!(self.writer)?;
        }
        self.writer.flush()
    }

    fn size(&self) -> io::Result<Size> {
        let termina::WindowSize { rows, cols, .. } = self.terminal.get_dimensions()?;
        Ok(Size {
            width: cols,
            height: rows,
        })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let termina::WindowSize {
            rows,
            cols,
            pixel_width,
            pixel_height,
        } = self.terminal.get_dimensions()?;
        Ok(WindowSize {
            columns_rows: Size {
                width: cols,
                height: rows,
            },
            pixels: Size {
                width: pixel_width.unwrap_or_default(),
                height: pixel_height.unwrap_or_default(),
            },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    #[cfg(feature = "scrolling-regions")]
    fn scroll_region_up(&mut self, region: std::ops::Range<u16>, amount: u16) -> io::Result<()> {
        write!(
            self.terminal,
            "{}{}{}",
            Csi::Cursor(csi::Cursor::SetTopAndBottomMargins {
                top: OneBased::from_zero_based(region.start),
                bottom: OneBased::from_zero_based(region.end)
            }),
            Csi::Edit(csi::Edit::ScrollUp(u32::from(amount))),
            Csi::Cursor(csi::Cursor::SetTopAndBottomMargins {
                top: OneBased::from_zero_based(0),
                bottom: OneBased::from_zero_based(u16::MAX - 1)
            })
        )
    }

    #[cfg(feature = "scrolling-regions")]
    fn scroll_region_down(&mut self, region: std::ops::Range<u16>, amount: u16) -> io::Result<()> {
        write!(
            self.terminal,
            "{}{}{}",
            Csi::Cursor(csi::Cursor::SetTopAndBottomMargins {
                top: OneBased::from_zero_based(region.start),
                bottom: OneBased::from_zero_based(region.end)
            }),
            Csi::Edit(csi::Edit::ScrollDown(u32::from(amount))),
            Csi::Cursor(csi::Cursor::SetTopAndBottomMargins {
                top: OneBased::from_zero_based(0),
                bottom: OneBased::from_zero_based(u16::MAX - 1)
            })
        )
    }
}

/// A trait for converting a Ratatui type to a termina type.
///
/// This trait is needed for avoiding the orphan rule when implementing `From` for termina types
/// once these are moved to a separate crate.
pub trait IntoTermina<C> {
    /// Converts the ratatui type to a termina type.
    fn into_termina(self) -> C;
}

/// A trait for converting a termina type to a Ratatui type.
///
/// This trait is needed for avoiding the orphan rule when implementing `From` for termina types
/// once these are moved to a separate crate.
pub trait FromTermina<C> {
    /// Converts the termina type to a ratatui type.
    fn from_termina(value: C) -> Self;
}

impl IntoTermina<ColorSpec> for Color {
    fn into_termina(self) -> ColorSpec {
        match self {
            Self::Reset => ColorSpec::Reset,
            Self::Black => ColorSpec::BLACK,
            Self::Red => ColorSpec::RED,
            Self::Green => ColorSpec::GREEN,
            Self::Yellow => ColorSpec::YELLOW,
            Self::Blue => ColorSpec::BLUE,
            Self::Magenta => ColorSpec::MAGENTA,
            Self::Cyan => ColorSpec::CYAN,
            Self::Gray => ColorSpec::WHITE,
            Self::DarkGray => ColorSpec::BRIGHT_BLACK,
            Self::LightRed => ColorSpec::BRIGHT_RED,
            Self::LightGreen => ColorSpec::BRIGHT_GREEN,
            Self::LightBlue => ColorSpec::BRIGHT_BLUE,
            Self::LightYellow => ColorSpec::BRIGHT_YELLOW,
            Self::LightMagenta => ColorSpec::BRIGHT_MAGENTA,
            Self::LightCyan => ColorSpec::BRIGHT_CYAN,
            Self::White => ColorSpec::BRIGHT_WHITE,
            Self::Indexed(i) => ColorSpec::PaletteIndex(i),
            Self::Rgb(r, g, b) => ColorSpec::TrueColor(
                RgbColor {
                    red: r,
                    green: g,
                    blue: b,
                }
                .into(),
            ),
        }
    }
}

impl FromTermina<ColorSpec> for Color {
    fn from_termina(value: ColorSpec) -> Self {
        match value {
            ColorSpec::Reset => Self::Reset,
            ColorSpec::BLACK => Self::Black,
            ColorSpec::RED => Self::Red,
            ColorSpec::GREEN => Self::Green,
            ColorSpec::YELLOW => Self::Yellow,
            ColorSpec::BLUE => Self::Blue,
            ColorSpec::MAGENTA => Self::Magenta,
            ColorSpec::CYAN => Self::Cyan,
            ColorSpec::WHITE => Self::Gray,
            ColorSpec::BRIGHT_BLACK => Self::DarkGray,
            ColorSpec::BRIGHT_RED => Self::LightRed,
            ColorSpec::BRIGHT_GREEN => Self::LightGreen,
            ColorSpec::BRIGHT_BLUE => Self::LightBlue,
            ColorSpec::BRIGHT_YELLOW => Self::LightYellow,
            ColorSpec::BRIGHT_MAGENTA => Self::LightMagenta,
            ColorSpec::BRIGHT_CYAN => Self::LightCyan,
            ColorSpec::BRIGHT_WHITE => Self::White,
            ColorSpec::TrueColor(RgbaColor {
                red, green, blue, ..
            }) => Self::Rgb(red, green, blue),
            ColorSpec::PaletteIndex(v) => Self::Indexed(v),
        }
    }
}

fn diff_modifiers(from: Modifier, to: Modifier) -> SgrModifiers {
    let mut modifiers = SgrModifiers::default();

    let removed = from - to;
    if removed.contains(Modifier::REVERSED) {
        modifiers |= SgrModifiers::NO_REVERSE;
    }
    if removed.contains(Modifier::BOLD) && !to.contains(Modifier::DIM) {
        modifiers |= SgrModifiers::INTENSITY_NORMAL;
    }
    if removed.contains(Modifier::DIM) {
        modifiers |= SgrModifiers::INTENSITY_NORMAL;
    }
    if removed.contains(Modifier::ITALIC) {
        modifiers |= SgrModifiers::NO_ITALIC;
    }
    if removed.contains(Modifier::CROSSED_OUT) {
        modifiers |= SgrModifiers::NO_STRIKE_THROUGH;
    }
    if removed.contains(Modifier::HIDDEN) {
        modifiers |= SgrModifiers::NO_INVISIBLE;
    }
    if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
        modifiers |= SgrModifiers::BLINK_NONE;
    }

    let added = to - from;
    if added.contains(Modifier::REVERSED) {
        modifiers |= SgrModifiers::REVERSE;
    }
    if added.contains(Modifier::BOLD) {
        modifiers |= SgrModifiers::INTENSITY_BOLD;
    }
    if added.contains(Modifier::DIM) {
        modifiers |= SgrModifiers::INTENSITY_DIM;
    }
    if added.contains(Modifier::ITALIC) {
        modifiers |= SgrModifiers::ITALIC;
    }
    if added.contains(Modifier::CROSSED_OUT) {
        modifiers |= SgrModifiers::STRIKE_THROUGH;
    }
    if added.contains(Modifier::HIDDEN) {
        modifiers |= SgrModifiers::INVISIBLE;
    }
    if added.contains(Modifier::SLOW_BLINK) {
        modifiers |= SgrModifiers::BLINK_SLOW;
    }
    if added.contains(Modifier::RAPID_BLINK) {
        modifiers |= SgrModifiers::BLINK_RAPID;
    }

    modifiers
}

// impl FromTermina<SgrModifiers> for Modifier {
//     fn from_termina(value: SgrModifiers) -> Self {
//         let mut res = Self::empty();
//         if value.intersects(SgrModifiers::INTENSITY_BOLD) {
//             res |= Self::BOLD;
//         }
//         if value.intersects(SgrModifiers::INTENSITY_DIM) {
//             res |= Self::DIM;
//         }
//         if value.intersects(SgrModifiers::ITALIC) {
//             res |= Self::ITALIC;
//         }
//         if value.intersects(
//             SgrModifiers::UNDERLINE_SINGLE
//                 | SgrModifiers::UNDERLINE_DOUBLE
//                 | SgrModifiers::UNDERLINE_CURLY
//                 | SgrModifiers::UNDERLINE_DOTTED
//                 | SgrModifiers::UNDERLINE_DASHED,
//         ) {
//             res |= Self::UNDERLINED;
//         }
//         if value.intersects(SgrModifiers::BLINK_SLOW) {
//             res |= Self::SLOW_BLINK;
//         }
//         if value.intersects(SgrModifiers::BLINK_RAPID) {
//             res |= Self::RAPID_BLINK;
//         }
//         if value.intersects(SgrModifiers::REVERSE) {
//             res |= Self::REVERSED;
//         }
//         if value.intersects(SgrModifiers::INVISIBLE) {
//             res |= Self::HIDDEN;
//         }
//         if value.intersects(SgrModifiers::STRIKE_THROUGH) {
//             res |= Self::CROSSED_OUT;
//         }
//         res
//     }
// }

// impl FromTermina<Stylized<'_>> for Style {
//     fn from_termina(value: Stylized<'_>) -> Self {
//         let mut sub_modifier = Modifier::empty();
//         if value.has(terminaAttribute::NoBold) {
//             sub_modifier |= Modifier::BOLD;
//         }
//         if value.attributes.has(terminaAttribute::NoItalic) {
//             sub_modifier |= Modifier::ITALIC;
//         }
//         if value.attributes.has(terminaAttribute::NotCrossedOut) {
//             sub_modifier |= Modifier::CROSSED_OUT;
//         }
//         if value.attributes.has(terminaAttribute::NoUnderline) {
//             sub_modifier |= Modifier::UNDERLINED;
//         }
//         if value.attributes.has(terminaAttribute::NoHidden) {
//             sub_modifier |= Modifier::HIDDEN;
//         }
//         if value.attributes.has(terminaAttribute::NoBlink) {
//             sub_modifier |= Modifier::RAPID_BLINK | Modifier::SLOW_BLINK;
//         }
//         if value.attributes.has(terminaAttribute::NoReverse) {
//             sub_modifier |= Modifier::REVERSED;
//         }
//
//         Self {
//             fg: value.foreground_color.map(Fromtermina::from_termina),
//             bg: value.background_color.map(Fromtermina::from_termina),
//             #[cfg(feature = "underline-color")]
//             underline_color: value.underline_color.map(Fromtermina::from_termina),
//             add_modifier: Modifier::from_termina(value.attributes),
//             sub_modifier,
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
//
//     use super::*;
//
//     #[rstest]
//     #[case(ColorSpec::Reset, Color::Reset)]
//     #[case(ColorSpec::Black, Color::Black)]
//     #[case(ColorSpec::DarkGrey, Color::DarkGray)]
//     #[case(ColorSpec::Red, Color::LightRed)]
//     #[case(ColorSpec::DarkRed, Color::Red)]
//     #[case(ColorSpec::Green, Color::LightGreen)]
//     #[case(ColorSpec::DarkGreen, Color::Green)]
//     #[case(ColorSpec::Yellow, Color::LightYellow)]
//     #[case(ColorSpec::DarkYellow, Color::Yellow)]
//     #[case(ColorSpec::Blue, Color::LightBlue)]
//     #[case(ColorSpec::DarkBlue, Color::Blue)]
//     #[case(ColorSpec::Magenta, Color::LightMagenta)]
//     #[case(ColorSpec::DarkMagenta, Color::Magenta)]
//     #[case(ColorSpec::Cyan, Color::LightCyan)]
//     #[case(ColorSpec::DarkCyan, Color::Cyan)]
//     #[case(ColorSpec::White, Color::White)]
//     #[case(ColorSpec::Grey, Color::Gray)]
//     #[case(ColorSpec::Rgb { r: 0, g: 0, b: 0 }, Color::Rgb(0, 0, 0) )]
//     #[case(ColorSpec::Rgb { r: 10, g: 20, b: 30 }, Color::Rgb(10, 20, 30) )]
//     #[case(ColorSpec::AnsiValue(32), Color::Indexed(32))]
//     #[case(ColorSpec::AnsiValue(37), Color::Indexed(37))]
//     fn from_termina_color(#[case] termina_color: ColorSpec, #[case] color: Color) {
//         assert_eq!(Color::from_termina(termina_color), color);
//     }
//
//     mod modifier {
//         use super::*;
//
//         #[rstest]
//         #[case(terminaAttribute::Reset, Modifier::empty())]
//         #[case(terminaAttribute::Bold, Modifier::BOLD)]
//         #[case(terminaAttribute::NoBold, Modifier::empty())]
//         #[case(terminaAttribute::Italic, Modifier::ITALIC)]
//         #[case(terminaAttribute::NoItalic, Modifier::empty())]
//         #[case(terminaAttribute::Underlined, Modifier::UNDERLINED)]
//         #[case(terminaAttribute::NoUnderline, Modifier::empty())]
//         #[case(terminaAttribute::OverLined, Modifier::empty())]
//         #[case(terminaAttribute::NotOverLined, Modifier::empty())]
//         #[case(terminaAttribute::DoubleUnderlined, Modifier::UNDERLINED)]
//         #[case(terminaAttribute::Undercurled, Modifier::UNDERLINED)]
//         #[case(terminaAttribute::Underdotted, Modifier::UNDERLINED)]
//         #[case(terminaAttribute::Underdashed, Modifier::UNDERLINED)]
//         #[case(terminaAttribute::Dim, Modifier::DIM)]
//         #[case(terminaAttribute::NormalIntensity, Modifier::empty())]
//         #[case(terminaAttribute::CrossedOut, Modifier::CROSSED_OUT)]
//         #[case(terminaAttribute::NotCrossedOut, Modifier::empty())]
//         #[case(terminaAttribute::NoUnderline, Modifier::empty())]
//         #[case(terminaAttribute::SlowBlink, Modifier::SLOW_BLINK)]
//         #[case(terminaAttribute::RapidBlink, Modifier::RAPID_BLINK)]
//         #[case(terminaAttribute::Hidden, Modifier::HIDDEN)]
//         #[case(terminaAttribute::NoHidden, Modifier::empty())]
//         #[case(terminaAttribute::Reverse, Modifier::REVERSED)]
//         #[case(terminaAttribute::NoReverse, Modifier::empty())]
//         fn from_termina_attribute(
//             #[case] termina_attribute: terminaAttribute,
//             #[case] ratatui_modifier: Modifier,
//         ) {
//             assert_eq!(Modifier::from_termina(termina_attribute), ratatui_modifier);
//         }
//
//         #[rstest]
//         #[case(&[terminaAttribute::Bold], Modifier::BOLD)]
//         #[case(&[terminaAttribute::Bold, terminaAttribute::Italic], Modifier::BOLD |
// Modifier::ITALIC)]         #[case(&[terminaAttribute::Bold, terminaAttribute::NotCrossedOut],
// Modifier::BOLD)]         #[case(&[terminaAttribute::Dim, terminaAttribute::Underdotted],
// Modifier::DIM | Modifier::UNDERLINED)]         #[case(&[terminaAttribute::Dim,
// terminaAttribute::SlowBlink, terminaAttribute::Italic], Modifier::DIM | Modifier::SLOW_BLINK |
// Modifier::ITALIC)]         #[case(&[terminaAttribute::Hidden, terminaAttribute::NoUnderline,
// terminaAttribute::NotCrossedOut], Modifier::HIDDEN)]         #[case(&[terminaAttribute::Reverse],
// Modifier::REVERSED)]         #[case(&[terminaAttribute::Reset], Modifier::empty())]
//         #[case(&[terminaAttribute::RapidBlink, terminaAttribute::CrossedOut],
// Modifier::RAPID_BLINK | Modifier::CROSSED_OUT)]         fn from_termina_attributes(
//             #[case] termina_attributes: &[terminaAttribute],
//             #[case] ratatui_modifier: Modifier,
//         ) {
//             assert_eq!(
//                 Modifier::from_termina(terminaAttributes::from(termina_attributes)),
//                 ratatui_modifier
//             );
//         }
//     }
//
//     #[rstest]
//     #[case(ContentStyle::default(), Style::default())]
//     #[case(
//         ContentStyle {
//             foreground_color: Some(ColorSpec::DarkYellow),
//             ..Default::default()
//         },
//         Style::default().fg(Color::Yellow)
//     )]
//     #[case(
//         ContentStyle {
//             background_color: Some(ColorSpec::DarkYellow),
//             ..Default::default()
//         },
//         Style::default().bg(Color::Yellow)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(terminaAttribute::Bold),
//             ..Default::default()
//         },
//         Style::default().add_modifier(Modifier::BOLD)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(terminaAttribute::NoBold),
//             ..Default::default()
//         },
//         Style::default().remove_modifier(Modifier::BOLD)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(terminaAttribute::Italic),
//             ..Default::default()
//         },
//         Style::default().add_modifier(Modifier::ITALIC)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(terminaAttribute::NoItalic),
//             ..Default::default()
//         },
//         Style::default().remove_modifier(Modifier::ITALIC)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(
//                 [terminaAttribute::Bold, terminaAttribute::Italic].as_ref()
//             ),
//             ..Default::default()
//         },
//         Style::default()
//             .add_modifier(Modifier::BOLD)
//             .add_modifier(Modifier::ITALIC)
//     )]
//     #[case(
//         ContentStyle {
//             attributes: terminaAttributes::from(
//                 [terminaAttribute::NoBold, terminaAttribute::NoItalic].as_ref()
//             ),
//             ..Default::default()
//         },
//         Style::default()
//             .remove_modifier(Modifier::BOLD)
//             .remove_modifier(Modifier::ITALIC)
//     )]
//     fn from_termina_content_style(#[case] content_style: ContentStyle, #[case] style: Style) {
//         assert_eq!(Style::from_termina(content_style), style);
//     }
//
//     #[test]
//     #[cfg(feature = "underline-color")]
//     fn from_termina_content_style_underline() {
//         let content_style = ContentStyle {
//             underline_color: Some(ColorSpec::DarkRed),
//             ..Default::default()
//         };
//         assert_eq!(
//             Style::from_termina(content_style),
//             Style::default().underline_color(Color::Red)
//         );
//     }
// }
