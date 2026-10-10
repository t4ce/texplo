//! Termdir owns this window. Shell3 supplies only the launch directive.
//! The chrome shares termdir's composition and actions; graph production is disabled.
use super::*;
use trueos::{
    input,
    ui4_scene::{self, Damage, Font, SceneTextRow},
};

const CELL_WIDTH: u32 = 10;
const CELL_HEIGHT: u32 = 20;

fn ui_error(error: ui4_scene::Error) -> io::Error {
    io::Error::other(format!("termdir UI4: {error:?}"))
}

fn retry(mut operation: impl FnMut() -> Result<(), ui4_scene::Error>) -> io::Result<()> {
    loop {
        match operation() {
            Ok(()) => return Ok(()),
            Err(ui4_scene::Error::Busy) => {
                trueos::vsys::poll_once();
                trueos::vsys::sleep_ms(1);
            }
            Err(error) => return Err(ui_error(error)),
        }
    }
}

fn cell_dimensions(zoom: usize) -> (u32, u32) {
    let percent = ZOOM_LEVELS[zoom] as u32;
    (
        (CELL_WIDTH * percent / 100).max(1),
        (CELL_HEIGHT * percent / 100).max(1),
    )
}

pub(super) fn cell_size(width: u32, height: u32, zoom: usize) -> (u16, u16) {
    let (cw, ch) = cell_dimensions(zoom);
    (
        (width / cw).min(u16::MAX as u32) as u16,
        (height / ch).min(u16::MAX as u32) as u16,
    )
}

fn color(color: Color, background: bool) -> [u8; 4] {
    let rgb = match color {
        Color::Reset => {
            if background {
                [35, 39, 39]
            } else {
                [232, 232, 232]
            }
        }
        Color::Black => [0, 0, 0],
        Color::DarkGrey => [128, 128, 128],
        Color::Red => [255, 0, 0],
        Color::DarkRed => [128, 0, 0],
        Color::Green => [0, 255, 0],
        Color::DarkGreen => [0, 128, 0],
        Color::Yellow => [255, 255, 0],
        Color::DarkYellow => [128, 128, 0],
        Color::Blue => [0, 0, 255],
        Color::DarkBlue => [0, 0, 128],
        Color::Magenta => [255, 0, 255],
        Color::DarkMagenta => [128, 0, 128],
        Color::Cyan => [0, 255, 255],
        Color::DarkCyan => [0, 128, 128],
        Color::White => [255, 255, 255],
        Color::Grey => [192, 192, 192],
        Color::Rgb { r, g, b } => [r, g, b],
        Color::AnsiValue(index) => {
            const BASIC: [[u8; 3]; 16] = [
                [0, 0, 0],
                [128, 0, 0],
                [0, 128, 0],
                [128, 128, 0],
                [0, 0, 128],
                [128, 0, 128],
                [0, 128, 128],
                [192, 192, 192],
                [128, 128, 128],
                [255, 0, 0],
                [0, 255, 0],
                [255, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
                [0, 255, 255],
                [255, 255, 255],
            ];
            if index < 16 {
                BASIC[index as usize]
            } else if index >= 232 {
                [8 + (index - 232) * 10; 3]
            } else {
                let value = index - 16;
                let channel = |n| if n == 0 { 0 } else { 55 + n * 40 };
                [
                    channel(value / 36),
                    channel((value / 6) % 6),
                    channel(value % 6),
                ]
            }
        }
    };
    [rgb[0], rgb[1], rgb[2], 255]
}

/// Direct styled cells to UI4. No ANSI encoding or Shell3 cell parser.
/// This first native backend repaints chrome on changes; it has no graph pan surface.
fn present(window: &mut ui4_scene::Frame, frame: &Frame, zoom: usize) -> io::Result<()> {
    let (cell_width, cell_height) = cell_dimensions(zoom);
    let (width, height) = (window.width(), window.height());
    let mut pixels = color(Color::Reset, true).repeat(width as usize * height as usize);
    let mut glyphs = Vec::new();
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let cell = frame.cell(x, y);
            let bg = color(cell.style.bg, true);
            let fg = color(cell.style.fg, false);
            for py in y as u32 * cell_height..((y as u32 + 1) * cell_height).min(height) {
                for px in x as u32 * cell_width..((x as u32 + 1) * cell_width).min(width) {
                    let offset = ((py * width + px) * 4) as usize;
                    pixels[offset..offset + 4].copy_from_slice(
                        if cell.style.underline && py % cell_height == cell_height - 2 {
                            &fg
                        } else {
                            &bg
                        },
                    );
                }
            }
            if !cell.is_continuation() && cell.ch != ' ' {
                glyphs.push((x, y, fg, cell.ch.to_string()));
            }
        }
    }
    retry(|| window.begin(ui4_scene::rgba(35, 39, 39, 255)))?;
    retry(|| window.write_opaque_rgba8(&pixels))?;
    // Each color is one ordered layer in the same asynchronous FontKernel request.
    let mut colors = Vec::new();
    for &(_, _, fg, _) in &glyphs {
        if !colors.contains(&fg) {
            colors.push(fg);
        }
    }
    for fg in colors {
        let rows: Vec<_> = glyphs
            .iter()
            .filter(|glyph| glyph.2 == fg)
            .map(|(x, y, _, text)| SceneTextRow {
                text,
                x: *x as f32 * cell_width as f32,
                y: *y as f32 * cell_height as f32,
                font_pixels: cell_height as f32,
            })
            .collect();
        for chunk in rows.chunks(64) {
            window
                .stamp_text_scene(
                    Font::Inconsolata,
                    (width, height),
                    u32::from_le_bytes(fg),
                    chunk,
                )
                .map_err(ui_error)?;
        }
    }
    retry(|| window.publish(Damage::full(width, height)))
}

