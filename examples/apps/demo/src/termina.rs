use std::error::Error;
use std::io::{Write, stderr, stdout};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::{Backend, TerminaBackend};
use termina::escape::csi;
use termina::event::KeyCode;
use termina::{Event, PlatformTerminal, Terminal as _};

use crate::app::App;
use crate::ui;

macro_rules! decset {
    ($mode:ident) => {
        csi::Csi::Mode(csi::Mode::SetDecPrivateMode(csi::DecPrivateMode::Code(
            csi::DecPrivateModeCode::$mode,
        )))
    };
}
macro_rules! decreset {
    ($mode:ident) => {
        csi::Csi::Mode(csi::Mode::ResetDecPrivateMode(csi::DecPrivateMode::Code(
            csi::DecPrivateModeCode::$mode,
        )))
    };
}

pub fn run(tick_rate: Duration, enhanced_graphics: bool) -> Result<(), Box<dyn Error>> {
    // setup terminal
    let mut platform_terminal = PlatformTerminal::new()?;
    platform_terminal.enter_raw_mode()?;
    write!(
        platform_terminal,
        "{}",
        decset!(ClearAndEnableAlternateScreen),
    )?;
    platform_terminal.flush()?;
    // create app and run it
    let app = App::new("Termina Demo", enhanced_graphics);

    let backend = TerminaBackend::new(platform_terminal, stdout());
    let mut terminal = Terminal::new(backend)?;
    let app_result = run_app(&mut terminal, app, tick_rate);

    // restore terminal
    write!(
        terminal.backend_mut().terminal_mut(),
        "{}",
        decreset!(ClearAndEnableAlternateScreen),
        // decreset!(MouseTracking),
        // decreset!(ButtonEventMouse),
        // decreset!(AnyEventMouse),
        // decreset!(RXVTMouse),
        // decreset!(SGRMouse),
    )?;
    if let Err(err) = app_result {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<W>(
    terminal: &mut Terminal<TerminaBackend<W>>,
    mut app: App,
    tick_rate: Duration,
) -> Result<(), Box<dyn Error>>
where
    W: Write,
{
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());

        if !terminal
            .backend()
            .terminal()
            .poll(|e| !e.is_escape(), Some(timeout))?
        {
            app.on_tick();
            last_tick = Instant::now();
            continue;
        }

        let ev = terminal.backend().terminal().read(|e| !e.is_escape())?;

        if let Event::Key(key) = ev {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => app.on_left(),
                KeyCode::Char('j') | KeyCode::Down => app.on_down(),
                KeyCode::Char('k') | KeyCode::Up => app.on_up(),
                KeyCode::Char('l') | KeyCode::Right => app.on_right(),
                KeyCode::Char(c) => app.on_key(c),
                _ => {}
            }
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
