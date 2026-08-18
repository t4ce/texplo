use crate::{
    actions::{MenuCommand, MenuContext, MenuLink, MenuSection, MenuState, MENU_ENTRIES},
    graph_view::{LineStyle, SelectionStats},
    layout::LayoutMode,
    screen::{text_cell_width, Frame, Style},
};

const DIGIT_GLYPHS: [&str; 10] = ["🯰", "🯱", "🯲", "🯳", "🯴", "🯵", "🯶", "🯷", "🯸", "🯹"];
const TOGGLE_GLYPHS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

pub fn draw_menu(
    frame: &mut Frame,
    menu_x: u16,
    bottom: u16,
    menu: MenuState,
    menu_width: u16,
    context: MenuContext,
    links: &[MenuLink],
    stats: Option<&SelectionStats>,
    depth_limit: usize,
    spacing_level: usize,
    line_style: LineStyle,
    layout_mode: LayoutMode,
) {
    let _ = menu_width;
    let blank = "│               │";
    for y in 0..=bottom {
        menu_line(frame, menu_x, y, blank);
    }

    // One literal gap between the stylized E and N. The extra horizontal cap
    // keeps the close glyph at the physical top-right cell of the 17-col menu.
    menu_line(frame, menu_x, 0, "├─╼ ᗰ ☰ＮＵ╾─┤🯀");
    menu_line(frame, menu_x, 1, "│Use ⌨   Tab  ╰─╮");
    menu_line(frame, menu_x, 2, "├─╼ Mount      ╾┤");
    menu_line(frame, menu_x, 4, blank);
    menu_line(frame, menu_x, 5, "├─╼ View       ╾┤");
    menu_line(frame, menu_x, 11, "│               ╿");
    menu_line(frame, menu_x, 12, "├─╼ Act        ╾┤");

    if let Some(index) = MenuState::index_for_command(MenuCommand::Reload) {
        if let Some(y) = MenuState::row_for_index(index, context) {
            let cursor = if menu.section == MenuSection::Mount && menu.cursor == index { "☩" } else { " " };
            menu_line(frame, menu_x, y, &entry_line(0, cursor, "first"));
        }
    }

    let view_rows = [
        (MenuCommand::Center, "center", None, None),
        (MenuCommand::Depth, "depth", Some(TOGGLE_GLYPHS[depth_limit.min(7)]), Some(('╭', '╮'))),
        (MenuCommand::Spacing, "space", Some(spacing_toggle_glyph(spacing_level.min(4))), Some(('┝', '┥'))),
        (MenuCommand::LineStyle, "line", Some(toggle_glyph(line_style_index(line_style), 4)), Some(('┝', '┥'))),
        (
            MenuCommand::ToggleLayout,
            "layout",
            Some(toggle_glyph(if layout_mode == LayoutMode::Tree { 0 } else { 1 }, 2)),
            Some(('╰', '╯')),
        ),
    ];
    for (local, (command, label, toggle, cap)) in view_rows.iter().enumerate() {
        let Some(index) = MenuState::index_for_command(*command) else { continue; };
        let Some(y) = MenuState::row_for_index(index, context) else { continue; };
        let cursor = if menu.section == MenuSection::View && menu.cursor == index { "☩" } else { " " };
        menu_line(frame, menu_x, y, &view_line(local, cursor, label, *toggle, *cap));
    }

    for index in 0..MENU_ENTRIES.len() {
        let entry = MENU_ENTRIES[index];
        if entry.section != MenuSection::Action || !MenuState::is_visible(index, context) {
            continue;
        }
        let Some(y) = MenuState::row_for_index(index, context) else { continue; };
        if y >= bottom { continue; }
        let local = MenuState::local_index(index, context).unwrap_or(0).min(9);
        let cursor = if menu.section == MenuSection::Action && menu.cursor == index { "☩" } else { " " };
        menu_line(frame, menu_x, y, &entry_line(local, cursor, entry.label));
    }

    let action_blank = 13 + MenuState::action_count(context) as u16;
    if action_blank < bottom {
        menu_line(frame, menu_x, action_blank, blank);
    }

    if let (Some(header), Some(stats)) = (MenuState::stats_header_row(context), stats) {
        if header + 5 < bottom {
            menu_line(frame, menu_x, header, "├─╼ Stats      ╾┤");
            let rows = [
                ("key", stats.key.as_str()),
                ("size", stats.size.as_str()),
                ("kind", stats.kind.as_str()),
                ("mod", stats.modified.as_str()),
                ("mode", stats.access.as_str()),
            ];
            for (offset, (key, value)) in rows.iter().enumerate() {
                menu_line(frame, menu_x, header + 1 + offset as u16, &stats_line(key, value));
            }
            if header + 6 < bottom {
                menu_line(frame, menu_x, header + 6, blank);
            }
        }
    }

    if let Some(clip_header) = MenuState::clip_header_row(bottom, context, links.len()) {
        menu_line(frame, menu_x, clip_header, "├─Clip 🖈       ╾┤");
        draw_links(frame, menu_x, bottom, context, menu, links);
    }

    menu_line(frame, menu_x, bottom, "╰───────────────╯");
}

