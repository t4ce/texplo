use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use crossterm::style::Color;

use crate::{
    layout::{self, LayoutMode, LayoutNode, WorldPos},
    screen::{terminal_cell_width, text_cell_width, Frame, Style},
};

const HARD_MAX_DEPTH: usize = 256;
const DEPTH_LEVELS: [usize; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
const DEFAULT_DEPTH_LIMIT: usize = 4;
const MAX_CHILDREN_PER_DIR: usize = 256;
const MAX_VISIBLE_NODES: usize = 256;

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

#[derive(Clone, Copy, Debug)]
pub struct WorldBounds {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

#[derive(Clone, Debug)]
pub struct FsNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_placeholder: bool,
    pub is_removed: bool,
    hidden: bool,
    depth: usize,
}

#[derive(Clone, Debug)]
pub struct SelectionStats {
    pub key: String,
    pub kind: String,
    pub size: String,
    pub modified: String,
    pub access: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    /// Preserve the existing connector choice: S-curve in tree layout and
    /// radial spline in radial layout.
    Default,
    Straight,
    Elbow,
    SoftArc,
}

impl LineStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Straight => "straight",
            Self::Elbow => "elbow",
            Self::SoftArc => "soft-arc",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Default => Self::Straight,
            Self::Straight => Self::Elbow,
            Self::Elbow => Self::SoftArc,
            Self::SoftArc => Self::Default,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct EdgeCell {
    x: i32,
    y: i32,
    mask: u8,
}

pub struct GraphView {
    root: PathBuf,
    nodes: Vec<FsNode>,
    positions: Vec<WorldPos>,
    edge_cells: Vec<EdgeCell>,
    camera_x: i32,
    camera_y: i32,
    column_gap: i32,
    layout_mode: LayoutMode,
    line_style: LineStyle,
    suppressed_parent_edges: Vec<bool>,
    depth_limit: usize,
    scene_revision: u64,
}

impl GraphView {
    pub fn from_current_dir() -> io::Result<Self> {
        let root = std::env::current_dir()?;
        let mut view = Self {
            root,
            nodes: Vec::new(),
            positions: Vec::new(),
            edge_cells: Vec::new(),
            camera_x: 0,
            camera_y: 0,
            column_gap: 18,
            layout_mode: LayoutMode::Tree,
            line_style: LineStyle::Default,
            suppressed_parent_edges: Vec::new(),
            depth_limit: DEFAULT_DEPTH_LIMIT,
            scene_revision: 0,
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

        // The mount root remains structural only. It participates in hierarchy,
        // move validation, and layout centering, but is never rendered as a node.
        self.nodes.push(FsNode {
            id: 0,
            parent: None,
            path: self.root.clone(),
            name: root_name,
            is_dir: true,
            is_placeholder: false,
            is_removed: false,
            hidden: false,
            depth: 0,
        });

        let root = self.root.clone();
        let mut remaining = MAX_VISIBLE_NODES;
        self.scan_dir(0, &root, 1, &mut remaining)?;
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

    pub fn mount_node(&mut self, id: usize) -> io::Result<()> {
        let node = self
            .node(id)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "folder disappeared"))?;
        if node.is_placeholder || node.is_removed || node.hidden || !node.is_dir {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a folder"));
        }
        self.mount_path(&node.path)
    }

    pub fn mount_path(&mut self, path: &Path) -> io::Result<()> {
        let meta = fs::metadata(path)?;
        if !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mount target is not a folder",
            ));
        }
        self.root = path.to_path_buf();
        self.center();
        self.reload()
    }

    pub fn find_node_by_path(&self, path: &Path) -> Option<usize> {
        self.nodes
            .iter()
            .find(|node| {
                !node.is_placeholder && !node.is_removed && !node.hidden && node.path == path
            })
            .map(|node| node.id)
    }

    fn scan_dir(
        &mut self,
        parent_id: usize,
        dir: &Path,
        depth: usize,
        remaining: &mut usize,
    ) -> io::Result<bool> {
        // depth_limit counts nested levels *after* the immediate children of
        // the mounted directory. Thus depth 0 still renders direct siblings,
        // while depth 4 renders filesystem depths 1 through 5.
        let max_visible_depth = self.depth_limit.saturating_add(1).min(HARD_MAX_DEPTH);
        if depth > max_visible_depth {
            return Ok(false);
        }

        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => return Ok(false),
            Err(err) => return Err(err),
        };

        // Only inspect enough directory entries to establish the per-folder cap.
        // This keeps an enormous directory from becoming an enormous allocation.
        let mut children = Vec::new();
        let mut directory_truncated = false;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".explorer-trash" {
                continue;
            }

            if children.len() >= MAX_CHILDREN_PER_DIR {
                directory_truncated = true;
                break;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            children.push((
                !file_type.is_dir(),
                name,
                entry.path(),
                file_type.is_dir(),
                file_type.is_symlink(),
            ));
        }

        children.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));

        // Add immediate children first. In particular, the mount root gets all of
        // its visible top-level islands before any one subtree can consume the
        // global 256-node rendering budget.
        let mut recurse = Vec::new();
        let mut globally_truncated = false;
        for (_, name, path, is_dir, is_symlink) in children {
            if *remaining == 0 {
                globally_truncated = true;
                break;
            }

            let id = self.nodes.len();
            self.nodes.push(FsNode {
                id,
                parent: Some(parent_id),
                path: path.clone(),
                name,
                is_dir,
                is_placeholder: false,
                is_removed: false,
                hidden: false,
                depth,
            });
            *remaining -= 1;

            if is_dir && !is_symlink {
                recurse.push((id, path));
            }
        }

        if directory_truncated || globally_truncated {
            self.push_placeholder(parent_id, depth);
        }
        if globally_truncated {
            return Ok(true);
        }

        for (id, path) in recurse {
            if *remaining == 0 {
                self.push_placeholder(parent_id, depth);
                return Ok(true);
            }
            if self.scan_dir(id, &path, depth + 1, remaining)? {
                self.push_placeholder(parent_id, depth);
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn push_placeholder(&mut self, parent_id: usize, depth: usize) {
        if self
            .nodes
            .iter()
            .any(|node| node.parent == Some(parent_id) && node.is_placeholder)
        {
            return;
        }

        let id = self.nodes.len();
        self.nodes.push(FsNode {
            id,
            parent: Some(parent_id),
            path: PathBuf::new(),
            name: "...".to_string(),
            is_dir: false,
            is_placeholder: true,
            is_removed: false,
            hidden: false,
            depth,
        });
    }

    fn rebuild_layout(&mut self) {
        let layout_nodes: Vec<LayoutNode> = self
            .nodes
            .iter()
            .map(|node| LayoutNode {
                id: node.id,
                parent: node.parent,
                depth: node.depth,
                visual_width: if node.is_placeholder {
                    3
                } else {
                    // icon + one separating space + filename/folder name
                    2 + node.name.chars().count()
                },
                is_dir: node.is_dir,
            })
            .collect();

        self.positions = match self.layout_mode {
            LayoutMode::Tree => layout::tree_layout(&layout_nodes, self.column_gap),
            LayoutMode::Radial => layout::radial_layout(&layout_nodes, self.column_gap),
        };
        self.suppressed_parent_edges = match self.layout_mode {
            LayoutMode::Tree => layout::tree_suppressed_parent_edges(&layout_nodes),
            LayoutMode::Radial => vec![false; layout_nodes.len()],
        };
        self.rebuild_edge_cache();
        self.bump_scene_revision();
    }

    fn rebuild_edge_cache(&mut self) {
        let mut braille = WorldBrailleCanvas::new();
        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            let Some(parent_id) = node.parent else {
                continue;
            };
            if parent_id == 0
                || self
                    .node(parent_id)
                    .map(|parent| parent.hidden)
                    .unwrap_or(true)
            {
                continue;
            }
            if self.layout_mode == LayoutMode::Tree
                && self
                    .suppressed_parent_edges
                    .get(node.id)
                    .copied()
                    .unwrap_or(false)
            {
                continue;
            }

            let Some(parent_pos) = self.positions.get(parent_id).copied() else {
                continue;
            };
            let Some(child_pos) = self.positions.get(node.id).copied() else {
                continue;
            };
            let parent = (parent_pos.x, parent_pos.y);
            let child = (child_pos.x, child_pos.y);
            let a = edge_anchor(
                parent,
                self.node_width(parent_id),
                child,
                self.node_width(node.id),
            );
            let b = edge_anchor(
                child,
                self.node_width(node.id),
                parent,
                self.node_width(parent_id),
            );

            match self.line_style {
                LineStyle::Default => match self.layout_mode {
                    LayoutMode::Tree => braille.draw_s_curve(a, b),
                    LayoutMode::Radial => braille.draw_radial_spline(a, b),
                },
                LineStyle::Straight => braille.draw_straight(a, b),
                LineStyle::Elbow => braille.draw_elbow(a, b),
                LineStyle::SoftArc => braille.draw_soft_arc(a, b),
            }
        }
        self.edge_cells = braille.into_cells();
    }

    pub fn set_layout_mode(&mut self, mode: LayoutMode) -> bool {
        if self.layout_mode == mode {
            self.center();
            return false;
        }
        self.layout_mode = mode;
        self.rebuild_layout();
        self.center();
        true
    }

    pub fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    pub fn center(&mut self) {
        self.camera_x = 0;
        self.camera_y = 0;
    }

    pub fn camera(&self) -> (i32, i32) {
        (self.camera_x, self.camera_y)
    }

    /// World-space bounds of everything that can contribute pixels/cells to the
    /// graph: visible labels/tombstones/placeholders plus cached Braille edges.
    /// The hidden structural mount root is deliberately excluded.
    pub fn content_bounds(&self) -> Option<WorldBounds> {
        let mut bounds: Option<WorldBounds> = None;

        let mut include = |x0: i32, x1: i32, y0: i32, y1: i32| {
            bounds = Some(match bounds {
                Some(mut b) => {
                    b.min_x = b.min_x.min(x0);
                    b.max_x = b.max_x.max(x1);
                    b.min_y = b.min_y.min(y0);
                    b.max_y = b.max_y.max(y1);
                    b
                }
                None => WorldBounds {
                    min_x: x0,
                    max_x: x1,
                    min_y: y0,
                    max_y: y1,
                },
            });
        };

        for node in &self.nodes {
            if node.id == 0 || node.hidden {
                continue;
            }
            let Some(pos) = self.positions.get(node.id).copied() else {
                continue;
            };
            let width = self.node_width(node.id).max(1) as i32;
            include(pos.x, pos.x + width - 1, pos.y, pos.y);
        }

        for cell in &self.edge_cells {
            include(cell.x, cell.x, cell.y, cell.y);
        }

        bounds
    }

    /// Camera limits derived from the current content rectangle and viewport.
    ///
    /// The user may deliberately overscroll away from the graph, but only by
    /// half of the currently visible graph viewport on each axis. This keeps
    /// navigation forgiving without allowing the camera to disappear into
    /// effectively unbounded empty space. Camera zero is always valid.
    pub fn camera_limits(&self, viewport: Viewport) -> ((i32, i32), (i32, i32)) {
        let Some(bounds) = self.content_bounds() else {
            return ((0, 0), (0, 0));
        };
        let (origin_x, origin_y) = self.screen_origin(viewport);

        // Baseline: content can reach the viewport edge. Then extend that legal
        // range by exactly half the visible width/height as soft overscroll.
        let left_aligned = viewport.x as i32 - origin_x - bounds.min_x;
        let right_aligned = viewport.right() as i32 - 1 - origin_x - bounds.max_x;
        let top_aligned = viewport.y as i32 - origin_y - bounds.min_y;
        let bottom_aligned = viewport.bottom() as i32 - 1 - origin_y - bounds.max_y;

        let overscroll_x = viewport.width as i32 / 2;
        let overscroll_y = viewport.height as i32 / 2;

        let min_x = right_aligned.saturating_sub(overscroll_x).min(0);
        let max_x = left_aligned.saturating_add(overscroll_x).max(0);
        let min_y = bottom_aligned.saturating_sub(overscroll_y).min(0);
        let max_y = top_aligned.saturating_add(overscroll_y).max(0);
        ((min_x, max_x), (min_y, max_y))
    }

    pub fn set_camera_clamped(&mut self, viewport: Viewport, x: i32, y: i32) -> bool {
        let ((min_x, max_x), (min_y, max_y)) = self.camera_limits(viewport);
        let next_x = x.clamp(min_x, max_x);
        let next_y = y.clamp(min_y, max_y);
        let changed = (next_x, next_y) != (self.camera_x, self.camera_y);
        self.camera_x = next_x;
        self.camera_y = next_y;
        changed
    }

    pub fn clamp_camera(&mut self, viewport: Viewport) -> bool {
        self.set_camera_clamped(viewport, self.camera_x, self.camera_y)
    }

    pub fn pan_clamped(&mut self, viewport: Viewport, dx: i32, dy: i32) -> bool {
        self.set_camera_clamped(
            viewport,
            self.camera_x.saturating_add(dx),
            self.camera_y.saturating_add(dy),
        )
    }

    /// Place a world-space point at the center of the current viewport, then
    /// clamp the result to the same content rectangle used by normal panning.
    pub fn jump_to_world(&mut self, viewport: Viewport, world_x: i32, world_y: i32) -> bool {
        let (origin_x, origin_y) = self.screen_origin(viewport);
        let center_x = viewport.x as i32 + viewport.width as i32 / 2;
        let center_y = viewport.y as i32 + viewport.height as i32 / 2;
        self.set_camera_clamped(
            viewport,
            center_x - origin_x - world_x,
            center_y - origin_y - world_y,
        )
    }

    pub fn line_style(&self) -> LineStyle {
        self.line_style
    }

    pub fn cycle_line_style(&mut self) -> LineStyle {
        self.line_style = self.line_style.next();
        // Connector style is intentionally independent from filesystem scanning
        // and node placement. Only the sparse Braille edge cache changes.
        self.rebuild_edge_cache();
        self.line_style
    }

    pub fn depth_limit(&self) -> usize {
        self.depth_limit
    }

    pub fn cycle_depth(&mut self) -> io::Result<usize> {
        let current = DEPTH_LEVELS
            .iter()
            .position(|value| *value == self.depth_limit)
            .unwrap_or_else(|| {
                DEPTH_LEVELS
                    .iter()
                    .position(|value| *value == DEFAULT_DEPTH_LIMIT)
                    .unwrap_or(0)
            });
        self.depth_limit = DEPTH_LEVELS[(current + 1) % DEPTH_LEVELS.len()];
        self.reload()?;
        Ok(self.depth_limit)
    }

    pub fn cycle_spacing(&mut self) -> i32 {
        self.column_gap += 4;
        if self.column_gap > 30 {
            self.column_gap = 14;
        }
        self.rebuild_layout();
        self.column_gap
    }

    pub fn spacing_level(&self) -> usize {
        match self.column_gap {
            14 => 0,
            18 => 1,
            22 => 2,
            26 => 3,
            30 => 4,
            value if value < 18 => 0,
            value if value < 22 => 1,
            value if value < 26 => 2,
            value if value < 30 => 3,
            _ => 4,
        }
    }

    pub fn toggle_layout_mode(&mut self) -> LayoutMode {
        let next = match self.layout_mode {
            LayoutMode::Tree => LayoutMode::Radial,
            LayoutMode::Radial => LayoutMode::Tree,
        };
        self.set_layout_mode(next);
        self.layout_mode
    }

    pub fn root_label(&self) -> String {
        self.root.display().to_string()
    }

    pub fn root_path(&self) -> &Path {
        &self.root
    }

    /// Revision for world-space geometry/content used by passive overlays such
    /// as the minimap. Camera movement deliberately does not change it.
    pub fn scene_revision(&self) -> u64 {
        self.scene_revision
    }

    /// Sparse world-space samples for the minimap. This intentionally ignores
    /// the current camera and line-style raster. Nodes plus a few parent-edge
    /// samples are enough to preserve the graph's overall silhouette/density.
    pub fn minimap_samples(&self) -> Vec<(i32, i32)> {
        let mut samples = Vec::with_capacity(self.nodes.len().saturating_mul(4));

        for node in &self.nodes {
            if node.id == 0 || node.hidden || node.is_removed {
                continue;
            }
            let Some(pos) = self.positions.get(node.id).copied() else {
                continue;
            };
            let center = (pos.x + self.node_width(node.id) as i32 / 2, pos.y);
            samples.push(center);

            let Some(parent_id) = node.parent else {
                continue;
            };
            if parent_id == 0 {
                continue;
            }
            if self.layout_mode == LayoutMode::Tree
                && self
                    .suppressed_parent_edges
                    .get(node.id)
                    .copied()
                    .unwrap_or(false)
            {
                continue;
            }
            let Some(parent) = self.node(parent_id) else {
                continue;
            };
            if parent.hidden || parent.is_removed {
                continue;
            }
            let Some(parent_pos) = self.positions.get(parent_id).copied() else {
                continue;
            };
            let parent_center = (
                parent_pos.x + self.node_width(parent_id) as i32 / 2,
                parent_pos.y,
            );

            // Three cheap interpolation samples suggest hierarchy without
            // copying the expensive/high-resolution Braille connector raster.
            for step in 1..=3 {
                let x = parent_center.0 + (center.0 - parent_center.0) * step / 4;
                let y = parent_center.1 + (center.1 - parent_center.1) * step / 4;
                samples.push((x, y));
            }
        }

        samples
    }

    fn bump_scene_revision(&mut self) {
        self.scene_revision = self.scene_revision.wrapping_add(1);
    }

    pub fn selection_stats(&self, id: usize) -> Option<SelectionStats> {
        let node = self.node(id)?;
        if node.id == 0 || node.is_placeholder || node.is_removed || node.hidden {
            return None;
        }

        let metadata = fs::metadata(&node.path).ok();
        let size = metadata
            .as_ref()
            .map(|meta| human_bytes(meta.len()))
            .unwrap_or_else(|| "?".to_string());
        let modified = metadata
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .map(relative_age)
            .unwrap_or_else(|| "?".to_string());
        let access = metadata
            .as_ref()
            .map(|meta| {
                if meta.permissions().readonly() {
                    "read only"
                } else {
                    "read/write"
                }
            })
            .unwrap_or("?")
            .to_string();

        Some(SelectionStats {
            key: format!("#{}", node.id),
            kind: if node.is_dir { "folder" } else { "file" }.to_string(),
            size,
            modified,
            access,
        })
    }

    pub fn node(&self, id: usize) -> Option<&FsNode> {
        self.nodes.get(id)
    }

    pub fn label(&self, id: usize) -> String {
        self.node(id)
            .map(|n| {
                if n.is_placeholder {
                    "...".to_string()
                } else if n.is_dir {
                    format!("{}/", n.name.trim_end_matches('/'))
                } else {
                    n.name.clone()
                }
            })
            .unwrap_or_else(|| "?".to_string())
    }

    fn visual_label(&self, id: usize) -> String {
        self.node(id)
            .map(|n| {
                if n.hidden {
                    String::new()
                } else if n.is_removed {
                    "🪦 removed".to_string()
                } else if n.is_placeholder {
                    "...".to_string()
                } else {
                    let icon = if n.is_dir { "🖿" } else { "🖹" };
                    format!("{icon} {}", n.name)
                }
            })
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn hit_test(&self, viewport: Viewport, x: u16, y: u16) -> Option<usize> {
        if !viewport.contains(x, y) {
            return None;
        }

        for node in self.nodes.iter().rev() {
            if node.id == 0 || node.is_placeholder || node.is_removed || node.hidden {
                continue;
            }
            let Some((sx, sy)) = self.screen_pos(node.id, viewport) else {
                continue;
            };
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
        if !viewport.contains(x, y) {
            return None;
        }

        // Folder labels remain visually unchanged, but their drop target grows
        // by one terminal cell on every side. The tree layout reserves extra Y
        // room between folder siblings so these virtual hit areas stay usable.
        for node in self.nodes.iter().rev() {
            if node.id == 0
                || node.is_placeholder
                || node.is_removed
                || node.hidden
                || !node.is_dir
                || !self.can_move(source, node.id)
            {
                continue;
            }
            let Some((sx, sy)) = self.screen_pos(node.id, viewport) else {
                continue;
            };
            let width = self.node_width(node.id) as i32;
            let px = x as i32;
            let py = y as i32;
            if px >= sx - 1 && px <= sx + width && py >= sy - 1 && py <= sy + 1 {
                return Some(node.id);
            }
        }
        None
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
        if src.is_placeholder
            || dst.is_placeholder
            || src.is_removed
            || dst.is_removed
            || src.hidden
            || dst.hidden
            || !dst.is_dir
            || src.parent == Some(target)
        {
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

    pub fn create_folder(&mut self, parent: usize, name: &str) -> io::Result<String> {
        validate_name(name)?;
        let parent_path = if parent == 0 {
            self.root.clone()
        } else {
            let node = self.node(parent).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "parent folder disappeared")
            })?;
            if node.is_placeholder || !node.is_dir {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "parent is not a folder",
                ));
            }
            node.path.clone()
        };
        let destination = parent_path.join(name);
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "name already exists",
            ));
        }
        fs::create_dir(&destination)?;
        let message = format!("NEW · 🖿 {name}");
        self.reload()?;
        Ok(message)
    }

    pub fn create_file(&mut self, parent: usize, name: &str) -> io::Result<String> {
        validate_name(name)?;
        let parent_path = if parent == 0 {
            self.root.clone()
        } else {
            let node = self.node(parent).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "parent folder disappeared")
            })?;
            if node.is_placeholder || node.is_removed || node.hidden || !node.is_dir {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "parent is not a folder",
                ));
            }
            node.path.clone()
        };
        let destination = parent_path.join(name);
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "name already exists",
            ));
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let message = format!("NEW · 🖹 {name}");
        self.reload()?;
        Ok(message)
    }

    pub fn rename_node(&mut self, source: usize, name: &str) -> io::Result<String> {
        validate_name(name)?;
        if source == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot rename mount root",
            ));
        }
        let node = self
            .node(source)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source node disappeared"))?;
        if node.is_placeholder {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot rename placeholder",
            ));
        }
        let parent = node
            .path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no parent"))?;
        let destination = parent.join(name);
        if destination == node.path {
            return Ok(format!("NAME · {}", node.name));
        }
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "name already exists",
            ));
        }
        fs::rename(&node.path, &destination)?;
        let message = format!("NAME · {} → {name}", node.name);
        self.reload()?;
        Ok(message)
    }

    pub fn move_node(&mut self, source: usize, target: usize) -> io::Result<String> {
        if !self.can_move(source, target) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid folder move",
            ));
        }

        let src = self
            .node(source)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source node disappeared"))?;
        let dst = self
            .node(target)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "target folder disappeared"))?;
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
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot trash root",
            ));
        }

        let src = self
            .node(source)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source node disappeared"))?;
        if src.is_placeholder {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot trash placeholder",
            ));
        }

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

        // Keep the current world layout stable after deletion. The deleted node
        // remains as a non-interactive tombstone until the next explicit reload,
        // mount/parent change, or depth refresh. Descendants of a removed folder
        // are hidden immediately because they no longer exist at their old paths.
        if let Some(node) = self.nodes.get_mut(source) {
            node.is_removed = true;
        }
        self.hide_descendants(source);

        // Positions are intentionally untouched. Only connector geometry changes:
        // edges inside a removed folder vanish, while its parent→tombstone edge may
        // remain. This is much cheaper than rescanning and laying out the tree.
        self.rebuild_edge_cache();
        self.bump_scene_revision();

        let message = format!("TRASH · {} → .explorer-trash/", src.name);
        Ok(message)
    }

    fn hide_descendants(&mut self, ancestor: usize) {
        let mut stack = vec![ancestor];
        while let Some(parent) = stack.pop() {
            let children: Vec<usize> = self
                .nodes
                .iter()
                .filter(|node| node.parent == Some(parent))
                .map(|node| node.id)
                .collect();
            for child in children {
                if let Some(node) = self.nodes.get_mut(child) {
                    node.hidden = true;
                }
                stack.push(child);
            }
        }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        viewport: Viewport,
        selected: Option<usize>,
        drag_id: Option<usize>,
        drop_target: Option<usize>,
    ) {
        // Connector geometry is cached in world space whenever the directory,
        // layout mode, or spacing changes. Panning and resize only translate and
        // clip these cached Braille cells.
        self.render_edges(frame, viewport);

        for node in &self.nodes {
            if node.id == 0 || node.hidden {
                continue;
            }
            let Some((sx, sy)) = self.screen_pos(node.id, viewport) else {
                continue;
            };
            if sy < viewport.y as i32 || sy >= viewport.bottom() as i32 {
                continue;
            }

            let label = self.visual_label(node.id);
            let mut style = Style::new(Color::Grey, Color::Reset);

            if node.is_removed {
                style = Style::new(Color::DarkGrey, Color::Reset);
            } else if node.is_placeholder {
                // Truncation must be visually loud enough to read as an actual
                // node in both tree and radial views.
                style = Style::new(Color::Black, Color::White);
            } else if node.is_dir {
                style = Style::new(Color::Black, Color::White);
            }
            if !node.is_removed && selected == Some(node.id) {
                style = Style::new(Color::Black, Color::DarkYellow);
            }
            if !node.is_removed && drop_target == Some(node.id) {
                style = Style::new(Color::Black, Color::Cyan);
            }
            if !node.is_removed && drag_id == Some(node.id) {
                style = Style::new(Color::DarkGrey, Color::Reset);
            }

            self.print_clipped(frame, viewport, sx, sy, &label, style);
        }
    }

    fn render_edges(&self, frame: &mut Frame, viewport: Viewport) {
        let (origin_x, origin_y) = self.screen_origin(viewport);
        let offset_x = origin_x + self.camera_x;
        let offset_y = origin_y + self.camera_y;
        let style = Style::new(Color::DarkGrey, Color::Reset);

        // edge_cells is sorted by (world_y, world_x). Convert the visible screen
        // band back to world rows and binary-search directly into the cache. A pan
        // therefore walks only connector cells that can actually reach the current
        // viewport instead of rescanning the complete graph.
        let world_top = viewport.y as i32 - offset_y;
        let world_bottom = viewport.bottom().saturating_sub(1) as i32 - offset_y;
        let first = self.edge_cells.partition_point(|cell| cell.y < world_top);

        for cell in &self.edge_cells[first..] {
            if cell.y > world_bottom {
                break;
            }
            let screen_x = offset_x + cell.x;
            if screen_x < viewport.x as i32 || screen_x >= viewport.right() as i32 {
                continue;
            }
            let screen_y = offset_y + cell.y;
            let ch = char::from_u32(0x2800 + cell.mask as u32).unwrap_or(' ');
            frame.put_i32(screen_x, screen_y, ch, style);
        }
    }

    pub fn screen_y(&self, id: usize, viewport: Viewport) -> Option<u16> {
        let (_, y) = self.screen_pos(id, viewport)?;
        if y < viewport.y as i32 || y >= viewport.bottom() as i32 {
            return None;
        }
        Some(y as u16)
    }

    fn screen_pos(&self, id: usize, viewport: Viewport) -> Option<(i32, i32)> {
        let p = *self.positions.get(id)?;
        let (origin_x, origin_y) = self.screen_origin(viewport);
        Some((
            origin_x + p.x + self.camera_x,
            origin_y + p.y + self.camera_y,
        ))
    }

    fn screen_origin(&self, viewport: Viewport) -> (i32, i32) {
        match self.layout_mode {
            LayoutMode::Tree => (
                viewport.x as i32 + 2,
                viewport.y as i32 + (viewport.height as i32 / 2),
            ),
            LayoutMode::Radial => (
                viewport.x as i32 + (viewport.width as i32 / 2),
                viewport.y as i32 + (viewport.height as i32 / 2),
            ),
        }
    }

    fn node_width(&self, id: usize) -> usize {
        self.node(id)
            .map(|node| {
                if node.hidden {
                    0
                } else if node.is_removed {
                    text_cell_width("🪦 removed")
                } else if node.is_placeholder {
                    3
                } else {
                    2 + node.name.chars().count()
                }
            })
            .unwrap_or(1)
            .max(1)
    }

    fn print_clipped(
        &self,
        frame: &mut Frame,
        viewport: Viewport,
        x: i32,
        y: i32,
        text: &str,
        style: Style,
    ) {
        if y < viewport.y as i32 || y >= viewport.bottom() as i32 {
            return;
        }

        let mut screen_x = x;
        for ch in text.chars() {
            let glyph_width = terminal_cell_width(ch) as i32;
            let glyph_right = screen_x + glyph_width;

            // Do not paint half of a wide glyph at the viewport boundary.
            if screen_x >= viewport.x as i32 && glyph_right <= viewport.right() as i32 {
                frame.put_display_i32(screen_x, y, ch, style);
            }

            screen_x = glyph_right;
            if screen_x >= viewport.right() as i32 {
                break;
            }
        }
    }
}

