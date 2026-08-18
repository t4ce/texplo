mod actions;
mod graph_view;
mod layout;
mod screen;

use std::{
    collections::VecDeque,
    env,
    io::{self, stdout},
    time::{Duration, Instant},
};

use actions::{
    dispatch_menu, draw_menu, draw_modal, execute_modal, link_index_for_row, modal_click,
    Dispatch, MenuContext, MenuLink, MenuState, Modal, PendingAction, MENU_ENTRIES,
};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    style::{Color, ResetColor},
    terminal::{
        self, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use graph_view::{GraphView, Viewport};
use screen::{Frame, Renderer, Style};

const DROP_X: u16 = 0;
const DROP_HIT_WIDTH: u16 = 3; // virtual hit area: columns 0, 1, and 2
const FRAME_X: u16 = 2;
const CANVAS_X: u16 = FRAME_X + 1;
const MENU_WIDTH: u16 = 26;
const LOG_ROWS: u16 = 6;
const MIN_WIDTH: u16 = 58;
const MIN_CONTENT_HEIGHT: u16 = 18;
const TICK: Duration = Duration::from_millis(16);
const MAX_EVENT_BATCH: usize = 64;
const RESIZE_DEBOUNCE: Duration = Duration::from_millis(70);
const BUILD_ID: &str = "v0.14 · wide-cell tombstones";
const FALL_TICK: Duration = Duration::from_millis(90);

#[derive(Clone, Copy)]
struct Config {
    diagnostics: bool,
}

impl Config {
    fn from_args() -> Self {
        let mut diagnostics = env::var("EXPLORER_DIAGNOSTICS")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);

        for arg in env::args().skip(1) {
            match arg.as_str() {
                "--diagnostics" | "--diagnostic" | "--diag" | "-d" => {
                    diagnostics = true;
                }
                "--no-diagnostics" | "--no-diagnostic" => {
                    diagnostics = false;
                }
                _ => {}
            }
        }

        Self { diagnostics }
    }
}

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
    over_menu: bool,
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
    modal: Option<Modal>,
    logs: VecDeque<String>,
    falling: Option<FallingGhost>,
    links: Vec<MenuLink>,
    diagnostics: bool,
    resize_deadline: Option<Instant>,
    should_exit: bool,
    dirty: bool,
}

