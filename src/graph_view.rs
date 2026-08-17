use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::io::{Stdout, Write};

const MAX_DEPTH: usize = 4;
const MAX_NODES: usize = 140;

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Viewport {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x
            && x < self.x.saturating_add(self.width)
            && y >= self.y
            && y < self.y.saturating_add(self.height)
    }

    pub fn right(&self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub fn bottom(&self) -> u16 {
        self.y.saturating_add(self.height)
    }
}

#[derive(Clone, Debug)]
pub struct FsNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    depth: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct WorldPos {
    x: i32,
    y: i32,
}

pub struct GraphView {
    root: PathBuf,
    nodes: Vec<FsNode>,
    positions: Vec<WorldPos>,
    camera_x: i32,
    camera_y: i32,
    column_gap: i32,
    row_gap: i32,
}

impl GraphView {
    pub fn from_current_dir() -> io::Result<Self> {
        let root = std::env::current_dir()?;
        let mut view = Self {
            root,
            nodes: Vec::new(),
            positions: Vec::new(),
            camera_x: 1,
            camera_y: 1,
            column_gap: 18,
            row_gap: 2,
        };
        view.reload()?;
        Ok(view)
    }

    pub fn reload(&mut self) -> io::Result<()> {
        self.nodes.clear();

        let root_name = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.root.display().to_string());

        self.nodes.push(FsNode {
            id: 0,
            parent: None,
            path: self.root.clone(),
            name: root_name,
            is_dir: true,
            depth: 0,
        });