fn validate_name(name: &str) -> io::Result<()> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid file/folder name",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

fn edge_anchor(
    from: (i32, i32),
    from_width: usize,
    toward: (i32, i32),
    toward_width: usize,
) -> Point {
    let fx = from.0 as f64 + from_width as f64 * 0.5;
    let fy = from.1 as f64 + 0.5;
    let tx = toward.0 as f64 + toward_width as f64 * 0.5;
    let ty = toward.1 as f64 + 0.5;
    let dx = tx - fx;
    let dy = ty - fy;

    if dx.abs() > dy.abs() * 1.7 {
        Point {
            x: if dx > 0.0 {
                from.0 as f64 + from_width as f64 + 0.15
            } else {
                from.0 as f64 - 0.15
            },
            y: fy,
        }
    } else {
        Point {
            x: fx,
            y: if dy > 0.0 {
                from.1 as f64 + 1.05
            } else {
                from.1 as f64 - 0.05
            },
        }
    }
}

/// Sparse world-space Braille mask cache. Each terminal cell owns a 2x4 dot
/// grid. It is rebuilt only when the filesystem or layout changes; camera motion
/// simply translates and clips the cached cells.
struct WorldBrailleCanvas {
    masks: HashMap<(i32, i32), u8>,
}

impl WorldBrailleCanvas {
    fn new() -> Self {
        Self {
            masks: HashMap::new(),
        }
    }