impl App {
    fn new(config: Config) -> io::Result<Self> {
        let graph = GraphView::from_current_dir()?;
        let mut logs = VecDeque::new();
        logs.push_back(format!("⇝ {BUILD_ID}"));
        logs.push_back(format!("⇝ Mounted {}", graph.root_label()));
        logs.push_back("⇝ LMB select/drag · MMB/WASD/arrows pan · Home center".to_string());
        logs.push_back("⇝ Tab changes menu segment · 0..9 runs local segment item".to_string());
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
            links: Vec::new(),
            diagnostics: config.diagnostics,
            resize_deadline: None,
            should_exit: false,
            dirty: true,
        })
    }

    fn menu_context(&self) -> MenuContext {
        match self.selected.and_then(|id| self.graph.node(id)) {
            Some(node) if node.is_dir => MenuContext::Folder,
            Some(_) => MenuContext::File,
            None => MenuContext::None,
        }
    }

    fn log(&mut self, message: impl Into<String>) {
        self.logs.push_back(format!("⇝ {}", message.into()));
        while self.logs.len() > LOG_ROWS as usize {
            self.logs.pop_front();
        }
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        let context = self.menu_context();
        self.menu.ensure_visible(context);
        self.dirty = true;
    }

    fn clear_transient(&mut self) {
        self.press = None;
        self.drag = None;
        self.pan = None;
    }

    fn schedule_resize(&mut self) {
        self.resize_deadline = Some(Instant::now() + RESIZE_DEBOUNCE);
    }

    fn update_resize(&mut self, now: Instant) {
        if self
            .resize_deadline
            .map(|deadline| now >= deadline)
            .unwrap_or(false)
        {
            self.resize_deadline = None;
            self.mark_dirty();
        }
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
    fn minimum_height(diagnostics: bool) -> u16 {
        MIN_CONTENT_HEIGHT + if diagnostics { LOG_ROWS } else { 0 }
    }

    fn current(diagnostics: bool) -> io::Result<Option<Self>> {
        let (width, height) = terminal::size()?;
        if width < MIN_WIDTH || height < Self::minimum_height(diagnostics) {
            return Ok(None);
        }

        let diagnostic_rows = if diagnostics { LOG_ROWS } else { 0 };
        let menu_x = width - MENU_WIDTH;
        let separator_y = height - diagnostic_rows - 1;
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
    let config = Config::from_args();
    let _terminal = TerminalGuard::enter()?;
    let mut out = stdout();
    let mut renderer = Renderer::default();
    let mut app = App::new(config)?;

    loop {
        let now = Instant::now();
        app.update_resize(now);
        update_animation(&mut app, now);

        if app.dirty && app.resize_deadline.is_none() {
            let frame = compose_frame(&app)?;
            renderer.present(&mut out, frame)?;
            app.dirty = false;
        }
        if app.should_exit {
            break;
        }

        if event::poll(TICK)? {
            // Coalesce a bounded burst before repainting. Drag and key-repeat
            // events often arrive in clusters; this renders the newest camera
            // position once instead of every intermediate position.
            for batch_index in 0..MAX_EVENT_BATCH {
                handle_event(&mut app, event::read()?)?;
                if app.should_exit || batch_index + 1 == MAX_EVENT_BATCH {
                    break;
                }
                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn handle_event(app: &mut App, event: Event) -> io::Result<()> {
    match event {
        Event::Resize(_, _) => app.schedule_resize(),
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key.code)?,
        Event::Mouse(mouse) => handle_mouse(app, mouse)?,
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) -> io::Result<()> {
    if app.modal.is_some() {
        let is_input = app
            .modal
            .as_ref()
            .map(|modal| modal.is_input())
            .unwrap_or(false);
        if is_input {
            match code {
                KeyCode::Enter => confirm_modal(app, true)?,
                KeyCode::Esc => confirm_modal(app, false)?,
                KeyCode::Backspace => {
                    let changed = app
                        .modal
                        .as_mut()
                        .map(|modal| modal.backspace())
                        .unwrap_or(false);
                    if changed {
                        app.mark_dirty();
                    }
                }
                KeyCode::Char(ch) => {
                    let changed = app
                        .modal
                        .as_mut()
                        .map(|modal| modal.push_char(ch))
                        .unwrap_or(false);
                    if changed {
                        app.mark_dirty();
                    }
                }
                _ => {}
            }
        } else {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    confirm_modal(app, true)?
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    confirm_modal(app, false)?
                }
                _ => {}
            }
        }
        return Ok(());
    }

    match code {
        KeyCode::Esc => app.should_exit = true,
        KeyCode::Tab => {
            let context = app.menu_context();
            if app.menu.cycle_section(false, context) {
                app.mark_dirty();
            }
        }
        KeyCode::BackTab => {
            let context = app.menu_context();
            if app.menu.cycle_section(true, context) {
                app.mark_dirty();
            }
        }
        KeyCode::Home => {
            if let Some(index) = MenuState::index_for_command(actions::MenuCommand::Center) {
                let context = app.menu_context();
                app.menu.set_cursor(index, context);
                invoke_menu(app, index)?;
            }
        }
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
            app.graph.pan(2, 0);
            app.mark_dirty();
        }
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
            app.graph.pan(-2, 0);
            app.mark_dirty();
        }
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
            app.graph.pan(0, 2);
            app.mark_dirty();
        }
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
            app.graph.pan(0, -2);
            app.mark_dirty();
        }
        KeyCode::Enter => {
            if app.menu_context() == MenuContext::Folder {
                if let Some(index) = MenuState::index_for_command(actions::MenuCommand::Enter) {
                    invoke_menu(app, index)?;
                }
            }
        }
        KeyCode::Delete | KeyCode::Backspace => {
            if let Some(source) = app.selected.filter(|id| *id != 0) {
                let mut modal = Modal::trash_node(&app.graph, source);
                if let Some(y) = selected_screen_y(app, source) {
                    modal = modal.with_origin_y(y);
                }
                app.modal = Some(modal);
                app.mark_dirty();
            }
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            let context = app.menu_context();
            if let Some(index) = app.menu.index_for_hotkey(ch, context) {
                app.menu.set_cursor(index, context);
                invoke_menu(app, index)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) -> io::Result<()> {
    let Some(layout) = Layout::current(app.diagnostics)? else {
        return Ok(());
    };

    if app.modal.is_some() {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(yes) = modal_click(layout.viewport, mouse.column, mouse.row) {
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
                let next = (pan.camera_x + dx * 2, pan.camera_y + dy * 2);
                if app.graph.camera() != next {
                    app.graph.set_camera(next.0, next.1);
                    app.mark_dirty();
                }
                return Ok(());
            }
            MouseEventKind::Up(_) => {
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
        let context = app.menu_context();
        if let Some(index) = MenuState::index_for_row(mouse.row, context) {
            if app.menu.set_cursor(index, context) {
                app.mark_dirty();
            }
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                invoke_menu(app, index)?;
            }
        } else if let Some(section) = MenuState::section_for_header_row(mouse.row) {
            if app.menu.set_section(section, context) {
                app.mark_dirty();
            }
        } else if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(link_index) =
                link_index_for_row(mouse.row, layout.separator_y, context, app.links.len())
            {
                open_link(app, link_index)?;
            }
        }
        return Ok(());
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Middle)
            if layout.viewport.contains(mouse.column, mouse.row) =>
        {
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
        MouseEventKind::Down(MouseButton::Left)
            if layout.viewport.contains(mouse.column, mouse.row) =>
        {
            if let Some(node) = app
                .graph
                .hit_test(layout.viewport, mouse.column, mouse.row)
            {
                if app.selected != Some(node) {
                    app.selected = Some(node);
                    app.mark_dirty();
                }
                app.press = Some(Press {
                    node,
                    start_x: mouse.column,
                    start_y: mouse.row,
                });
            } else {
                if app.selected.is_some() {
                    app.selected = None;
                    app.mark_dirty();
                }
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
                    let over_menu =
                        mouse.column >= layout.menu_x && mouse.row <= layout.separator_y;
                    let over_trash = !over_menu
                        && is_trash_column(mouse.column)
                        && mouse.row <= layout.separator_y;
                    let target = if over_trash || over_menu {
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
                        over_menu,
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
                let dropped_in_menu = drag.over_menu
                    || (mouse.column >= layout.menu_x && mouse.row <= layout.separator_y);
                let dropped_in_trash = !dropped_in_menu
                    && (drag.over_trash
                        || (is_trash_column(mouse.column)
                            && mouse.row <= layout.separator_y));
                if dropped_in_menu {
                    add_link(app, drag.node);
                } else if dropped_in_trash {
                    app.modal = Some(
                        Modal::trash_node(&app.graph, drag.node).with_origin_y(mouse.row),
                    );
                    app.mark_dirty();
                } else if let Some(target) = drag.target {
                    app.modal = Some(Modal::move_node(&app.graph, drag.node, target));
                    app.mark_dirty();
                } else {
                    app.log("MOVE · cancelled · target must be a valid folder");
                }
            }
            app.press = None;
            app.mark_dirty();
        }
        _ => {}
    }

    Ok(())
}

fn is_trash_column(x: u16) -> bool {
    x < DROP_HIT_WIDTH
}

fn add_link(app: &mut App, node_id: usize) {
    let Some(node) = app.graph.node(node_id) else {
        return;
    };
    if node.is_placeholder {
        return;
    }
    let link = MenuLink {
        path: node.path.clone(),
        is_dir: node.is_dir,
    };
    let status = format!("LINK · {}", link.path.display());
    app.links.retain(|existing| existing.path != link.path);
    app.links.push(link);
    app.log(status);
}

fn open_link(app: &mut App, index: usize) -> io::Result<()> {
    let Some(link) = app.links.get(index).cloned() else {
        return Ok(());
    };

    let result: io::Result<String> = if link.is_dir {
        match app.graph.mount_path(&link.path) {
            Ok(()) => {
                app.selected = None;
                Ok(format!("LINK · enter {}", link.path.display()))
            }
            Err(err) => Err(err),
        }
    } else if let Some(parent) = link.path.parent() {
        match app.graph.mount_path(parent) {
            Ok(()) => {
                app.selected = app.graph.find_node_by_path(&link.path);
                Ok(format!("LINK · {}", link.path.display()))
            }
            Err(err) => Err(err),
        }
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "link has no parent",
        ))
    };

    match result {
        Ok(status) => {
            app.clear_transient();
            app.log(status);
        }
        Err(err) => app.log(format!("LINK FAILED · {err}")),
    }
    Ok(())
}