pub fn close_button_hit(menu_x: u16, menu_width: u16, x: u16, y: u16) -> bool {
    y == 0 && x == menu_x.saturating_add(menu_width.saturating_sub(1))
}


fn line_style_index(style: LineStyle) -> usize {
    match style {
        LineStyle::Default => 0,
        LineStyle::Straight => 1,
        LineStyle::Elbow => 2,
        LineStyle::SoftArc => 3,
    }
}

fn spacing_toggle_glyph(index: usize) -> &'static str {
    const SLOTS: [usize; 5] = [0, 1, 3, 5, 7];
    TOGGLE_GLYPHS[SLOTS[index.min(4)]]
}

fn toggle_glyph(index: usize, states: usize) -> &'static str {
    if states <= 1 {
        return TOGGLE_GLYPHS[0];
    }
    let index = index.min(states - 1);
    let slot = (index * 7 + (states - 1) / 2) / (states - 1);
    TOGGLE_GLYPHS[slot.min(7)]
}

fn entry_line(local: usize, cursor: &str, label: &str) -> String {
    let digit = DIGIT_GLYPHS[local.min(9)];
    let label = pad_cells(&clip_cells(label, 11), 11);
    format!("│{digit} {cursor} {label}│")
}

fn view_line(
    local: usize,
    cursor: &str,
    label: &str,
    toggle: Option<&str>,
    cap: Option<(char, char)>,
) -> String {
    let digit = DIGIT_GLYPHS[local.min(9)];
    let prefix = format!("│{digit} {cursor} ");
    let rail = match (toggle, cap) {
        (Some(toggle), Some((left, right))) => format!("{left}{toggle}{right}"),
        _ => "╽".to_string(),
    };
    let budget = 17usize
        .saturating_sub(text_cell_width(&prefix))
        .saturating_sub(text_cell_width(&rail));
    let label = clip_cells(label, budget);
    let gap = " ".repeat(budget.saturating_sub(text_cell_width(&label)));
    format!("{prefix}{label}{gap}{rail}")
}

fn stats_line(key: &str, value: &str) -> String {
    let key = pad_cells(&clip_cells(key, 5), 5);
    let value = pad_cells(&clip_cells(value, 10), 10);
    format!("│{key}{value}│")
}

fn draw_links(
    frame: &mut Frame,
    menu_x: u16,
    bottom: u16,
    context: MenuContext,
    menu: MenuState,
    links: &[MenuLink],
) {
    let visible = MenuState::clip_visible_count(bottom, context, links.len());
    if visible == 0 {
        return;
    }

    if links.is_empty() {
        let y = bottom - 1;
        let cursor = if menu.section == MenuSection::Clip { "☩" } else { " " };
        menu_line(frame, menu_x, y, &entry_line(0, cursor, "Empty"));
        return;
    }

    // The Clip section is bottom-anchored. First stays nearest the bottom;
    // subsequent links stack upward, and tight terminals simply show fewer.
    for index in 0..visible {
        let y = bottom - 1 - index as u16;
        let cursor = if menu.section == MenuSection::Clip && menu.clip_cursor == index { "☩" } else { " " };
        menu_line(frame, menu_x, y, &entry_line(index, cursor, &links[index].label()));
    }
}

pub fn link_index_for_row(
    row: u16,
    bottom: u16,
    context: MenuContext,
    link_count: usize,
) -> Option<usize> {
    if link_count == 0 || row >= bottom {
        return None;
    }
    let header = MenuState::clip_header_row(bottom, context, link_count)?;
    if row <= header {
        return None;
    }
    let index = bottom.saturating_sub(1).saturating_sub(row) as usize;
    let visible = MenuState::clip_visible_count(bottom, context, link_count);
    (index < visible).then_some(index)
}

fn menu_line(frame: &mut Frame, x: u16, y: u16, text: &str) {
    let fitted = fit_cells(text, 17);
    print_at(frame, x, y, &fitted, Style::default());
}

fn clip_cells(text: &str, max: usize) -> String {
    if text_cell_width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let suffix = if max >= 2 { "…" } else { "" };
    let budget = max.saturating_sub(text_cell_width(suffix));
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = crate::screen::terminal_cell_width(ch) as usize;
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str(suffix);
    out
}

fn pad_cells(text: &str, width: usize) -> String {
    let used = text_cell_width(text);
    format!("{}{}", text, " ".repeat(width.saturating_sub(used)))
}

fn fit_cells(text: &str, width: usize) -> String {
    let clipped = clip_cells(text, width);
    pad_cells(&clipped, width)
}


fn print_at(frame: &mut Frame, x: u16, y: u16, text: &str, style: Style) {
    frame.put_str(x, y, text, style);
}