fn modifiers(bits: u8) -> KeyModifiers {
    let mut result = KeyModifiers::empty();
    if bits & 0x11 != 0 {
        result |= KeyModifiers::CONTROL;
    }
    if bits & 0x22 != 0 {
        result |= KeyModifiers::SHIFT;
    }
    if bits & 0x44 != 0 {
        result |= KeyModifiers::ALT;
    }
    result
}

fn key(event: input::TrueosKeyboardOutputEvent) -> Option<(KeyCode, KeyModifiers)> {
    let modifiers = modifiers(event.modifiers);
    if event.kind == input::KEYBOARD_OUTPUT_KIND_TEXT {
        return char::from_u32(event.codepoint).map(|ch| (KeyCode::Char(ch), modifiers));
    }
    if event.flags & input::KEYBOARD_OUTPUT_FLAG_PRESS == 0 {
        return None;
    }
    let code = match event.key_code {
        input::KEYBOARD_KEY_ESCAPE => KeyCode::Esc,
        input::KEYBOARD_KEY_TAB if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        input::KEYBOARD_KEY_TAB => KeyCode::Tab,
        input::KEYBOARD_KEY_ENTER => KeyCode::Enter,
        input::KEYBOARD_KEY_BACKSPACE => KeyCode::Backspace,
        input::KEYBOARD_KEY_DELETE => KeyCode::Delete,
        input::KEYBOARD_KEY_HOME => KeyCode::Home,
        input::KEYBOARD_KEY_END => KeyCode::End,
        input::KEYBOARD_KEY_PAGE_UP => KeyCode::PageUp,
        input::KEYBOARD_KEY_PAGE_DOWN => KeyCode::PageDown,
        input::KEYBOARD_KEY_ARROW_LEFT => KeyCode::Left,
        input::KEYBOARD_KEY_ARROW_RIGHT => KeyCode::Right,
        input::KEYBOARD_KEY_ARROW_UP => KeyCode::Up,
        input::KEYBOARD_KEY_ARROW_DOWN => KeyCode::Down,
        _ => return None,
    };
    Some((code, modifiers))
}

#[derive(Default)]
struct Keyboard {
    suppressed: Option<(u32, u32, u32)>,
}
impl Keyboard {
    fn take(&mut self, event: input::TrueosKeyboardOutputEvent) -> Option<(KeyCode, KeyModifiers)> {
        let identity = (event.controller_id, event.device_seq, event.codepoint);
        if event.kind == input::KEYBOARD_OUTPUT_KIND_TEXT {
            if self.suppressed.take() == Some(identity) {
                return None;
            }
            return key(event);
        }
        let result = key(event);
        self.suppressed = (result.is_some() && event.codepoint != 0).then_some(identity);
        result
    }
}