    fn into_cells(self) -> Vec<EdgeCell> {
        let mut cells: Vec<EdgeCell> = self
            .masks
            .into_iter()
            .filter_map(|((x, y), mask)| (mask != 0).then_some(EdgeCell { x, y, mask }))
            .collect();
        cells.sort_unstable_by_key(|cell| (cell.y, cell.x));
        cells
    }

    fn to_dot(point: Point) -> Point {
        Point {
            x: point.x * 2.0,
            y: point.y * 4.0,
        }
    }

    fn set_dot(&mut self, x: f64, y: f64) {
        let x = x.round() as i32;
        let y = y.round() as i32;
        let cell_x = x.div_euclid(2);
        let cell_y = y.div_euclid(4);
        let local_x = x.rem_euclid(2) as usize;
        let local_y = y.rem_euclid(4) as usize;
        let bit_table = [[0_u8, 3_u8], [1, 4], [2, 5], [6, 7]];
        let bit = bit_table[local_y][local_x];
        *self.masks.entry((cell_x, cell_y)).or_insert(0) |= 1 << bit;
    }

    fn draw_segment(&mut self, a: Point, b: Point) {
        let mut x0 = a.x.round() as i32;
        let mut y0 = a.y.round() as i32;
        let x1 = b.x.round() as i32;
        let y1 = b.y.round() as i32;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.set_dot(x0 as f64, y0 as f64);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = err * 2;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn sample_cubic(&mut self, a: Point, c1: Point, c2: Point, b: Point) {
        let distance = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let steps = ((distance / 1.9).ceil() as usize).clamp(10, 512);
        let mut previous = a;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mt = 1.0 - t;
            let point = Point {
                x: mt * mt * mt * a.x
                    + 3.0 * mt * mt * t * c1.x
                    + 3.0 * mt * t * t * c2.x
                    + t * t * t * b.x,
                y: mt * mt * mt * a.y
                    + 3.0 * mt * mt * t * c1.y
                    + 3.0 * mt * t * t * c2.y
                    + t * t * t * b.y,
            };
            self.draw_segment(previous, point);
            previous = point;
        }
    }

