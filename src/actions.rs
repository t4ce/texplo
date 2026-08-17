use std::io::{self, Stdout, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::graph_view::{GraphView, Viewport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    Reload,
    Parent,
    TreeLayout,
    Spacing,
    Sha256,
    Zip,
    Rename,
    NewFolder,
    Delete,
    Exit,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuEntry {
    pub hotkey: char,
    pub glyph: &'static str,
    pub label: &'static str,
    pub command: MenuCommand,
}

pub const MENU_ENTRIES: [MenuEntry; 10] = [
    MenuEntry { hotkey: '0', glyph: "🯰", label: "reload", command: MenuCommand::Reload },
    MenuEntry { hotkey: '1', glyph: "🯱", label: "parent", command: MenuCommand::Parent },
    MenuEntry { hotkey: '2', glyph: "🯲", label: "tree layout", command: MenuCommand::TreeLayout },
    MenuEntry { hotkey: '3', glyph: "🯳", label: "spacing", command: MenuCommand::Spacing },
    MenuEntry { hotkey: '4', glyph: "🯴", label: "sha256", command: MenuCommand::Sha256 },
    MenuEntry { hotkey: '5', glyph: "🯵", label: "zip", command: MenuCommand::Zip },
    MenuEntry { hotkey: '6', glyph: "🯶", label: "rename", command: MenuCommand::Rename },
    MenuEntry { hotkey: '7', glyph: "🯷", label: "new folder", command: MenuCommand::NewFolder },
    MenuEntry { hotkey: '8', glyph: "🯸", label: "delete", command: MenuCommand::Delete },
    MenuEntry { hotkey: '9', glyph: "🯹", label: "exit", command: MenuCommand::Exit },
];

#[derive(Clone, Copy, Debug)]
pub struct MenuState {
    pub cursor: usize,
}

impl Default for MenuState {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl MenuState {
    pub fn set_cursor(&mut self, index: usize) {
        if index < MENU_ENTRIES.len() {
            self.cursor = index;
        }
    }

    pub fn index_for_hotkey(key: char) -> Option<usize> {
        MENU_ENTRIES.iter().position(|entry| entry.hotkey == key)
    }

    pub fn row_for_index(index: usize) -> u16 {
        match index {
            0 => 2,
            1 => 3,
            2 => 6,
            3 => 7,
            4 => 10,
            5 => 11,
            6 => 12,
            7 => 13,
            8 => 14,
            9 => 15,
            _ => u16::MAX,
        }
    }

    pub fn index_for_row(row: u16) -> Option<usize> {
        (0..MENU_ENTRIES.len()).find(|&index| Self::row_for_index(index) == row)
    }
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    Move { source: usize, target: usize },
    Trash { source: usize },
}

#[derive(Clone, Debug)]
pub struct Confirmation {
    pub pending: PendingAction,
    pub title: String,
    pub lines: Vec<String>,
}

impl Confirmation {
    pub fn move_node(graph: &GraphView, source: usize, target: usize) -> Self {
        let source_name = graph.label(source);
        let target_name = graph.label(target);
        Self {
            pending: PendingAction::Move { source, target },
            title: "Confirm move".to_string(),
            lines: vec![
                format!("Move {source_name}"),
                format!("into {target_name} ?"),
            ],
        }
    }

    pub fn trash_node(graph: &GraphView, source: usize) -> Self {
        let source_name = graph.label(source);
        Self {
            pending: PendingAction::Trash { source },
            title: "Confirm trash".to_string(),
            lines: vec![
                format!("Move {source_name} to recycle area?"),
                "Stored in .explorer-trash/".to_string(),
            ],
        }
    }
}

pub enum Dispatch {
    Exit,
    Confirm(Confirmation),
    Status(String),
}

pub struct ActionOutcome {
    pub status: String,
    pub trashed_label: Option<String>,
}

pub fn dispatch_menu(
    command: MenuCommand,
    graph: &mut GraphView,
    selected: Option<usize>,
) -> io::Result<Dispatch> {
    match command {
        MenuCommand::Reload => {
            graph.reload()?;
            Ok(Dispatch::Status(format!("MOUNT · {}", graph.root_label())))
        }
        MenuCommand::Parent => {
            if graph.mount_parent()? {
                Ok(Dispatch::Status(format!("MOUNT · {}", graph.root_label())))
            } else {
                Ok(Dispatch::Status("MOUNT · already at filesystem root".to_string()))
            }
        }
        MenuCommand::TreeLayout => {
            graph.center();
            Ok(Dispatch::Status("VIEW · tree layout · centered".to_string()))
        }
        MenuCommand::Spacing => {
            let gap = graph.cycle_spacing();
            Ok(Dispatch::Status(format!("VIEW · spacing {gap} columns")))
        }
        MenuCommand::Sha256 => Ok(Dispatch::Status(
            "ACTION · sha256 callback reserved (std + crossterm only)".to_string(),
        )),
        MenuCommand::Zip => Ok(Dispatch::Status(
            "ACTION · zip callback reserved (std + crossterm only)".to_string(),
        )),
        MenuCommand::Rename => Ok(Dispatch::Status(
            "ACTION · rename callback reserved for text-input modal".to_string(),
        )),
        MenuCommand::NewFolder => Ok(Dispatch::Status(
            "ACTION · new-folder callback reserved for text-input modal".to_string(),
        )),
        MenuCommand::Delete => match selected {
            Some(0) | None => Ok(Dispatch::Status("DELETE · select one file/folder first".to_string())),
            Some(source) => Ok(Dispatch::Confirm(Confirmation::trash_node(graph, source))),
        },
        MenuCommand::Exit => Ok(Dispatch::Exit),
    }
}

pub fn execute_confirmation(
    confirmation: Confirmation,
    graph: &mut GraphView,
) -> io::Result<ActionOutcome> {
    match confirmation.pending {
        PendingAction::Move { source, target } => {
            let status = graph.move_node(source, target)?;
            Ok(ActionOutcome { status, trashed_label: None })
        }
        PendingAction::Trash { source } => {
            let label = graph.label(source);
            let status = graph.trash_node(source)?;
            Ok(ActionOutcome { status, trashed_label: Some(label) })
        }
    }
}


#[derive(Clone, Copy)]
struct ModalGeometry {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    yes_x0: u16,
    yes_x1: u16,
    no_x0: u16,
    no_x1: u16,
    button_y: u16,
}

pub fn confirmation_click(viewport: Viewport, x: u16, y: u16) -> Option<bool> {
    let g = modal_geometry(viewport);
    if y != g.button_y {
        return None;
    }
    if x >= g.yes_x0 && x <= g.yes_x1 {
        Some(true)
    } else if x >= g.no_x0 && x <= g.no_x1 {
        Some(false)
    } else {
        None
    }
}

pub fn draw_menu(
    out: &mut Stdout,
    menu_x: u16,
    bottom: u16,
    menu: MenuState,
    root_label: &str,
    menu_width: u16,
) -> io::Result<()> {
    let right = menu_x + menu_width - 1;

    print_at(out, menu_x, 0, "┯")?;
    for cx in menu_x + 1..right {
        print_at(out, cx, 0, "─")?;
    }
    print_at(out, right, 0, "╮")?;
    print_at(out, menu_x + 3, 0, "╼ Menu ╾")?;

    for y in 1..bottom {
        print_at(out, menu_x, y, "│")?;
        print_at(out, right, y, "│")?;
    }
    print_at(out, menu_x, bottom, "╰")?;
    for cx in menu_x + 1..right {
        print_at(out, cx, bottom, "─")?;
    }
    print_at(out, right, bottom, "╯")?;

    menu_text(out, menu_x + 2, 1, "mount", false)?;
    menu_text(out, menu_x + 2, 5, "view", false)?;
    menu_text(out, menu_x + 2, 9, "actions", false)?;

    for (index, entry) in MENU_ENTRIES.iter().enumerate() {
        let y = MenuState::row_for_index(index);
        if y >= bottom {
            continue;
        }
        let is_cursor = menu.cursor == index;
        let cursor = if is_cursor { "☩" } else { " " };
        let line = format!("{} {} {:<14}", entry.glyph, cursor, entry.label);
        menu_text(out, menu_x + 2, y, &line, is_cursor)?;
    }

    if bottom > 17 {
        let root = clip_text(root_label, menu_width.saturating_sub(4) as usize);
        menu_text(out, menu_x + 2, bottom - 2, &root, false)?;
        menu_text(out, menu_x + 2, bottom - 1, "esc exit · home center", false)?;
    }

    Ok(())
}

pub fn draw_confirmation(
    out: &mut Stdout,
    confirm: &Confirmation,
    viewport: Viewport,
) -> io::Result<()> {
    let g = modal_geometry(viewport);
    let inner = g.width.saturating_sub(2) as usize;

    queue!(
        out,
        SetBackgroundColor(Color::Grey),
        SetForegroundColor(Color::Black)
    )?;
    for row in 0..g.height {
        print_at(out, g.x, g.y + row, &" ".repeat(g.width as usize))?;
    }

    print_at(out, g.x, g.y, "╒")?;
    print_at(out, g.x + 1, g.y, &"═".repeat(inner))?;
    print_at(out, g.x + g.width - 1, g.y, "╕")?;

    let title = clip_text(&confirm.title, inner.saturating_sub(4));
    let title_x = g.x + 1 + (inner.saturating_sub(title.chars().count()) / 2) as u16;
    print_at(out, title_x, g.y, &title)?;

    for row in 1..g.height - 1 {
        print_at(out, g.x, g.y + row, "│")?;
        print_at(out, g.x + g.width - 1, g.y + row, "│")?;
    }

    for (i, line) in confirm.lines.iter().take(3).enumerate() {
        let clipped = clip_text(line, inner.saturating_sub(2));
        print_at(out, g.x + 2, g.y + 2 + i as u16, &clipped)?;
    }

    queue!(
        out,
        SetBackgroundColor(Color::White),
        SetForegroundColor(Color::Black)
    )?;
    let yes_width = (g.yes_x1 - g.yes_x0 + 1) as usize;
    let no_width = (g.no_x1 - g.no_x0 + 1) as usize;
    print_at(
        out,
        g.yes_x0,
        g.button_y,
        &format!("{:^width$}", "⫸ yes (Y)", width = yes_width),
    )?;
    print_at(
        out,
        g.no_x0,
        g.button_y,
        &format!("{:^width$}", "⫸ no (N)", width = no_width),
    )?;

    queue!(
        out,
        SetBackgroundColor(Color::Grey),
        SetForegroundColor(Color::Black)
    )?;
    print_at(out, g.x, g.y + g.height - 1, "╘")?;
    print_at(out, g.x + 1, g.y + g.height - 1, &"═".repeat(inner))?;
    print_at(out, g.x + g.width - 1, g.y + g.height - 1, "╛")?;
    queue!(out, ResetColor)?;
    Ok(())
}

fn modal_geometry(viewport: Viewport) -> ModalGeometry {
    let width = viewport.width.saturating_sub(4).min(44).max(24);
    let height = 8;
    let x = viewport.x + viewport.width.saturating_sub(width) / 2;
    let y = viewport.y + viewport.height.saturating_sub(height) / 2;
    let inner = width.saturating_sub(2);
    let half = inner / 2;
    ModalGeometry {
        x,
        y,
        width,
        height,
        yes_x0: x + 1,
        yes_x1: x + half,
        no_x0: x + half + 1,
        no_x1: x + width - 2,
        button_y: y + height - 2,
    }
}

fn menu_text(out: &mut Stdout, x: u16, y: u16, text: &str, active: bool) -> io::Result<()> {
    if active {
        queue!(
            out,
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::White)
        )?;
    } else {
        queue!(out, ResetColor)?;
    }
    print_at(out, x, y, text)?;
    queue!(out, ResetColor)?;
    Ok(())
}

fn clip_text(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    if max <= 3 {
        return text.chars().take(max).collect();
    }
    let mut clipped: String = text.chars().take(max - 3).collect();
    clipped.push_str("...");
    clipped
}

fn print_at(out: &mut Stdout, x: u16, y: u16, text: &str) -> io::Result<()> {
    queue!(out, MoveTo(x, y), Print(text))
}
