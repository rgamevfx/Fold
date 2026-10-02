//! Quiet file-explorer presentation. Selection, search and expansion are not project edits.
#[cfg(test)]
#[path = "project_panel_tests.rs"]
mod tests;
#[path = "project_thumbnails.rs"]
mod thumbnails;
use fold_foundation::{BinId, Time};
use fold_platform::{
    browser::{BrowserCommand as Command, Entry},
    desktop::{DesktopClient, DesktopCommand, ViewLocation},
};
use fold_project::{ItemId, Parent, Snapshot};
use fold_ui::sdk::{
    ExtensionUi, Panel, PanelRegistry,
    imgui::{self, Key, MouseButton, Ui},
    project_drop,
    toolbar::{ToolbarIcon, icon_button},
};
use std::collections::BTreeSet;

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    registry.register("fold.app", ProjectPanel::default())
}
#[derive(Clone)]
struct Row {
    entry: Entry,
    name: String,
    depth: usize,
}
#[derive(Default)]
pub struct ProjectPanel {
    folder: Parent,
    tree: bool,
    search: String,
    expanded: BTreeSet<BinId>,
    selected: Option<Entry>,
    rename: Option<Entry>,
    new_bin: Option<Parent>,
    name: String,
    open_name: bool,
    rect: Option<([f32; 2], [f32; 2])>,
    external_drag: Option<[f32; 2]>,
    bin_rects: Vec<(Parent, [f32; 2], [f32; 2])>,
    thumbnails: thumbnails::Thumbnails,
}
fn send(host: &mut dyn DesktopClient, command: Command) {
    host.command(DesktopCommand::Browser(command));
}
fn contains(p: [f32; 2], min: [f32; 2], max: [f32; 2]) -> bool {
    p[0] >= min[0] && p[1] >= min[1] && p[0] < max[0] && p[1] < max[1]
}
impl ProjectPanel {
    fn rows(&self, snapshot: &Snapshot) -> Vec<Row> {
        let org = &snapshot.state().organization;
        let query = self.search.trim().to_lowercase();
        if !query.is_empty() {
            let mut rows: Vec<_> = org
                .bins
                .values()
                .map(|b| Row {
                    entry: Entry::Bin(b.id),
                    name: b.name.clone(),
                    depth: 0,
                })
                .chain(org.items.iter().map(|i| Row {
                    entry: Entry::Item(i.id),
                    name: i.name.clone(),
                    depth: 0,
                }))
                .filter(|r| r.name.to_lowercase().contains(&query))
                .collect();
            rows.sort_by_key(|r| {
                (
                    matches!(r.entry, Entry::Item(_)),
                    r.name.to_lowercase(),
                    r.entry,
                )
            });
            return rows;
        }
        let mut children = std::collections::BTreeMap::<Parent, Vec<Row>>::new();
        for bin in org.bins.values() {
            children.entry(bin.parent).or_default().push(Row {
                entry: Entry::Bin(bin.id),
                name: bin.name.clone(),
                depth: 0,
            });
        }
        for item in &org.items {
            children.entry(item.parent).or_default().push(Row {
                entry: Entry::Item(item.id),
                name: item.name.clone(),
                depth: 0,
            });
        }
        for rows in children.values_mut() {
            rows.sort_by_key(|r| {
                (
                    matches!(r.entry, Entry::Item(_)),
                    r.name.to_lowercase(),
                    r.entry,
                )
            });
        }
        let root = if self.tree { Parent::Root } else { self.folder };
        let mut pending: Vec<_> = children
            .remove(&root)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .collect();
        let mut rows = Vec::new();
        // Iterative preorder preserves folder adjacency without recursion or repeated scans.
        while let Some(row) = pending.pop() {
            if self.tree
                && let Entry::Bin(id) = row.entry
                && self.expanded.contains(&id)
            {
                pending.extend(
                    children
                        .remove(&Parent::Bin(id))
                        .unwrap_or_default()
                        .into_iter()
                        .rev()
                        .map(|mut child| {
                            child.depth = row.depth + 1;
                            child
                        }),
                );
            }
            rows.push(row);
        }
        rows
    }
    fn toggle_expansion(&mut self, id: BinId) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }
    fn open(&mut self, row: &Row, host: &mut dyn DesktopClient) {
        match row.entry {
            Entry::Bin(id) => {
                if self.tree {
                    self.toggle_expansion(id);
                } else {
                    self.folder = Parent::Bin(id);
                    self.search.clear();
                }
            }
            Entry::Item(ItemId::Document(document)) => {
                host.command(DesktopCommand::Navigate(ViewLocation {
                    document,
                    time: Time::ZERO,
                    label: row.name.clone(),
                }))
            }
            Entry::Item(ItemId::Asset(_)) => {}
        }
    }
    fn destination(&self, row: Option<&Row>, snapshot: &Snapshot) -> Parent {
        row.and_then(|r| match r.entry {
            Entry::Bin(id) => Some(Parent::Bin(id)),
            Entry::Item(id) => snapshot
                .state()
                .organization
                .items
                .iter()
                .find(|i| i.id == id)
                .map(|i| i.parent),
        })
        .unwrap_or(self.folder)
    }
    fn context_menu(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        snapshot: &Snapshot,
        row: Option<&Row>,
    ) {
        let parent = self.destination(row, snapshot);
        if ui.menu_item_with_shortcut("Import…", "Ctrl+I") {
            send(host, Command::ChooseImport(parent));
        }
        if ui.menu_item("New bin…") {
            self.new_bin = Some(parent);
            self.rename = None;
            self.name = "New bin".into();
            self.open_name = true;
        }
        if let Some(_menu) = ui.begin_menu("New") {
            for kind in host.document_kinds() {
                if ui.menu_item(kind.title) {
                    send(
                        host,
                        Command::Create {
                            kind: kind.type_id.into(),
                            parent,
                        },
                    );
                }
            }
        }
        if let Some(row) = row {
            ui.separator();
            if matches!(row.entry, Entry::Bin(_) | Entry::Item(ItemId::Document(_)))
                && ui.menu_item_with_shortcut("Open", "Enter")
            {
                self.open(row, host);
            }
            if matches!(row.entry, Entry::Item(_))
                && let Some(_menu) = ui.begin_menu("Place into")
            {
                for item in &snapshot.state().organization.items {
                    if let ItemId::Document(id) = item.id {
                        let document = &snapshot.state().documents[&id];
                        let _id = ui.push_id(&format!("{id:?}"));
                        if ui.menu_item_enabled_selected_no_shortcut(
                            &item.name,
                            false,
                            host.supports_document(document) && row.entry != Entry::Item(item.id),
                        ) {
                            let at = if host.state().viewer_document == Some(id) {
                                Time::new(
                                    i64::from(host.state().frame) * i64::from(host.state().rate[1]),
                                    host.state().rate[0],
                                )
                                .unwrap_or(Time::ZERO)
                            } else {
                                Time::ZERO
                            };
                            project_drop::place(host, row.entry, id, None, at);
                        }
                    }
                }
            }
            if ui.menu_item_with_shortcut("Rename…", "F2") {
                self.rename = Some(row.entry);
                self.new_bin = None;
                self.name = row.name.clone();
                self.open_name = true;
            }
            if let Some(_menu) = ui.begin_menu("Move to") {
                if ui.menu_item("Project") {
                    send(
                        host,
                        Command::Move {
                            entry: row.entry,
                            parent: Parent::Root,
                        },
                    );
                }
                for bin in snapshot.state().organization.bins.values() {
                    let _id = ui.push_id(&format!("{:?}", bin.id));
                    if ui.menu_item(&bin.name) {
                        send(
                            host,
                            Command::Move {
                                entry: row.entry,
                                parent: Parent::Bin(bin.id),
                            },
                        );
                    }
                }
            }
            if let Entry::Item(ItemId::Asset(id)) = row.entry
                && ui.menu_item("Relink…")
            {
                send(host, Command::Relink(id));
            }
            if ui.menu_item_with_shortcut("Delete", "Delete") {
                send(host, Command::Delete(row.entry));
            }
        }
    }
    fn row(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        snapshot: &Snapshot,
        row: &Row,
        width: f32,
        grid: bool,
    ) {
        let _id = ui.push_id(&format!("{:?}", row.entry));
        let origin = ui.cursor_screen_pos();
        let height = if grid {
            width * 0.5625 + ui.text_line_height() + 12.
        } else {
            ui.text_line_height() + 8.
        };
        if ui
            .selectable_config("##item")
            .selected(self.selected == Some(row.entry))
            .size([width, height])
            .build()
        {
            self.selected = Some(row.entry);
        }
        let hovered = ui.is_item_hovered();
        let focused = ui.is_item_focused();
        if hovered && ui.is_mouse_double_clicked(MouseButton::Left) {
            self.open(row, host);
        }
        if focused {
            self.selected = Some(row.entry);
        }
        project_drop::source(ui, row.entry, &row.name);
        if let Entry::Bin(id) = row.entry {
            let max = [origin[0] + width, origin[1] + height];
            self.bin_rects.push((Parent::Bin(id), origin, max));
            if let Some(entry) = project_drop::target(ui) {
                send(
                    host,
                    Command::Move {
                        entry,
                        parent: Parent::Bin(id),
                    },
                );
            }
            if self.tree
                && hovered
                && ui.is_mouse_clicked(MouseButton::Left)
                && ui.mouse_pos()[0] < origin[0] + row.depth as f32 * 14. + 23.
            {
                self.toggle_expansion(id);
            }
        }
        if let Some(_popup) = ui.begin_popup_context_item() {
            self.selected = Some(row.entry);
            self.context_menu(ui, host, snapshot, Some(row));
        }
        let mut warning = None;
        let (kind, texture) = match row.entry {
            Entry::Bin(_) => ("Folder", None),
            Entry::Item(ItemId::Document(id)) => {
                let document = &snapshot.state().documents[&id];
                if !host.supports_document(document) {
                    warning = Some("Provider unavailable".into());
                }
                (
                    host.document_kinds()
                        .into_iter()
                        .find(|k| k.type_id == document.type_id)
                        .map(|k| k.title)
                        .unwrap_or("Document"),
                    None,
                )
            }
            Entry::Item(ItemId::Asset(id)) => {
                let asset = &snapshot.state().assets[&id];
                let texture = match self.thumbnails.get(asset) {
                    Some(Ok(id)) => Some(id),
                    Some(Err(error)) => {
                        warning = Some(error);
                        None
                    }
                    None => None,
                };
                ("Media", texture)
            }
        };
        let draw = ui.get_window_draw_list();
        let text = ui.style_color(imgui::StyleColor::Text);
        let muted = ui.style_color(imgui::StyleColor::TextDisabled);
        let indent = if grid { 0. } else { row.depth as f32 * 14. };
        if grid {
            let min = [origin[0] + 4., origin[1] + 4.];
            let max = [
                origin[0] + width - 4.,
                origin[1] + height - ui.text_line_height() - 8.,
            ];
            if let Some(texture) = texture {
                draw.add_image(texture, min, max, [0., 0.], [1., 1.], [1.; 4]);
            } else if matches!(row.entry, Entry::Bin(_)) {
                let center = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
                draw.add_rect(
                    [center[0] - 25., center[1] - 16.],
                    [center[0] - 3., center[1] - 7.],
                    muted,
                )
                .filled(true)
                .rounding(2.)
                .build();
                draw.add_rect(
                    [center[0] - 25., center[1] - 10.],
                    [center[0] + 25., center[1] + 18.],
                    muted,
                )
                .filled(true)
                .rounding(2.)
                .build();
            } else {
                draw.add_rect(min, max, ui.style_color(imgui::StyleColor::FrameBg))
                    .filled(true)
                    .build();
                draw.add_text([min[0] + 8., min[1] + 12.], muted, kind);
            }
        } else {
            let symbol = match row.entry {
                Entry::Bin(id) => {
                    if self.expanded.contains(&id) {
                        "v"
                    } else {
                        ">"
                    }
                }
                Entry::Item(ItemId::Document(_)) => match kind {
                    "Sequence" => "S",
                    "Composition" => "C",
                    "Motion" => "G",
                    _ => "?",
                },
                _ => "M",
            };
            draw.add_text([origin[0] + indent + 4., origin[1] + 4.], muted, symbol);
        }
        let label_position = [
            origin[0] + if grid { 4. } else { indent + 24. },
            origin[1]
                + if grid {
                    height - ui.text_line_height() - 4.
                } else {
                    4.
                },
        ];
        let max_chars = ((width - (label_position[0] - origin[0]) - 10.)
            / (ui.current_font_size() * 0.55))
            .max(1.) as usize;
        let label = if row.name.chars().count() > max_chars {
            format!(
                "{}…",
                row.name
                    .chars()
                    .take(max_chars.saturating_sub(1))
                    .collect::<String>()
            )
        } else {
            row.name.clone()
        };
        draw.add_text(label_position, text, label);
        if let Some(error) = &warning {
            draw.add_text(
                [origin[0] + width - 12., origin[1] + 4.],
                [1., 0.55, 0.25, 1.],
                "!",
            );
            if hovered {
                ui.tooltip_text(format!("{}\n{error}", row.name));
            }
        } else if hovered {
            ui.tooltip_text(format!("{}\n{kind}", row.name));
        }
    }
}
impl Panel for ProjectPanel {
    fn id(&self) -> &'static str {
        "fold.app.project"
    }
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.external_drag = position;
    }
    fn prepare_frame(&mut self, context: &mut imgui::Context) {
        self.thumbnails.prepare(context);
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) -> bool {
        if !self
            .rect
            .is_some_and(|(min, max)| contains(position, min, max))
        {
            return false;
        }
        let destination = self
            .bin_rects
            .iter()
            .rev()
            .find(|(_, min, max)| contains(position, *min, *max))
            .map(|(p, _, _)| *p)
            .unwrap_or(self.folder);
        send(
            host,
            Command::Import {
                paths: paths.to_vec(),
                destination,
            },
        );
        true
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let org = &snapshot.state().organization;
        if let Some(id) = host.take_imported_items().first().copied()
            && let Some(item) = org.items.iter().find(|i| i.id == id)
        {
            self.selected = Some(Entry::Item(id));
            self.search.clear();
            if !self.tree {
                self.folder = item.parent;
            }
            let mut parent = item.parent;
            while let Parent::Bin(bin) = parent {
                self.expanded.insert(bin);
                parent = org.bins[&bin].parent;
            }
        }
        if let Parent::Bin(id) = self.folder
            && !org.bins.contains_key(&id)
        {
            self.folder = Parent::Root;
        }
        self.expanded.retain(|id| org.bins.contains_key(id));
        self.bin_rects.clear();
        let origin = ui.cursor_screen_pos();
        let available = ui.content_region_avail();
        self.rect = Some((origin, [origin[0] + available[0], origin[1] + available[1]]));
        if icon_button(ui, "up", ToolbarIcon::Back, "Parent folder (Alt+Up)") {
            self.folder = match self.folder {
                Parent::Root => Parent::Root,
                Parent::Bin(id) => org.bins[&id].parent,
            };
        }
        ui.same_line();
        let title = match self.folder {
            Parent::Root => "Project",
            Parent::Bin(id) => &org.bins[&id].name,
        };
        let characters =
            ((available[0] - 180.) / (ui.current_font_size() * 0.55)).clamp(1., 14.) as usize;
        let short: String = title.chars().take(characters).collect();
        if ui.button(format!("{short}##folder")) {
            ui.open_popup("folders");
        }
        if let Some(_popup) = ui.begin_popup("folders") {
            if ui.menu_item("Project") {
                self.folder = Parent::Root;
            }
            let mut parent = self.folder;
            while let Parent::Bin(id) = parent {
                let bin = &org.bins[&id];
                if ui.menu_item(&bin.name) {
                    self.folder = parent;
                }
                parent = bin.parent;
            }
        }
        ui.same_line();
        if icon_button(
            ui,
            "view",
            if self.tree {
                ToolbarIcon::Grid
            } else {
                ToolbarIcon::List
            },
            if self.tree {
                "Thumbnail view"
            } else {
                "List view"
            },
        ) {
            self.tree = !self.tree;
            if self.tree {
                self.folder = Parent::Root;
            }
        }
        ui.same_line();
        ui.set_next_item_width(ui.content_region_avail()[0].max(30.));
        ui.input_text("##search", &mut self.search)
            .hint("Search")
            .build();
        let rows = self.rows(&snapshot);
        let keyboard = ui.is_window_focused_with_flags(imgui::FocusedFlags::ROOT_AND_CHILD_WINDOWS)
            && !ui.io().want_text_input();
        let selected = rows
            .iter()
            .find(|r| Some(r.entry) == self.selected)
            .cloned();
        if keyboard {
            if ui.io().key_ctrl() && ui.is_key_pressed(Key::I) {
                send(
                    host,
                    Command::ChooseImport(self.destination(selected.as_ref(), &snapshot)),
                );
            }
            if ui.io().key_shift() && ui.is_key_pressed(Key::F10) {
                ui.open_popup("Selection actions");
            }
            if let Some(row) = &selected {
                if ui.is_key_pressed(Key::Enter) {
                    self.open(row, host);
                }
                if ui.is_key_pressed(Key::Delete) {
                    send(host, Command::Delete(row.entry));
                }
                if ui.is_key_pressed(Key::F2) {
                    self.rename = Some(row.entry);
                    self.new_bin = None;
                    self.name = row.name.clone();
                    self.open_name = true;
                }
                if let Entry::Bin(id) = row.entry {
                    if ui.is_key_pressed(Key::RightArrow) {
                        self.expanded.insert(id);
                    }
                    if ui.is_key_pressed(Key::LeftArrow) {
                        self.expanded.remove(&id);
                    }
                }
            }
            if ui.io().key_alt() && ui.is_key_pressed(Key::UpArrow) {
                self.folder = match self.folder {
                    Parent::Root => Parent::Root,
                    Parent::Bin(id) => org.bins[&id].parent,
                };
            }
        }
        if let Some(_popup) = ui.begin_popup("Selection actions") {
            self.context_menu(ui, host, &snapshot, selected.as_ref());
        }
        if host.state().busy {
            ui.text_disabled("Import / background work…");
            ui.same_line();
            if ui.small_button("Cancel") {
                host.command(DesktopCommand::Cancel);
            }
        } else if !host.state().status.is_empty()
            && ![
                "Edit committed",
                "Project saved",
                "Project opened",
                "Import committed",
            ]
            .contains(&host.state().status.as_str())
        {
            ui.text_wrapped(&host.state().status);
        }
        ui.child_window("project-items")
            .size([0., 0.])
            .build(ui, || {
                let grid = !self.tree;
                let avail = ui.content_region_avail()[0].max(1.);
                let columns = if grid {
                    ((avail + 8.) / 140.).floor().max(1.) as usize
                } else {
                    1
                };
                let width =
                    ((avail - (columns.saturating_sub(1)) as f32 * 8.) / columns as f32).max(1.);
                let height = if grid {
                    width * 0.5625 + ui.text_line_height() + 12.
                } else {
                    ui.text_line_height() + 8.
                };
                let mut clipper = imgui::ListClipper::new(rows.len().div_ceil(columns))
                    .items_height(height + ui.clone_style().item_spacing()[1])
                    .begin(ui);
                while clipper.step() {
                    for line in clipper.display_start()..clipper.display_end() {
                        for column in 0..columns {
                            if let Some(row) = rows.get(line * columns + column) {
                                if column > 0 {
                                    ui.same_line();
                                }
                                self.row(ui, host, &snapshot, row, width, grid);
                            }
                        }
                    }
                }
                let empty_start = ui.cursor_screen_pos();
                let empty_size = ui.content_region_avail();
                ui.invisible_button("##empty", [empty_size[0].max(1.), empty_size[1].max(1.)]);
                if let Some(entry) = project_drop::target(ui) {
                    send(
                        host,
                        Command::Move {
                            entry,
                            parent: self.folder,
                        },
                    );
                }
                if let Some(_popup) = ui.begin_popup_context_item() {
                    self.context_menu(ui, host, &snapshot, None);
                }
                if rows.is_empty() {
                    ui.get_window_draw_list().add_text(
                        [empty_start[0] + 6., empty_start[1] + 8.],
                        ui.style_color(imgui::StyleColor::TextDisabled),
                        if self.search.is_empty() {
                            "Empty bin"
                        } else {
                            "No matches"
                        },
                    );
                }
            });
        if let Some(position) = self.external_drag
            && let Some((min, max)) = self
                .rect
                .filter(|(min, max)| contains(position, *min, *max))
        {
            let (min, max) = self
                .bin_rects
                .iter()
                .rev()
                .find(|(_, min, max)| contains(position, *min, *max))
                .map(|(_, a, b)| (*a, *b))
                .unwrap_or((min, max));
            ui.get_window_draw_list()
                .add_rect(min, max, ui.style_color(imgui::StyleColor::DragDropTarget))
                .thickness(2.)
                .build();
        }
        if self.open_name {
            ui.open_popup("Name");
            self.open_name = false;
        }
        if let Some(_popup) = ui.begin_popup("Name") {
            if ui.is_window_appearing() {
                ui.set_keyboard_focus_here();
            }
            let enter = ui
                .input_text("##name", &mut self.name)
                .enter_returns_true(true)
                .build();
            if enter || ui.button("OK") {
                if let Some(entry) = self.rename.take() {
                    send(
                        host,
                        Command::Rename {
                            entry,
                            name: self.name.trim().into(),
                        },
                    );
                } else if let Some(parent) = self.new_bin.take() {
                    send(
                        host,
                        Command::NewBin {
                            parent,
                            name: self.name.trim().into(),
                        },
                    );
                }
                ui.close_current_popup();
            }
            ui.same_line();
            if ui.button("Cancel") {
                ui.close_current_popup();
            }
        }
    }
}