    fn sample_quadratic(&mut self, a: Point, c: Point, b: Point) {
        let distance = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let steps = ((distance / 2.1).ceil() as usize).clamp(8, 512);
        let mut previous = a;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mt = 1.0 - t;
            let point = Point {
                x: mt * mt * a.x + 2.0 * mt * t * c.x + t * t * b.x,
                y: mt * mt * a.y + 2.0 * mt * t * c.y + t * t * b.y,
            };
            self.draw_segment(previous, point);
            previous = point;
        }
    }

    fn draw_straight(&mut self, a: Point, b: Point) {
        self.draw_segment(Self::to_dot(a), Self::to_dot(b));
    }

    fn draw_elbow(&mut self, a: Point, b: Point) {
        let a = Self::to_dot(a);
        let b = Self::to_dot(b);
        let dx = b.x - a.x;
        let dy = b.y - a.y;

        if dx.abs() >= dy.abs() {
            let middle_x = ((a.x + b.x) * 0.5).round();
            let p1 = Point {
                x: middle_x,
                y: a.y,
            };
            let p2 = Point {
                x: middle_x,
                y: b.y,
            };
            self.draw_segment(a, p1);
            self.draw_segment(p1, p2);
            self.draw_segment(p2, b);
        } else {
            let middle_y = ((a.y + b.y) * 0.5).round();
            let p1 = Point {
                x: a.x,
                y: middle_y,
            };
            let p2 = Point {
                x: b.x,
                y: middle_y,
            };
            self.draw_segment(a, p1);
            self.draw_segment(p1, p2);
            self.draw_segment(p2, b);
        }
    }

    fn draw_soft_arc(&mut self, a: Point, b: Point) {
        let a = Self::to_dot(a);
        let b = Self::to_dot(b);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let distance = (dx * dx + dy * dy).sqrt().max(1.0);
        let bend = (distance * 0.15).clamp(4.0, 18.0);
        let nx = -dy / distance;
        let ny = dx / distance;
        let sign = if a.x + b.x < 0.0 { -1.0 } else { 1.0 };
        let control = Point {
            x: (a.x + b.x) * 0.5 + nx * bend * sign,
            y: (a.y + b.y) * 0.5 + ny * bend * sign,
        };
        self.sample_quadratic(a, control, b);
    }

    fn draw_s_curve(&mut self, a: Point, b: Point) {
        let a = Self::to_dot(a);
        let b = Self::to_dot(b);
        let dx = b.x - a.x;
        let dy = b.y - a.y;

        if dx.abs() >= dy.abs() {
            self.sample_cubic(
                a,
                Point {
                    x: a.x + dx * 0.42,
                    y: a.y,
                },
                Point {
                    x: b.x - dx * 0.42,
                    y: b.y,
                },
                b,
            );
        } else {
            self.sample_cubic(
                a,
                Point {
                    x: a.x,
                    y: a.y + dy * 0.42,
                },
                Point {
                    x: b.x,
                    y: b.y - dy * 0.42,
                },
                b,
            );
        }
    }

    fn draw_radial_spline(&mut self, a: Point, b: Point) {
        let a = Self::to_dot(a);
        let b = Self::to_dot(b);
        let al = (a.x.powi(2) + a.y.powi(2)).sqrt().max(1.0);
        let bl = (b.x.powi(2) + b.y.powi(2)).sqrt().max(1.0);
        let au = Point {
            x: a.x / al,
            y: a.y / al,
        };
        let bu = Point {
            x: b.x / bl,
            y: b.y / bl,
        };
        let distance = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let reach = (distance * 0.36).max(5.0);

        self.sample_cubic(
            a,
            Point {
                x: a.x + au.x * reach,
                y: a.y + au.y * reach,
            },
            Point {
                x: b.x - bu.x * reach,
                y: b.y - bu.y * reach,
            },
            b,
        );
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn relative_age(when: SystemTime) -> String {
    let elapsed = SystemTime::now()
        .duration_since(when)
        .unwrap_or(Duration::ZERO);
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}