fn pointer(app: &mut App, event: ui4_scene::PointerEvent) -> io::Result<()> {
    let (columns, rows) = app.size()?;
    let (cell_width, cell_height) = cell_dimensions(app.zoom_step);
    if columns == 0 || rows == 0 || event.local_x < 0 || event.local_y < 0 {
        return Ok(());
    }
    let column = (event.local_x as u32 / cell_width).min(columns as u32 - 1) as u16;
    let row = (event.local_y as u32 / cell_height).min(rows as u32 - 1) as u16;
    let mut kinds = Vec::new();
    for (mask, button) in [
        (ui4_scene::POINTER_BUTTON_PRIMARY, MouseButton::Left),
        (ui4_scene::POINTER_BUTTON_SECONDARY, MouseButton::Right),
        (ui4_scene::POINTER_BUTTON_MIDDLE, MouseButton::Middle),
    ] {
        if event.buttons_pressed & mask != 0 {
            kinds.push(MouseEventKind::Down(button));
        }
        if event.buttons_released & mask != 0 {
            kinds.push(MouseEventKind::Up(button));
        }
        if event.buttons_down & mask != 0 && (event.dx != 0 || event.dy != 0) {
            kinds.push(MouseEventKind::Drag(button));
        }
    }
    if event.wheel != 0 {
        kinds.push(if event.wheel > 0 {
            MouseEventKind::ScrollUp
        } else {
            MouseEventKind::ScrollDown
        });
    }
    if kinds.is_empty() {
        kinds.push(MouseEventKind::Moved);
    }
    for kind in kinds {
        handle_mouse(
            app,
            MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::empty(),
            },
        )?;
    }
    Ok(())
}