fn invoke_menu(app: &mut App, index: usize) -> io::Result<()> {
    let entry = MENU_ENTRIES[index];
    match dispatch_menu(entry.command, &mut app.graph, app.selected)? {
        Dispatch::Exit => app.should_exit = true,
        Dispatch::Modal(mut modal) => {
            if modal.trash_origin_y.is_none() {
                let trash_source = match &modal.pending {
                    PendingAction::Trash { source } => Some(*source),
                    _ => None,
                };
                if let Some(source) = trash_source {
                    if let Some(y) = selected_screen_y(app, source) {
                        modal = modal.with_origin_y(y);
                    }
                }
            }
            app.modal = Some(modal);
            app.mark_dirty();
        }
        Dispatch::Status(status) => {
            if matches!(
                entry.command,
                actions::MenuCommand::Reload
                    | actions::MenuCommand::Parent
                    | actions::MenuCommand::Enter
                    | actions::MenuCommand::Depth
            ) {
                app.selected = None;
                app.clear_transient();
            }
            app.log(status);
        }
    }
    Ok(())
}

fn selected_screen_y(app: &App, source: usize) -> Option<u16> {
    let layout = Layout::current(app.diagnostics).ok().flatten()?;
    app.graph.screen_y(source, layout.viewport)
}

fn confirm_modal(app: &mut App, yes: bool) -> io::Result<()> {
    let Some(modal) = app.modal.take() else {
        return Ok(());
    };

    if !yes {
        app.log(if modal.is_input() {
            "INPUT · cancelled"
        } else {
            "CONFIRM · no · cancelled"
        });
        return Ok(());
    }

    if modal.is_input()
        && modal
            .input_value()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        app.modal = Some(modal);
        app.log("INPUT · name cannot be empty");
        return Ok(());
    }

    let retry_modal = if modal.is_input() {
        Some(modal.clone())
    } else {
        None
    };
    let falling_seed = match &modal.pending {
        PendingAction::Trash { source } => app.graph.node(*source).map(|node| {
            (
                if node.is_dir { "🖿" } else { "🖹" },
                modal.trash_origin_y,
            )
        }),
        _ => None,
    };

    match execute_modal(modal, &mut app.graph) {
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
                    app.mark_dirty();
                }
            }
        }
        Err(err) => {
            if retry_modal.is_some() {
                app.modal = retry_modal;
            }
            app.log(format!("ACTION FAILED · {err}"));
        }
    }
    Ok(())
}

