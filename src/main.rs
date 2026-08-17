mod actions;
mod graph_view;

use std::{
    collections::VecDeque,
    io::{self, stdout, Stdout, Write},
    time::{Duration, Instant},
};

use actions::{
    confirmation_click, dispatch_menu, draw_confirmation, draw_menu, draw_menu_cursor_only,
    execute_confirmation, Confirmation, Dispatch, MenuState, MENU_ENTRIES,
};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use graph_view::{GraphView, Viewport};

const DROP_X: u16 = 0;
const FRAME_X: u16 = 2;
const CANVAS_X: u16 = FRAME_X + 1;
const MENU_WIDTH: u16 = 26;
const LOG_ROWS: u16 = 6;
const MIN_WIDTH: u16 = 58;
const MIN_HEIGHT: u16 = 24;
const TICK: Duration = Duration::from_millis(45);
const BUILD_ID: &str = "v0.6 · split paths";
const FALL_TICK: Duration = Duration::from_millis(90);

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;

        if let Err(err) = execute!(
            stdout(),
            EnterAlternateScreen,
            DisableLineWrap,
            EnableMouseCapture,
            Hide
        ) {
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            stdout(),
            ResetColor,
            Show,
            DisableMouseCapture,
            EnableLineWrap,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[derive(Clone, Copy)]
struct Press {
    node: usize,
    start_x: u16,
    start_y: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DragState {
    node: usize,
    x: u16,
    y: u16,
    target: Option<usize>,
    over_trash: bool,
}

#[derive(Clone, Copy)]
struct PanState {
    start_x: u16,
    start_y: u16,
    camera_x: i32,
    camera_y: i32,
}

struct FallingGhost {
    icon: &'static str,
    y: u16,
    next_step: Instant,
}

struct App {
    graph: GraphView,
    selected: Option<usize>,
    press: Option<Press>,
    drag: Option<DragState>,
    pan: Option<PanState>,
    menu: MenuState,
    modal: Option<Confirmation>,
    logs: VecDeque<String>,
    falling: Option<FallingGhost>,
    should_exit: bool,
    dirty: bool,
    menu_dirty: Option<usize>,
    falling_dirty: Option<u16>,
}

impl App {
    fn new() -> io::Result<Self> {
        let graph = GraphView::from_current_dir()?;
        let mut logs = VecDeque::new();
        logs.push_back(format!("⇝ {BUILD_ID}"));
        logs.push_back(format!("⇝ Mounted {}", graph.root_label()));
        logs.push_back("⇝ LMB select/drag · MMB pan · Home center".to_string());
        logs.push_back("⇝ 0..9 menu hotkeys · Y/N confirmation".to_string());
        Ok(Self {
            graph,
            selected: None,
            press: None,
            drag: None,
            pan: None,
            menu: MenuState::default(),
            modal: None,
            logs,
            falling: None,
            should_exit: false,
            dirty: true,
            menu_dirty: None,
            falling_dirty: None,
        })
    }

    fn log(&mut self, message: impl Into<String>) {
        self.logs.push_back(format!("⇝ {}", message.into()));
        while self.logs.len() > LOG_ROWS as usize {
            self.logs.pop_front();
        }
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.menu_dirty = None;
        self.falling_dirty = None;
    }

    fn mark_menu_dirty(&mut self, previous: usize) {
        if !self.dirty && self.menu_dirty.is_none() {
            self.menu_dirty = Some(previous);
        }
    }

    fn clear_transient(&mut self) {
        self.press = None;
        self.drag = None;
        self.pan = None;
    }
}

#[derive(Clone, Copy)]
struct Layout {
    width: u16,
    menu_x: u16,
    separator_y: u16,
    canvas_bottom_y: u16,
    viewport: Viewport,
}

impl Layout {
    fn current() -> io::Result<Option<Self>> {
        let (width, height) = terminal::size()?;
        if width < MIN_WIDTH || height < MIN_HEIGHT {
            return Ok(None);
        }

        let menu_x = width - MENU_WIDTH;
        let separator_y = height - LOG_ROWS - 1;
        let canvas_bottom_y = separator_y - 1;
        let viewport = Viewport {
            x: CANVAS_X,
            y: 1,
            width: menu_x.saturating_sub(CANVAS_X),
            height: canvas_bottom_y,
        };

        Ok(Some(Self {
            width,
            menu_x,
            separator_y,
            canvas_bottom_y,
            viewport,
        }))
    }
}

fn main() -> io::Result<()> {
    let _terminal = TerminalGuard::enter()?;
    let mut out = stdout();
    let mut app = App::new()?;

    loop {
        if app.dirty {
            draw(&mut out, &app)?;
            app.dirty = false;
            app.menu_dirty = None;
            app.falling_dirty = None;
        } else {
            if let Some(previous_y) = app.falling_dirty.take() {
                draw_falling_only(&mut out, &app, previous_y)?;
            }
            if let Some(previous) = app.menu_dirty.take() {
                draw_menu_only(&mut out, &app, previous)?;
            }
        }
        if app.should_exit {
            break;
        }

        if event::poll(TICK)? {
            let ev = event::read()?;
            handle_event(&mut app, ev)?;
        }
        update_animation(&mut app);
    }

    Ok(())
}

fn handle_event(app: &mut App, ev: Event) -> io::Result<()> {
    match ev {
        Event::Resize(_, _) => app.mark_dirty(),
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key.code)?,
        Event::Mouse(mouse) => handle_mouse(app, mouse)?,
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) -> io::Result<()> {
    if app.modal.is_some() {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => confirm_modal(app, true)?,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => confirm_modal(app, false)?,
            _ => {}
        }
        return Ok(());
    }

    match code {
        KeyCode::Esc => app.should_exit = true,
        KeyCode::Home => {
            if let Some(index) = MenuState::index_for_command(actions::MenuCommand::Center) {
                let previous = app.menu.cursor;
                if app.menu.set_cursor(index) {
                    app.mark_menu_dirty(previous);
                }
                invoke_menu(app, index)?;
            }
        }
        KeyCode::Left => { app.graph.pan(2, 0); app.mark_dirty(); }
        KeyCode::Right => { app.graph.pan(-2, 0); app.mark_dirty(); }
        KeyCode::Up => { app.graph.pan(0, 1); app.mark_dirty(); }
        KeyCode::Down => { app.graph.pan(0, -1); app.mark_dirty(); }
        KeyCode::Delete | KeyCode::Backspace => {
            if let Some(source) = app.selected.filter(|id| *id != 0) {
                let mut confirm = Confirmation::trash_node(&app.graph, source);
                if let Some(y) = selected_screen_y(app, source) {
                    confirm = confirm.with_origin_y(y);
                }
                app.modal = Some(confirm);
                app.mark_dirty();
            }
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            if let Some(index) = MenuState::index_for_hotkey(ch) {
                let previous = app.menu.cursor;
                if app.menu.set_cursor(index) {
                    app.mark_menu_dirty(previous);
                }
                invoke_menu(app, index)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) -> io::Result<()> {
    let Some(layout) = Layout::current()? else {
        return Ok(());
    };

    if app.modal.is_some() {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(yes) = confirmation_click(layout.viewport, mouse.column, mouse.row) {
                confirm_modal(app, yes)?;
            }
        }
        return Ok(());
    }

    if let Some(pan) = app.pan {
        match mouse.kind {
            MouseEventKind::Drag(_) => {
                let dx = mouse.column as i32 - pan.start_x as i32;
                let dy = mouse.row as i32 - pan.start_y as i32;
                let next = (pan.camera_x + dx, pan.camera_y + dy);
                if app.graph.camera() != next {
                    app.graph.set_camera(next.0, next.1);
                    app.mark_dirty();
                }
                return Ok(());
            }
            MouseEventKind::Up(_) => {
                // Release changes no visible state. Do not redraw here: some terminals
                // emit a small burst of release-related mouse events, and repainting
                // each one causes the stationary full-screen flash.
                app.pan = None;
                return Ok(());
            }
            _ => {}
        }
    }

    if mouse.column >= layout.menu_x
        && app.press.is_none()
        && app.drag.is_none()
        && app.pan.is_none()
    {
        if let Some(index) = MenuState::index_for_row(mouse.row) {
            let previous = app.menu.cursor;
            if app.menu.set_cursor(index) {
                app.mark_menu_dirty(previous);
            }
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                invoke_menu(app, index)?;
            }
        }
        return Ok(());
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Middle) if layout.viewport.contains(mouse.column, mouse.row) => {
            let (camera_x, camera_y) = app.graph.camera();
            app.pan = Some(PanState {
                start_x: mouse.column,
                start_y: mouse.row,
                camera_x,
                camera_y,
            });
            app.press = None;
            app.drag = None;
        }
        MouseEventKind::Down(MouseButton::Left) if layout.viewport.contains(mouse.column, mouse.row) => {
            if let Some(node) = app.graph.hit_test(layout.viewport, mouse.column, mouse.row) {
                if app.selected != Some(node) { app.mark_dirty(); }
                app.selected = Some(node);
                app.press = Some(Press {
                    node,
                    start_x: mouse.column,
                    start_y: mouse.row,
                });
            } else {
                if app.selected.is_some() { app.mark_dirty(); }
                app.selected = None;
                app.press = None;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(press) = app.press {
                if press.node == 0 {
                    return Ok(());
                }
                let moved = mouse.column.abs_diff(press.start_x) + mouse.row.abs_diff(press.start_y);
                if moved >= 1 {
                    let over_trash = mouse.column == DROP_X
                        && mouse.row >= 1
                        && mouse.row <= layout.canvas_bottom_y;
                    let target = if over_trash {
                        None
                    } else {
                        app.graph.folder_drop_target(
                            layout.viewport,
                            mouse.column,
                            mouse.row,
                            press.node,
                        )
                    };
                    let next_drag = DragState {
                        node: press.node,
                        x: mouse.column,
                        y: mouse.row,
                        target,
                        over_trash,
                    };
                    if app.drag != Some(next_drag) {
                        app.drag = Some(next_drag);
                        app.mark_dirty();
                    }
                }
            }
        }
        MouseEventKind::Up(_) => {
            if let Some(drag) = app.drag.take() {
                if drag.over_trash {
                    app.modal = Some(Confirmation::trash_node(&app.graph, drag.node).with_origin_y(drag.y));
                    app.mark_dirty();
                } else if let Some(target) = drag.target {
                    app.modal = Some(Confirmation::move_node(&app.graph, drag.node, target));
                    app.mark_dirty();
                } else {
                    app.log("MOVE · cancelled · target must be a valid folder");
                }
            }
            app.press = None;
        }
        _ => {}
    }

    Ok(())
}

fn invoke_menu(app: &mut App, index: usize) -> io::Result<()> {
    let entry = MENU_ENTRIES[index];
    match dispatch_menu(entry.command, &mut app.graph, app.selected)? {
        Dispatch::Exit => app.should_exit = true,
        Dispatch::Confirm(mut confirm) => {
            if confirm.trash_origin_y.is_none() {
                let trash_source = match &confirm.pending {
                    actions::PendingAction::Trash { source } => Some(*source),
                    _ => None,
                };
                if let Some(source) = trash_source {
                    if let Some(y) = selected_screen_y(app, source) {
                        confirm = confirm.with_origin_y(y);
                    }
                }
            }
            app.modal = Some(confirm);
            app.mark_dirty();
        }
        Dispatch::Status(status) => {
            if matches!(entry.command, actions::MenuCommand::Reload | actions::MenuCommand::Parent) {
                app.selected = None;
                app.clear_transient();
            }
            app.log(status);
        }
    }
    Ok(())
}

fn selected_screen_y(app: &App, source: usize) -> Option<u16> {
    let layout = Layout::current().ok().flatten()?;
    app.graph.screen_y(source, layout.viewport)
}

fn confirm_modal(app: &mut App, yes: bool) -> io::Result<()> {
    let Some(confirm) = app.modal.take() else {
        return Ok(());
    };

    if !yes {
        app.log("CONFIRM · no · cancelled");
        return Ok(());
    }

    // Capture the visual trash metadata before the filesystem operation reloads
    // the graph and the source node disappears. A drag-origin Y is preserved by
    // the confirmation itself, so the icon continues falling from where the
    // user actually released it rather than teleporting back to the top.
    let falling_seed = match &confirm.pending {
        actions::PendingAction::Trash { source } => app.graph.node(*source).map(|node| {
            (if node.is_dir { "🖿" } else { "🖹" }, confirm.trash_origin_y)
        }),
        _ => None,
    };

    match execute_confirmation(confirm, &mut app.graph) {
        Ok(outcome) => {
            app.selected = None;
            app.clear_transient();
            app.log(outcome.status);
            if outcome.trashed_label.is_some() {
                if let Some((icon, origin_y)) = falling_seed {
                    let y = origin_y.unwrap_or(1);
                    app.falling = Some(FallingGhost {
                        icon,
                        y,
                        next_step: Instant::now() + FALL_TICK,
                    });
                }
            }
        }
        Err(err) => app.log(format!("ACTION FAILED · {err}")),
    }
    Ok(())
}

fn update_animation(app: &mut App) {
    let now = Instant::now();
    let bottom = Layout::current()
        .ok()
        .flatten()
        .map(|layout| layout.separator_y)
        .unwrap_or(1);

    let Some(falling) = app.falling.as_mut() else {
        return;
    };
    if now < falling.next_step {
        return;
    }

    let previous_y = falling.y;
    let finished = falling.y >= bottom;
    if !finished {
        falling.y += 1;
        falling.next_step = now + FALL_TICK;
    } else {
        app.falling = None;
    }

    // Only column 0 changed. Repaint that tiny animation strip instead of
    // clearing/rebuilding the entire graph and menu for every falling step.
    app.falling_dirty = Some(previous_y);
}

fn draw_menu_only(out: &mut Stdout, app: &App, previous: usize) -> io::Result<()> {
    let Some(layout) = Layout::current()? else {
        return Ok(());
    };

    draw_menu_cursor_only(
        out,
        layout.menu_x,
        layout.separator_y,
        app.menu,
        MENU_WIDTH,
        previous,
    )?;
    queue!(out, ResetColor)?;
    out.flush()
}

fn draw_falling_only(out: &mut Stdout, app: &App, previous_y: u16) -> io::Result<()> {
    let Some(layout) = Layout::current()? else {
        return Ok(());
    };

    // Clear both cells potentially occupied by a wide icon. Column 1 is the
    // dedicated spacer, so this never erases graph content.
    queue!(out, ResetColor)?;
    print_at(out, DROP_X, previous_y.min(layout.separator_y), "  ")?;

    if previous_y >= layout.separator_y {
        queue!(out, SetForegroundColor(Color::DarkGrey))?;
        print_at(out, DROP_X, layout.separator_y, "🗑")?;
        queue!(out, ResetColor)?;
    }

    if let Some(falling) = &app.falling {
        draw_falling(out, falling, layout)?;
    } else {
        // The final animation tick removes the falling icon and reveals the bin.
        queue!(out, SetForegroundColor(Color::DarkGrey))?;
        print_at(out, DROP_X, layout.separator_y, "🗑")?;
        queue!(out, ResetColor)?;
    }

    out.flush()
}

fn draw(out: &mut Stdout, app: &App) -> io::Result<()> {
    let (width, height) = terminal::size()?;
    queue!(out, ResetColor, MoveTo(0, 0), Clear(ClearType::All))?;

    let Some(layout) = Layout::current()? else {
        print_at(out, 0, 0, "terminal too small")?;
        print_at(
            out,
            0,
            1,
            &format!("need at least {MIN_WIDTH}x{MIN_HEIGHT}; got {width}x{height}"),
        )?;
        out.flush()?;
        return Ok(());
    };

    draw_top_rule(out, app, layout.menu_x)?;
    draw_canvas_frame(out, layout.canvas_bottom_y)?;
    draw_dropzone(out, app, layout)?;

    let drag_id = app.drag.map(|d| d.node);
    let drop_target = app.drag.and_then(|d| d.target);
    app.graph.render(out, layout.viewport, app.selected, drag_id, drop_target)?;

    if let Some(drag) = app.drag {
        draw_drag_box(out, app, drag, layout)?;
    }
    if let Some(falling) = &app.falling {
        draw_falling(out, falling, layout)?;
    }

    draw_menu(
        out,
        layout.menu_x,
        layout.separator_y,
        app.menu,
        MENU_WIDTH,
    )?;
    draw_bottom_rule(out, app, layout.menu_x, layout.separator_y)?;
    draw_logs(out, app, layout.separator_y + 1, layout.width)?;

    if let Some(confirm) = &app.modal {
        draw_confirmation(out, confirm, layout.viewport)?;
    }

    queue!(out, ResetColor)?;
    out.flush()
}

fn draw_top_rule(out: &mut Stdout, app: &App, menu_x: u16) -> io::Result<()> {
    // The top title is always the directory currently loaded as the mount.
    // Selection is deliberately kept out of this frame and shown below.
    // One space on either side of the world-map glyph keeps it visually clean
    // without the previous extra right-side gap.
    let map_cluster = " 🗺 ";
    let map_x = menu_x.saturating_sub(4);
    let title = app.graph.root_label();

    draw_rule_title(out, 0, 0, map_x, &title, "🞃", "🞁")?;
    print_at(out, map_x, 0, map_cluster)
}

fn draw_canvas_frame(out: &mut Stdout, bottom_y: u16) -> io::Result<()> {
    for y in 1..=bottom_y {
        let edge = if y == 1 {
            "⎛"
        } else if y == bottom_y {
            "⎝"
        } else {
            "⎜"
        };
        print_at(out, FRAME_X, y, edge)?;
    }
    Ok(())
}

fn draw_dropzone(out: &mut Stdout, app: &App, layout: Layout) -> io::Result<()> {
    // Column 0 is a drop area, not a permanent border. Keep it visually empty
    // until a dragged item is actually over the recycle target.
    let active_drag_y = app
        .drag
        .filter(|d| d.over_trash)
        .map(|d| d.y.clamp(1, layout.canvas_bottom_y));

    if let Some(y) = active_drag_y {
        queue!(out, SetBackgroundColor(Color::DarkRed), SetForegroundColor(Color::White))?;
        print_at(out, DROP_X, y, "▼")?;
        print_at(out, DROP_X, layout.separator_y, "🗑")?;
        queue!(out, ResetColor)?;
    } else {
        queue!(out, SetForegroundColor(Color::DarkGrey))?;
        print_at(out, DROP_X, layout.separator_y, "🗑")?;
        queue!(out, ResetColor)?;
    }
    Ok(())
}

fn draw_drag_box(out: &mut Stdout, app: &App, drag: DragState, layout: Layout) -> io::Result<()> {
    let Some(node) = app.graph.node(drag.node) else {
        return Ok(());
    };
    let icon = if node.is_dir { "🖿" } else { "🖹" };
    let name = app.graph.label(drag.node);
    let inner = name.chars().count().clamp(4, 18);
    let visible = clip_text(&name, inner);
    let box_width = inner + 4;

    let mut x = drag.x.saturating_add(1);
    if x + box_width as u16 >= layout.menu_x {
        x = drag.x.saturating_sub(box_width as u16);
    }
    x = x.max(CANVAS_X);
    let y = drag.y.saturating_add(1).min(layout.canvas_bottom_y.saturating_sub(2));

    print_at(out, x, y, icon)?;
    print_at(out, x + 2, y, &format!("╾{}╮", "─".repeat(inner + 1)))?;
    print_at(out, x + 2, y + 1, &format!("╿ {:<width$}│", visible, width = inner))?;
    print_at(out, x + 2, y + 2, &format!("╰{}╯", "─".repeat(inner + 2)))
}

fn draw_falling(out: &mut Stdout, falling: &FallingGhost, layout: Layout) -> io::Result<()> {
    // The recycle animation is the same icon used by graph nodes. It starts at
    // the drag release Y (when available) and falls only the remaining distance
    // into the bin. Column 1 is intentionally free, so a wide glyph at column 0
    // does not collide with the graph frame.
    let y = falling.y.min(layout.separator_y);
    queue!(out, ResetColor, SetForegroundColor(Color::Grey))?;
    print_at(out, DROP_X, y, falling.icon)?;
    queue!(out, ResetColor)?;
    Ok(())
}

fn draw_bottom_rule(out: &mut Stdout, app: &App, menu_x: u16, y: u16) -> io::Result<()> {
    let selected_path = app
        .selected
        .and_then(|id| app.graph.node(id))
        .filter(|node| !node.is_placeholder)
        .map(|node| node.path.display().to_string())
        .filter(|path| !path.is_empty());

    match selected_path {
        Some(path) => draw_rule_title(out, FRAME_X, y, menu_x, &path, "🞁", "🞃"),
        None => draw_pattern(out, FRAME_X, y, menu_x, "🞁", "🞃"),
    }
}

fn draw_rule_title(
    out: &mut Stdout,
    start_x: u16,
    y: u16,
    end_x: u16,
    title: &str,
    first: &'static str,
    second: &'static str,
) -> io::Result<()> {
    let width = end_x.saturating_sub(start_x) as usize;
    draw_pattern(out, start_x, y, end_x, first, second)?;
    if width < 7 || title.is_empty() {
        return Ok(());
    }

    // The title may consume essentially the whole rule, but retain one
    // decorative pair on each side. Tail clipping preserves file extensions.
    const SIDE: &str = "🞃🞁";
    let side_width = SIDE.chars().count();
    let fixed = side_width * 2 + 2;
    let title_width = width.saturating_sub(fixed).max(1);
    let visible = clip_text_tail(title, title_width);
    let label = format!("{SIDE} {visible} {SIDE}");
    let label_width = label.chars().count();

    if label_width >= width {
        return print_at(out, start_x, y, &clip_text_tail(&label, width));
    }

    let x = start_x + ((width - label_width) / 2) as u16;
    print_at(out, x, y, &label)
}

fn draw_logs(out: &mut Stdout, app: &App, start_y: u16, width: u16) -> io::Result<()> {
    for row in 0..LOG_ROWS {
        let text = app
            .logs
            .get(row as usize)
            .map(String::as_str)
            .unwrap_or("");
        let clipped = clip_text(text, width as usize);
        print_at(out, 0, start_y + row, &clipped)?;
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
        let glyph = if (x - start_x) % 2 == 0 { first } else { second };
        print_at(out, x, y, glyph)?;
    }
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

fn clip_text_tail(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let tail: String = text.chars().rev().take(max - 1).collect::<Vec<_>>().into_iter().rev().collect();
    format!("…{tail}")
}

fn print_at(out: &mut Stdout, x: u16, y: u16, text: &str) -> io::Result<()> {
    queue!(out, MoveTo(x, y), Print(text))
}