pub(super) fn run(config: Config) -> io::Result<()> {
    let (display_width, display_height) = ui4_scene::output_dimensions().map_err(ui_error)?;
    let width = 1200.min(display_width.saturating_mul(9) / 10).max(1);
    let height = 840.min(display_height.saturating_mul(9) / 10).max(1);
    let mut window = ui4_scene::Frame::open(
        ((display_width - width) / 2) as i32,
        ((display_height - height) / 2) as i32,
        width,
        height,
    )
    .map_err(ui_error)?;
    window.set_title("termdir").map_err(ui_error)?;
    window.set_primary_activation(true).map_err(ui_error)?;
    window
        .set_escape_key_action(trueos::ui4_solara_text::FrameEscapeKeyAction::DeliverToApplication)
        .map_err(ui_error)?;
    let mut app = App::new(config)?;
    app.native_extent = Some((width, height));
    let mut keyboard = Keyboard::default();
    loop {
        for _ in 0..MAX_EVENT_BATCH {
            let Some(event) = window.take_keyboard_event().map_err(ui_error)? else {
                break;
            };
            if let Some((code, modifiers)) = keyboard.take(event) {
                handle_key(&mut app, code, modifiers)?;
            }
        }
        for _ in 0..MAX_EVENT_BATCH {
            let Some(event) = window.take_pointer_event().map_err(ui_error)? else {
                break;
            };
            pointer(&mut app, event)?;
        }
        if let Some(event) = window.take_resize_event().map_err(ui_error)? {
            retry(|| window.resize(event.width, event.height))?;
            app.native_extent = Some((event.width, event.height));
            app.mark_dirty();
        }
        if app.should_exit {
            break;
        }
        update_animation(&mut app, Instant::now());
        if app.dirty {
            let frame = compose_frame(&mut app)?;
            present(&mut window, &frame, app.zoom_step)?;
            app.dirty = false;
        }
        trueos::vsys::poll_once();
        trueos::vsys::sleep_ms(16);
    }
    window
        .close(ui4_scene::CloseRequest::default())
        .map_err(ui_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new(Config {
            backend: Backend::Ui4,
            diagnostics: false,
            initial_path: Some(PathBuf::from("/no-filesystem-read-needed")),
            initial_depth: Some(2),
        })
        .unwrap();
        app.native_extent = Some((1000, 700));
        app
    }

    fn row(frame: &Frame, y: u16) -> String {
        (0..frame.width()).map(|x| frame.cell(x, y).ch).collect()
    }

    #[test]
    fn native_chrome_composes_controls_and_empty_graph_without_terminal_queries() {
        let mut app = app();
        let frame = compose_frame(&mut app).unwrap();
        assert_eq!((frame.width(), frame.height()), (100, 35));
        let text = (0..frame.height())
            .map(|y| row(&frame, y))
            .collect::<Vec<_>>()
            .join("\n");
        for expected in ["Mount", "View", "Act", "Clip", "M A P", "not ready"] {
            assert!(text.contains(expected), "missing chrome {expected}");
        }
        assert!(app.graph.minimap_samples().is_empty());
        assert!(app.graph.node(1).is_none());
        let layout = Layout::current(&app).unwrap().unwrap();
        let map = minimap_geometry(&app, layout).unwrap();
        let label_y = layout.viewport.y + layout.viewport.height / 2;
        for y in layout.viewport.y..layout.viewport.bottom() {
            for x in layout.viewport.x..layout.viewport.right() {
                if !map.contains(x, y) && y != label_y {
                    assert_eq!(frame.cell(x, y).ch, ' ', "unexpected graph at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn native_chrome_reload_and_depth_never_populate_graph() {
        let mut app = app();
        app.graph.reload().unwrap();
        app.graph.set_depth_limit(7).unwrap();
        app.graph.cycle_spacing();
        app.graph.toggle_layout_mode();
        assert!(app.graph.node(0).is_some());
        assert!(app.graph.node(1).is_none());
        assert!(app.graph.minimap_samples().is_empty());
        let layout = Layout::current(&app).unwrap().unwrap();
        let mut frame = Frame::new(100, 35);
        app.graph
            .render(&mut frame, layout.viewport, None, None, None);
        assert!((0..frame.height()).all(|y| row(&frame, y).chars().all(|ch| ch == ' ')));
    }

    #[test]
    fn native_chrome_resize_controls_and_close_use_own_geometry() {
        let mut app = app();
        let section = app.menu.section;
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::empty()).unwrap();
        assert_ne!(app.menu.section, section);
        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::empty(),
            },
        )
        .unwrap();
        assert!(!app.minimap_visible);
        app.native_extent = Some((1280, 960));
        let frame = compose_frame(&mut app).unwrap();
        assert_eq!((frame.width(), frame.height()), (128, 48));
        assert!(!(0..48).any(|y| row(&frame, y).contains("M A P")));
        let layout = Layout::current(&app).unwrap().unwrap();
        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: layout.menu_x + MENU_WIDTH - 1,
                row: 0,
                modifiers: KeyModifiers::empty(),
            },
        )
        .unwrap();
        assert!(app.should_exit);
    }

    #[test]
    fn native_chrome_zoom_and_root_creation_modal_remain_operable() {
        let mut app = app();
        let zoom = MenuState::index_for_command(actions::MenuCommand::Zoom).unwrap();
        invoke_menu(&mut app, zoom).unwrap();
        assert_eq!(app.size().unwrap(), (83, 28));
        let create = MenuState::index_for_command(actions::MenuCommand::NewFolder).unwrap();
        invoke_menu(&mut app, create).unwrap();
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::empty()).unwrap();
        assert!(
            app.modal
                .as_ref()
                .unwrap()
                .input_value()
                .unwrap()
                .contains('x')
        );
        let frame = compose_frame(&mut app).unwrap();
        assert_eq!((frame.width(), frame.height()), (83, 28));
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::empty()).unwrap();
        assert!(app.modal.is_none() && !app.should_exit);
        assert!(app.graph.node(1).is_none());
    }

    #[test]
    fn native_chrome_named_key_text_pair_is_delivered_once_and_ctrl_q_is_preserved() {
        let mut keyboard = Keyboard::default();
        let mut event = input::TrueosKeyboardOutputEvent::default();
        event.kind = input::KEYBOARD_OUTPUT_KIND_KEY;
        event.flags = input::KEYBOARD_OUTPUT_FLAG_PRESS;
        event.key_code = input::KEYBOARD_KEY_TAB;
        event.codepoint = 9;
        assert_eq!(keyboard.take(event).unwrap().0, KeyCode::Tab);
        event.kind = input::KEYBOARD_OUTPUT_KIND_TEXT;
        assert!(keyboard.take(event).is_none());
        event.codepoint = 'q' as u32;
        event.modifiers = 1;
        let (code, mods) = keyboard.take(event).unwrap();
        let mut app = app();
        handle_key(&mut app, code, mods).unwrap();
        assert!(app.should_exit && app.should_shutdown);
    }
}
