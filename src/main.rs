use std::io::{self, stdout, Stdout, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::Print,
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

const DROP_X: u16 = 0;
const FRAME_X: u16 = 2;
const CANVAS_X: u16 = FRAME_X + 1;
const MENU_WIDTH: u16 = 13;
const LOG_ROWS: u16 = 6;
const MIN_WIDTH: u16 = 32;
const MIN_HEIGHT: u16 = 15;

const LOGS: [&str; LOG_ROWS as usize] = [
    "⇝ Async delete started.",
    "⇝ Async delete completed.",
    "⇝ Async move started/completed/failed.",
    "⇝ Reason: \"Out of Storage\".",
    "⇝ File/Folder renamed.",
    "...",
];

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;

        if let Err(err) = execute!(stdout(), EnterAlternateScreen, DisableLineWrap, Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), Show, EnableLineWrap, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn main() -> io::Result<()> {
    let _terminal = TerminalGuard::enter()?;
    let mut out = stdout();

    draw(&mut out)?;

    loop {
        match event::read()? {
            Event::Resize(_, _) => draw(&mut out)?,
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc =>
            {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn draw(out: &mut Stdout) -> io::Result<()> {
    let (width, height) = terminal::size()?;
    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;

    if width < MIN_WIDTH || height < MIN_HEIGHT {
        print_at(out, 0, 0, "terminal too small")?;
        print_at(
            out,
            0,
            1,
            &format!("need at least {MIN_WIDTH}x{MIN_HEIGHT}; got {width}x{height}"),
        )?;
        out.flush()?;
        return Ok(());
    }

    let menu_x = width - MENU_WIDTH;
    let separator_y = height - LOG_ROWS - 1;
    let canvas_bottom_y = separator_y - 1;

    draw_top_rule(out, menu_x)?;
    draw_canvas(out, menu_x, canvas_bottom_y)?;
    draw_dropzone(out, canvas_bottom_y, separator_y)?;
    draw_menu(out, menu_x, separator_y)?;
    draw_bottom_rule(out, menu_x, separator_y)?;
    draw_logs(out, separator_y + 1, width)?;

    out.flush()
}

fn draw_top_rule(out: &mut Stdout, menu_x: u16) -> io::Result<()> {
    draw_pattern(out, 0, 0, menu_x, "🞃", "🞁")?;

    let label_start = centered_x(FRAME_X, menu_x.saturating_sub(4), " Text ");
    print_at(out, label_start, 0, " Text ")?;

    // Draw this last so a terminal that renders the map glyph as two cells
    // cannot push the menu header out of alignment.
    if menu_x >= 3 {
        print_at(out, menu_x - 3, 0, "🗺")?;
    }

    Ok(())
}

fn draw_canvas(out: &mut Stdout, menu_x: u16, bottom_y: u16) -> io::Result<()> {
    let zero_count = menu_x.saturating_sub(CANVAS_X) as usize;
    let zeros = "0".repeat(zero_count);

    for y in 1..=bottom_y {
        let edge = if y == 1 {
            "⎛"
        } else if y == bottom_y {
            "⎝"
        } else {
            "⎜"
        };

        print_at(out, FRAME_X, y, edge)?;
        print_at(out, CANVAS_X, y, &zeros)?;
    }

    Ok(())
}

fn draw_dropzone(out: &mut Stdout, canvas_bottom_y: u16, separator_y: u16) -> io::Result<()> {
    let fixed = [
        (1, "🖿"),
        (3, "🖿"),
        (4, "🖹"),
        (5, "🖿"),
        (6, "🖿"),
    ];

    for (y, icon) in fixed {
        if y <= canvas_bottom_y {
            print_at(out, DROP_X, y, icon)?;
        }
    }

    print_at(out, DROP_X, separator_y, "🗑")
}

fn draw_menu(out: &mut Stdout, menu_x: u16, bottom_y: u16) -> io::Result<()> {
    print_at(out, menu_x, 0, "┯─╼ Menu ╾──╮")?;

    for y in 1..bottom_y {
        print_at(out, menu_x, y, "│")?;
        print_at(out, menu_x + MENU_WIDTH - 1, y, "│")?;
    }

    print_at(out, menu_x, 1, "│  Mount    │")?;
    print_at(out, menu_x, bottom_y, "╰───────────╯")
}

fn draw_bottom_rule(out: &mut Stdout, menu_x: u16, y: u16) -> io::Result<()> {
    draw_pattern(out, 2, y, menu_x, "🞁", "🞃")?;

    let label_start = centered_x(FRAME_X, menu_x, " Text ");
    print_at(out, label_start, y, " Text ")
}

fn draw_logs(out: &mut Stdout, start_y: u16, width: u16) -> io::Result<()> {
    for (row, line) in LOGS.iter().enumerate() {
        let clipped: String = line.chars().take(width as usize).collect();
        print_at(out, 0, start_y + row as u16, &clipped)?;
    }

    Ok(())
}

fn draw_pattern(
    out: &mut Stdout,
    start_x: u16,
    y: u16,
    end_x: u16,
    first: &'static str,
    second: &'static str,
) -> io::Result<()> {
    for x in start_x..end_x {
        let glyph = if (x - start_x) % 2 == 0 {
            first
        } else {
            second
        };
        print_at(out, x, y, glyph)?;
    }

    Ok(())
}

fn centered_x(start_x: u16, end_x: u16, text: &str) -> u16 {
    let width = end_x.saturating_sub(start_x);
    let text_width = text.chars().count() as u16;
    start_x + width.saturating_sub(text_width) / 2
}

fn print_at(out: &mut Stdout, x: u16, y: u16, text: &str) -> io::Result<()> {
    queue!(out, MoveTo(x, y), Print(text))
}