        let root = self.root.clone();
        self.scan_dir(0, &root, 1)?;
        self.rebuild_layout();
        Ok(())
    }

    pub fn mount_parent(&mut self) -> io::Result<bool> {
        let Some(parent) = self.root.parent().map(Path::to_path_buf) else {
            return Ok(false);
        };
        if parent == self.root {
            return Ok(false);
        }
        self.root = parent;
        self.center();
        self.reload()?;
        Ok(true)
    }

    fn scan_dir(&mut self, parent_id: usize, dir: &Path, depth: usize) -> io::Result<()> {
        if depth > MAX_DEPTH || self.nodes.len() >= MAX_NODES {
            return Ok(());
        }

        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
            Err(err) => return Err(err),
        };

        let mut children = Vec::new();
        for entry in entries.flatten() {
            if self.nodes.len() + children.len() >= MAX_NODES {
                break;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".explorer-trash" {
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            children.push((!file_type.is_dir(), name, entry.path(), file_type.is_dir(), file_type.is_symlink()));
        }

        children.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));

        for (_, name, path, is_dir, is_symlink) in children {
            if self.nodes.len() >= MAX_NODES {
                break;
            }

            let id = self.nodes.len();
            self.nodes.push(FsNode {
                id,
                parent: Some(parent_id),
                path: path.clone(),
                name,
                is_dir,
                depth,
            });

            if is_dir && !is_symlink {
                self.scan_dir(id, &path, depth + 1)?;
            }
        }

        Ok(())
    }

    fn rebuild_layout(&mut self) {
        self.positions = vec![WorldPos::default(); self.nodes.len()];
        let mut next_leaf = 0_i32;
        self.place_subtree(0, &mut next_leaf);
    }

    fn place_subtree(&mut self, id: usize, next_leaf: &mut i32) -> i32 {
        let depth = self.nodes[id].depth as i32;
        let children: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.parent == Some(id))
            .map(|n| n.id)
            .collect();

        let y = if children.is_empty() {
            let y = *next_leaf * self.row_gap;
            *next_leaf += 1;
            y
        } else {
            let mut first = None;
            let mut last = 0;
            for child in children {
                let cy = self.place_subtree(child, next_leaf);
                first.get_or_insert(cy);
                last = cy;
            }
            (first.unwrap_or(last) + last) / 2
        };

        self.positions[id] = WorldPos {
            x: depth * self.column_gap,
            y,
        };
        y
    }

    pub fn center(&mut self) {
        self.camera_x = 1;
        self.camera_y = 1;
    }

    pub fn camera(&self) -> (i32, i32) {
        (self.camera_x, self.camera_y)
    }

    pub fn set_camera(&mut self, x: i32, y: i32) {
        self.camera_x = x;
        self.camera_y = y;
    }

    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.camera_x += dx;
        self.camera_y += dy;
    }

    pub fn cycle_spacing(&mut self) -> i32 {
        self.column_gap += 4;
        if self.column_gap > 30 {
            self.column_gap = 14;
        }
        self.rebuild_layout();
        self.column_gap
    }

    pub fn root_label(&self) -> String {
        self.root.display().to_string()
    }

    pub fn node(&self, id: usize) -> Option<&FsNode> {
        self.nodes.get(id)
    }

    pub fn label(&self, id: usize) -> String {
        self.node(id)
            .map(|n| {
                if n.is_dir {
                    format!("{}/", n.name.trim_end_matches('/'))
                } else {
                    n.name.clone()
                }
            })
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn hit_test(&self, viewport: Viewport, x: u16, y: u16) -> Option<usize> {
        if !viewport.contains(x, y) {
            return None;
        }

        for node in self.nodes.iter().rev() {
            let (sx, sy) = self.screen_pos(node.id, viewport)?;
            let width = self.node_width(node.id) as i32;
            if y as i32 == sy && x as i32 >= sx && (x as i32) < sx + width {
                return Some(node.id);
            }
        }
        None
    }

    pub fn folder_drop_target(
        &self,
        viewport: Viewport,
        x: u16,
        y: u16,
        source: usize,
    ) -> Option<usize> {
        let target = self.hit_test(viewport, x, y)?;
        let target_node = self.node(target)?;
        if !target_node.is_dir || !self.can_move(source, target) {
            return None;
        }
        Some(target)
    }

    pub fn can_move(&self, source: usize, target: usize) -> bool {
        if source == 0 || source == target {
            return false;
        }
        let Some(src) = self.node(source) else {
            return false;
        };
        let Some(dst) = self.node(target) else {
            return false;
        };
        if !dst.is_dir || src.parent == Some(target) {
            return false;
        }
        if src.is_dir && self.is_descendant(target, source) {
            return false;
        }
        true
    }

    fn is_descendant(&self, candidate: usize, ancestor: usize) -> bool {
        let mut current = self.node(candidate).and_then(|n| n.parent);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.node(id).and_then(|n| n.parent);
        }
        false
    }

    pub fn move_node(&mut self, source: usize, target: usize) -> io::Result<String> {
        if !self.can_move(source, target) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid folder move"));
        }

        let src = self.node(source).cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "source node disappeared")
        })?;
        let dst = self.node(target).cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "target folder disappeared")
        })?;
        let name = src.path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "source has no file name")
        })?;
        let destination = dst.path.join(name);
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists in {}", src.name, dst.name),
            ));
        }

        fs::rename(&src.path, &destination)?;
        let message = format!("MOVE · {} → {}", src.name, dst.name);
        self.reload()?;
        Ok(message)
    }

    pub fn trash_node(&mut self, source: usize) -> io::Result<String> {
        if source == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot trash root"));
        }

        let src = self.node(source).cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "source node disappeared")
        })?;

        let trash = self.root.join(".explorer-trash");
        fs::create_dir_all(&trash)?;

        let original = src.path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "source has no file name")
        })?;
        let mut destination = trash.join(original);

        if destination.exists() {
            let stem = src
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("item");
            let ext = src.path.extension().and_then(|s| s.to_str());
            for n in 2..10_000 {
                let candidate = match ext {
                    Some(ext) if !src.is_dir => format!("{stem}.{n}.{ext}"),
                    _ => format!("{stem}.{n}"),
                };
                destination = trash.join(candidate);
                if !destination.exists() {
                    break;
                }
            }
        }

        fs::rename(&src.path, &destination)?;
        let message = format!("TRASH · {} → .explorer-trash/", src.name);
        self.reload()?;
        Ok(message)
    }

    pub fn render(
        &self,
        out: &mut Stdout,
        viewport: Viewport,
        selected: Option<usize>,
        drag_id: Option<usize>,
        drop_target: Option<usize>,
    ) -> io::Result<()> {
        for node in &self.nodes {
            let Some(parent_id) = node.parent else {
                continue;
            };
            let Some((px, py)) = self.screen_pos(parent_id, viewport) else {
                continue;
            };
            let Some((cx, cy)) = self.screen_pos(node.id, viewport) else {
                continue;
            };

            let parent_end = px + self.node_width(parent_id) as i32;
            let elbow = (cx - 2).max(parent_end + 1);
            self.draw_h(out, viewport, parent_end, elbow, py, '─')?;
            self.draw_v(out, viewport, elbow, py, cy, '│')?;
            self.draw_h(out, viewport, elbow, cx - 1, cy, '─')?;
        }

        for node in &self.nodes {
            let Some((sx, sy)) = self.screen_pos(node.id, viewport) else {
                continue;
            };
            if sy < viewport.y as i32 || sy >= viewport.bottom() as i32 {
                continue;
            }

            let label = self.label(node.id);
            let mut fg = Color::Grey;
            let mut bg = Color::Reset;

            if node.is_dir {
                fg = Color::Black;
                bg = Color::White;
            }
            if selected == Some(node.id) {
                fg = Color::Black;
                bg = Color::DarkYellow;
            }
            if drop_target == Some(node.id) {
                fg = Color::Black;
                bg = Color::Cyan;
            }
            if drag_id == Some(node.id) {
                fg = Color::DarkGrey;
                bg = Color::Reset;
            }

            self.print_clipped(out, viewport, sx, sy, &label, fg, bg)?;
        }

        queue!(out, ResetColor)?;
        Ok(())
    }

    fn screen_pos(&self, id: usize, viewport: Viewport) -> Option<(i32, i32)> {
        let p = *self.positions.get(id)?;
        Some((
            viewport.x as i32 + p.x + self.camera_x,
            viewport.y as i32 + p.y + self.camera_y,
        ))
    }

    fn node_width(&self, id: usize) -> usize {
        self.label(id).chars().count().max(1)
    }

    fn draw_h(
        &self,
        out: &mut Stdout,
        viewport: Viewport,
        x0: i32,
        x1: i32,
        y: i32,
        ch: char,
    ) -> io::Result<()> {
        if y < viewport.y as i32 || y >= viewport.bottom() as i32 {
            return Ok(());
        }
        let start = x0.min(x1).max(viewport.x as i32);
        let end = x0.max(x1).min(viewport.right() as i32 - 1);
        for x in start..=end {
            queue!(out, MoveTo(x as u16, y as u16), SetForegroundColor(Color::DarkGrey), Print(ch))?;
        }
        Ok(())
    }

    fn draw_v(
        &self,
        out: &mut Stdout,
        viewport: Viewport,
        x: i32,
        y0: i32,
        y1: i32,
        ch: char,
    ) -> io::Result<()> {
        if x < viewport.x as i32 || x >= viewport.right() as i32 {
            return Ok(());
        }
        let start = y0.min(y1).max(viewport.y as i32);
        let end = y0.max(y1).min(viewport.bottom() as i32 - 1);
        for y in start..=end {
            queue!(out, MoveTo(x as u16, y as u16), SetForegroundColor(Color::DarkGrey), Print(ch))?;
        }
        Ok(())
    }

    fn print_clipped(
        &self,
        out: &mut Stdout,
        viewport: Viewport,
        x: i32,
        y: i32,
        text: &str,
        fg: Color,
        bg: Color,
    ) -> io::Result<()> {
        if y < viewport.y as i32 || y >= viewport.bottom() as i32 {
            return Ok(());
        }

        let mut sx = x;
        for ch in text.chars() {
            if sx >= viewport.x as i32 && sx < viewport.right() as i32 {
                queue!(
                    out,
                    MoveTo(sx as u16, y as u16),
                    SetForegroundColor(fg),
                    SetBackgroundColor(bg),
                    Print(ch)
                )?;
            }
            sx += 1;
            if sx >= viewport.right() as i32 {
                break;
            }
        }
        Ok(())
    }
}