fn update_animation(app: &mut App, now: Instant) {
    let stop_y = Layout::current(app.diagnostics)
        .ok()
        .flatten()
        // The paper bin occupies separator_y. The icon may reach exactly one
        // row above it, then disappears on the following animation tick.
        .map(|layout| layout.separator_y.saturating_sub(1).max(1))
        .unwrap_or(1);

    let mut changed = false;
    let mut finished = false;
    if let Some(falling) = app.falling.as_mut() {
        if now >= falling.next_step {
            changed = true;
            if falling.y >= stop_y {
                finished = true;
            } else {
                falling.y = falling.y.saturating_add(1).min(stop_y);
                falling.next_step = now + FALL_TICK;
            }
        }
    }

    if finished {
        app.falling = None;
    }
    if changed {
        app.mark_dirty();
    }
}

fn compose_frame(app: &App) -> io::Result<Frame> {
    let (width, height) = terminal::size()?;
    let mut frame = Frame::new(width, height);

    let Some(layout) = Layout::current(app.diagnostics)? else {
        print_at(&mut frame, 0, 0, "terminal too small", Style::default());
        print_at(
            &mut frame,
            0,
            1,
            &format!(
                "need at least {MIN_WIDTH}x{}; got {width}x{height}",
                Layout::minimum_height(app.diagnostics)
            ),
            Style::default(),
        );
        return Ok(frame);
    };

    draw_top_rule(&mut frame, app, layout.menu_x);
    draw_canvas_frame(&mut frame, layout.canvas_bottom_y);
    draw_dropzone(&mut frame, app, layout);

    let drag_id = app.drag.map(|drag| drag.node);
    let drop_target = app.drag.and_then(|drag| drag.target);
    app.graph.render(
        &mut frame,
        layout.viewport,
        app.selected,
        drag_id,
        drop_target,
    );

    if let Some(drag) = app.drag {
        draw_drag_box(&mut frame, app, drag, layout);
    }
    if let Some(falling) = &app.falling {
        draw_falling(&mut frame, falling, layout);
    }

    draw_menu(
        &mut frame,
        layout.menu_x,
        layout.separator_y,
        app.menu,
        MENU_WIDTH,
        app.menu_context(),
        &app.links,
        app.graph.depth_limit(),
    );
    draw_bottom_rule(
        &mut frame,
        app,
        layout.menu_x,
        layout.separator_y,
    );

    if app.diagnostics {
        draw_logs(
            &mut frame,
            app,
            layout.separator_y + 1,
            layout.width,
        );
    }

    if let Some(modal) = &app.modal {
        draw_modal(&mut frame, modal, layout.viewport);
    }

    Ok(frame)
}

fn draw_top_rule(frame: &mut Frame, app: &App, menu_x: u16) {
    let map_cluster = " 🗺 ";
    let map_width = map_cluster.chars().count() as u16;
    let map_x = menu_x.saturating_sub(map_width);
    let title = app.graph.root_label();

    draw_rule_title(frame, 0, 0, map_x, &title, "🞃", "🞁");
    print_at(frame, map_x, 0, map_cluster, Style::default());
}

fn draw_canvas_frame(frame: &mut Frame, bottom_y: u16) {
    for y in 1..=bottom_y {
        let edge = if y == 1 {
            "⎛"
        } else if y == bottom_y {
            "⎝"
        } else {
            "⎜"
        };
        print_at(frame, FRAME_X, y, edge, Style::default());
    }
}

fn draw_dropzone(frame: &mut Frame, app: &App, layout: Layout) {
    let active = app.drag.filter(|drag| drag.over_trash).and_then(|drag| {
        app.graph.node(drag.node).map(|node| {
            (
                drag.y.clamp(1, layout.canvas_bottom_y),
                if node.is_dir { "🖿" } else { "🖹" },
            )
        })
    });

    if let Some((y, icon)) = active {
        let active_style = Style::new(Color::White, Color::DarkRed);
        print_at(frame, DROP_X, y, icon, active_style);
        print_at(frame, DROP_X, layout.separator_y, "🗑", active_style);
    } else {
        print_at(
            frame,
            DROP_X,
            layout.separator_y,
            "🗑",
            Style::new(Color::DarkGrey, Color::Reset),
        );
    }
}

fn draw_drag_box(frame: &mut Frame, app: &App, drag: DragState, layout: Layout) {
    let Some(node) = app.graph.node(drag.node) else {
        return;
    };

    let icon = if node.is_dir { "🖿" } else { "🖹" };
    let name = node.name.clone();
    let inner = name.chars().count().clamp(4, 18);
    let visible = clip_text(&name, inner);
    let box_width = inner + 2;

    let mut x = drag.x.saturating_add(1);
    if x + box_width as u16 >= layout.menu_x {
        x = drag.x.saturating_sub(box_width as u16);
    }
    x = x.max(CANVAS_X);
    let y = drag
        .y
        .saturating_add(1)
        .min(layout.canvas_bottom_y.saturating_sub(2));

    let top_dashes = inner.saturating_sub(2);
    print_at(
        frame,
        x,
        y,
        &format!("  ╾{}╮", "─".repeat(top_dashes)),
        Style::default(),
    );
    print_at(
        frame,
        x,
        y + 1,
        &format!("╿{:<width$}│", visible, width = inner),
        Style::default(),
    );
    print_at(
        frame,
        x,
        y + 2,
        &format!("╰{}{}", "─".repeat(inner), icon),
        Style::default(),
    );
}

fn draw_falling(frame: &mut Frame, falling: &FallingGhost, layout: Layout) {
    let last_visible_y = layout.separator_y.saturating_sub(2).max(1);
    let y = falling.y.clamp(1, last_visible_y);
    print_at(
        frame,
        DROP_X,
        y,
        falling.icon,
        Style::new(Color::Grey, Color::Reset),
    );
}

fn draw_bottom_rule(frame: &mut Frame, app: &App, menu_x: u16, y: u16) {
    let selected_path = app
        .selected
        .and_then(|id| app.graph.node(id))
        .filter(|node| !node.is_placeholder)
        .map(|node| node.path.display().to_string())
        .filter(|path| !path.is_empty());

    match selected_path {
        Some(path) => draw_rule_title(frame, FRAME_X, y, menu_x, &path, "🞁", "🞃"),
        None => draw_pattern(frame, FRAME_X, y, menu_x, "🞁", "🞃"),
    }
}

fn draw_rule_title(
    frame: &mut Frame,
    start_x: u16,
    y: u16,
    end_x: u16,
    title: &str,
    first: &'static str,
    second: &'static str,
) {
    let width = end_x.saturating_sub(start_x) as usize;
    draw_pattern(frame, start_x, y, end_x, first, second);
    if width < 7 || title.is_empty() {
        return;
    }

    const SIDE: &str = "🞃🞁";
    let side_width = SIDE.chars().count();
    let fixed = side_width * 2 + 2;
    let title_width = width.saturating_sub(fixed).max(1);
    let visible = clip_text_tail(title, title_width);
    let label = format!("{SIDE} {visible} {SIDE}");
    let label_width = label.chars().count();

    if label_width >= width {
        print_at(
            frame,
            start_x,
            y,
            &clip_text_tail(&label, width),
            Style::default(),
        );
        return;
    }

    let x = start_x + ((width - label_width) / 2) as u16;
    print_at(frame, x, y, &label, Style::default());
}

fn draw_logs(frame: &mut Frame, app: &App, start_y: u16, width: u16) {
    for row in 0..LOG_ROWS {
        let text = app
            .logs
            .get(row as usize)
            .map(String::as_str)
            .unwrap_or("");
        let clipped = clip_text(text, width as usize);
        print_at(frame, 0, start_y + row, &clipped, Style::default());
    }
}

fn draw_pattern(
    frame: &mut Frame,
    start_x: u16,
    y: u16,
    end_x: u16,
    first: &'static str,
    second: &'static str,
) {
    for x in start_x..end_x {
        let glyph = if (x - start_x) % 2 == 0 {
            first
        } else {
            second
        };
        print_at(frame, x, y, glyph, Style::default());
    }
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
    let tail: String = text
        .chars()
        .rev()
        .take(max - 1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

fn print_at(frame: &mut Frame, x: u16, y: u16, text: &str, style: Style) {
    frame.put_str(x, y, text, style);
}
