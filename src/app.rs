// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

mod theme;

use ectd_rust::{
    dossier::{Dossier, relative},
    export::{ExportReport, export_consolidated},
    tree::{HEADERS, NodeKind, Tree, ViewOptions},
};
use eframe::egui::{self, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
};

pub const APP_TITLE: &str = concat!("eCTD Viewer v", env!("CARGO_PKG_VERSION"), " (Rust)");

#[derive(Default, PartialEq, Clone, Copy)]
enum Tab {
    #[default]
    Overview,
    Metadata,
    Xml,
    Versions,
}

#[derive(Default)]
struct Details {
    title: String,
    overview: Vec<(String, String)>,
    metadata: Vec<(String, String)>,
    warnings: Vec<String>,
    xml: String,
    versions: Vec<Version>,
    path: Option<PathBuf>,
    predecessor: Option<PathBuf>,
}

struct Version {
    depth: usize,
    sequence: String,
    operation: String,
    path: Option<PathBuf>,
}
enum Job {
    Loaded(Arc<Dossier>),
    Exported(ExportReport),
    Cancelled,
}
enum Action {
    Select(usize),
    Toggle(usize),
    Open(PathBuf),
    Copy(String),
}

pub struct ViewerApp {
    dossier: Option<Arc<Dossier>>,
    tree: Option<Tree>,
    selected: Option<usize>,
    options: ViewOptions,
    search: String,
    path_input: String,
    columns: [bool; 12],
    tab: Tab,
    details: Details,
    job: Option<Receiver<Result<Job, String>>>,
    busy: String,
    message: Option<(String, String)>,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, root: Option<PathBuf>) -> Self {
        theme::configure(&cc.egui_ctx);
        let mut app = Self::empty();
        if let Some(root) = root {
            app.load(root, &cc.egui_ctx);
        }
        app
    }

    fn empty() -> Self {
        Self {
            dossier: None,
            tree: None,
            selected: None,
            options: ViewOptions::default(),
            search: String::new(),
            path_input: String::new(),
            columns: [
                true, false, true, true, true, false, false, false, false, false, false, false,
            ],
            tab: Tab::default(),
            details: Details::default(),
            job: None,
            busy: String::new(),
            message: None,
        }
    }

    fn background(
        &mut self,
        ctx: &egui::Context,
        text: &str,
        work: impl FnOnce() -> Result<Job, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.job = Some(rx);
        self.busy = text.into();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err("Background task failed unexpectedly.".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn load(&mut self, root: PathBuf, ctx: &egui::Context) {
        self.path_input = root.display().to_string();
        self.background(ctx, "Loading dossier…", move || {
            Dossier::load(&root).map(|d| Job::Loaded(Arc::new(d)))
        });
    }

    fn poll(&mut self) {
        let Some(rx) = &self.job else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Background task disconnected.".into()),
        };
        self.job = None;
        self.busy.clear();
        match result {
            Ok(Job::Loaded(dossier)) => {
                self.tree = Some(Tree::new(&dossier, self.options));
                self.path_input = dossier.root.display().to_string();
                self.dossier = Some(dossier);
                self.selected = Some(0);
                self.search.clear();
                self.update_details();
            }
            Ok(Job::Exported(report)) => {
                let title = if report.missing > 0 || !report.errors.is_empty() {
                    "Export incomplete"
                } else {
                    "Export completed"
                };
                self.message = Some((title.into(), report.summary()));
            }
            Ok(Job::Cancelled) => {}
            Err(e) => self.message = Some(("Error".into(), e)),
        }
    }

    fn selected_sequence(&self) -> Option<String> {
        self.tree
            .as_ref()?
            .nodes
            .get(self.selected?)?
            .sequence
            .clone()
    }

    fn rebuild(&mut self) {
        let Some(dossier) = &self.dossier else {
            return;
        };
        let selected_seq = self.selected_sequence();
        let selected_node = self.tree.as_ref().and_then(|tree| {
            let node = tree.nodes.get(self.selected?)?;
            Some((
                node.kind.clone(),
                node.sequence.clone(),
                node.columns[0].clone(),
                node.columns[1].clone(),
            ))
        });
        let tree = self
            .tree
            .get_or_insert_with(|| Tree::new(dossier, self.options));
        tree.set_options(dossier, self.options);
        self.selected = selected_node
            .and_then(|(kind, sequence, name, node_type)| {
                tree.nodes.iter().position(|node| {
                    node.kind == kind
                        && node.sequence == sequence
                        && (kind != NodeKind::Group
                            || (node.columns[0] == name && node.columns[1] == node_type))
                })
            })
            .or_else(|| selected_seq.and_then(|seq| tree.sequence_nodes.get(&seq).copied()))
            .or(Some(0));
        self.update_details();
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        self.background(
            ctx,
            "Select dossier directory…",
            || match rfd::FileDialog::new()
                .set_title("Select eCTD dossier directory")
                .pick_folder()
            {
                Some(root) => Dossier::load(&root).map(|d| Job::Loaded(Arc::new(d))),
                None => Ok(Job::Cancelled),
            },
        );
    }

    fn export_dialog(&mut self, ctx: &egui::Context) {
        let (Some(dossier), Some(seq)) = (self.dossier.clone(), self.selected_sequence()) else {
            return;
        };
        self.background(ctx, "Export consolidated documents…", move || {
            let Some(parent) = rfd::FileDialog::new()
                .set_title("Select parent folder for consolidated export")
                .pick_folder()
            else {
                return Ok(Job::Cancelled);
            };
            let destination = parent.join(format!("{seq}-consolidated"));
            export_consolidated(&dossier, &seq, &destination).map(Job::Exported)
        });
    }

    fn controls(&mut self, ctx: &egui::Context) {
        let busy = self.job.is_some();
        let sequence = self.selected_sequence();
        let name = self.dossier.as_ref().map_or_else(
            || "No dossier selected".to_owned(),
            |dossier| {
                dossier
                    .root
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        );
        egui::TopBottomPanel::top("controls")
            .frame(egui::Frame::new().fill(theme::SURFACE).inner_margin(egui::Margin::symmetric(18, 12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let title_width = (ui.available_width() - 370.0).max(180.0);
                    ui.allocate_ui_with_layout(egui::vec2(title_width, 48.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        egui::Frame::new().fill(theme::ACCENT_SOFT).corner_radius(9).inner_margin(10)
                            .show(ui, |ui| theme::icon(ui, theme::Icon::Folder, theme::ACCENT, 24.0));
                        ui.vertical(|ui| {
                            ui.set_max_width((title_width - 60.0).max(100.0));
                            theme::eyebrow(ui, "DOSSIER WORKSPACE");
                            ui.add(egui::Label::new(RichText::new(&name).size(20.0).strong()).truncate()).on_hover_text(&name);
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!busy && sequence.is_some(), egui::Button::new("Export Consolidated…").min_size(egui::vec2(0.0, 32.0)))
                            .on_hover_text("Copy active documents for the selected sequence to a new or empty folder.").clicked() {
                            self.export_dialog(ctx);
                        }
                        ui.add_enabled_ui(!busy && self.dossier.is_some(), |ui| {
                            if theme::icon_button(ui, theme::Icon::Reload, "Reload dossier").clicked() {
                                self.load(self.dossier.as_ref().unwrap().root.clone(), ctx);
                            }
                        });
                        if ui.add_enabled(!busy, theme::primary("Open Dossier…")).clicked() { self.open_dialog(ctx); }
                    });
                });
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Location").size(11.0).color(theme::MUTED));
                    let width = (ui.available_width() - 66.0).max(120.0);
                    let input = ui.add_sized([width, 28.0], egui::TextEdit::singleline(&mut self.path_input)
                        .font(egui::FontId::monospace(11.5)).hint_text("Paste a dossier folder path…"));
                    let enter = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let load = ui.add_enabled(!busy && !self.path_input.trim().is_empty(), egui::Button::new("Load")).clicked();
                    if (load || enter) && !busy && !self.path_input.trim().is_empty() {
                        self.load(PathBuf::from(self.path_input.trim()), ctx);
                    }
                });
            });
        if self.dossier.is_none() {
            return;
        }
        let mut changed = false;
        egui::TopBottomPanel::top("view_options")
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(18, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(7)
                        .inner_margin(3)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if theme::choice(ui, !self.options.toc, "Files").clicked() {
                                    changed |= self.options.toc;
                                    self.options.toc = false;
                                }
                                if theme::choice(ui, self.options.toc, "CTD-TOC").clicked() {
                                    changed |= !self.options.toc;
                                    self.options.toc = true;
                                }
                            });
                        });
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(7)
                        .inner_margin(3)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if theme::choice(ui, !self.options.consolidated, "Sequence changes")
                                    .clicked()
                                {
                                    changed |= self.options.consolidated;
                                    self.options.consolidated = false;
                                }
                                if theme::choice(ui, self.options.consolidated, "Consolidated")
                                    .clicked()
                                {
                                    changed |= !self.options.consolidated;
                                    self.options.consolidated = true;
                                }
                            });
                        });
                    ui.menu_button("Display", |ui| {
                        changed |= ui
                            .checkbox(
                                &mut self.options.show_backbones,
                                "Show backbone XMLs in Documents",
                            )
                            .changed();
                        ui.separator();
                        theme::eyebrow(ui, "VISIBLE COLUMNS");
                        for (i, heading) in HEADERS.iter().enumerate().skip(1) {
                            if i == 11 && self.options.toc {
                                continue;
                            }
                            ui.checkbox(
                                &mut self.columns[i],
                                if i == 7 && self.options.toc {
                                    "Filename"
                                } else {
                                    heading
                                },
                            );
                        }
                    });
                    if ui.button("Expand")
                        .on_hover_text("Expand the selected sequence. Select the dossier root to expand all sequences.")
                        .clicked()
                        && let (Some(tree), Some(dossier)) = (&mut self.tree, &self.dossier)
                    {
                        if let Some(seq) = sequence.as_deref() {
                            tree.expand_sequence(dossier, seq);
                        } else {
                            for seq in &dossier.sequences {
                                tree.expand_sequence(dossier, &seq.name);
                            }
                        }
                    }
                    if ui.button("Collapse").on_hover_text("Collapse all branches to the sequence list and clear the search.").clicked() {
                        if let Some(tree) = &mut self.tree {
                            tree.collapse_to_sequences();
                        }
                        self.search.clear();
                    }
                    ui.add_space(5.0);
                    theme::icon(ui, theme::Icon::Search, theme::MUTED, 15.0);
                    let search_max_width = if ctx.content_rect().width() > 1100.0 {
                        310.0
                    } else {
                        230.0
                    };
                    let search_width = (ui.available_rect_before_wrap().width() - 44.0)
                        .clamp(140.0, search_max_width);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .id(egui::Id::new("document_search"))
                            .hint_text("Search documents…")
                            .desired_width(search_width)
                            .margin(egui::Margin::symmetric(10, 7)),
                    )
                    .on_hover_text(
                        "Search all columns and sequences, including collapsed sequences.",
                    );
                    if !self.search.is_empty()
                        && ui.small_button("×").on_hover_text("Clear search").clicked()
                    {
                        self.search.clear();
                    }
                });
            });
        if changed {
            self.rebuild();
        }
    }

    fn actions(&mut self, actions: Vec<Action>, ctx: &egui::Context) {
        for action in actions {
            match action {
                Action::Select(id) => {
                    self.selected = Some(id);
                    self.update_details();
                }
                Action::Toggle(id) => {
                    if let (Some(tree), Some(dossier)) = (&mut self.tree, &self.dossier)
                        && !tree.expanded.remove(&id)
                    {
                        if let NodeKind::Sequence(seq) = tree.nodes[id].kind.clone() {
                            tree.populate(dossier, &seq);
                        }
                        tree.expanded.insert(id);
                    }
                }
                Action::Open(path) => {
                    if !path.exists() {
                        self.message = Some(("File not found".into(), path.display().to_string()));
                    } else if let Err(e) = open::that(&path) {
                        self.message = Some(("Could not open file".into(), e.to_string()));
                    }
                }
                Action::Copy(text) => ctx.copy_text(text),
            }
        }
    }

    fn update_details(&mut self) {
        self.details = Details::default();
        let (Some(dossier), Some(tree), Some(id)) = (&self.dossier, &self.tree, self.selected)
        else {
            return;
        };
        let node = &tree.nodes[id];
        let details = &mut self.details;
        details.title = node.columns[0].clone();
        match &node.kind {
            NodeKind::Leaf(id) => {
                let leaf = &dossier.leaves[*id];
                details.title = dossier.title(*id);
                details.path = dossier.open_path(*id).map(Path::to_owned);
                details.predecessor = leaf
                    .predecessor
                    .and_then(|prev| dossier.open_path(prev))
                    .map(Path::to_owned);
                details.overview = vec![
                    ("Sequence".into(), leaf.sequence.clone()),
                    ("Operation".into(), leaf.operation.clone()),
                    (
                        "Status".into(),
                        dossier.status(*id, self.options.consolidated),
                    ),
                    (
                        "Backbone".into(),
                        dossier.backbones[leaf.backbone].path.display().to_string(),
                    ),
                    ("Leaf ID".into(), leaf.id.clone()),
                    ("Title".into(), leaf.title.clone()),
                    ("Href".into(), leaf.href.clone()),
                    ("modified-file".into(), leaf.modified_file.clone()),
                    (
                        "File (opens)".into(),
                        details
                            .path
                            .as_ref()
                            .map_or(String::new(), |p| p.display().to_string()),
                    ),
                    (
                        "Exists".into(),
                        details
                            .path
                            .as_ref()
                            .is_some_and(|p| p.is_file())
                            .to_string(),
                    ),
                    ("CTD-TOC".into(), leaf.toc.join(" / ")),
                ];
                details.metadata = dossier.metadata(*id).into_iter().collect();
                details.warnings = leaf.warnings.clone();
                details.xml = leaf.xml.clone();
                let (versions, cycle) = dossier.versions(*id);
                details.versions = versions
                    .into_iter()
                    .enumerate()
                    .rev()
                    .map(|(depth, id)| {
                        let leaf = &dossier.leaves[id];
                        Version {
                            depth,
                            sequence: leaf.sequence.clone(),
                            operation: leaf.operation.clone(),
                            path: dossier.open_path(id).map(Path::to_owned),
                        }
                    })
                    .collect();
                if cycle && !details.warnings.iter().any(|s| s.contains("Cycle")) {
                    details
                        .warnings
                        .push("Cycle in modified-file chain.".into());
                }
            }
            NodeKind::Backbone(id) => {
                let bb = &dossier.backbones[*id];
                details.path = Some(bb.path.clone());
                details.overview = vec![
                    ("Sequence".into(), bb.sequence.clone()),
                    ("Path".into(), bb.path.display().to_string()),
                    ("Kind".into(), bb.kind.clone()),
                    ("Region".into(), bb.region.clone().unwrap_or_default()),
                    ("Root tag".into(), bb.root_tag.clone()),
                    ("Leaf count".into(), bb.leaf_count.to_string()),
                    (
                        "Parse status".into(),
                        bb.parse_error.clone().unwrap_or_else(|| "OK".into()),
                    ),
                ];
                for (prefix, uri) in &bb.namespaces {
                    details
                        .overview
                        .push((format!("xmlns:{prefix}"), uri.clone()));
                }
                details.metadata = details.overview.clone();
                details.xml = std::fs::read(&bb.path)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_else(|e| format!("Could not read XML: {e}"));
                if let Some(e) = &bb.parse_error {
                    details.warnings.push(e.clone());
                }
            }
            NodeKind::Sequence(seq) => {
                if let Some(seq) = dossier.sequence(seq) {
                    details.overview = vec![
                        ("Sequence".into(), seq.name.clone()),
                        ("Delta leaves".into(), seq.delta.len().to_string()),
                        ("Active leaves".into(), seq.consolidated.len().to_string()),
                    ];
                    for &id in &seq.backbones {
                        let bb = &dossier.backbones[id];
                        details.overview.push((
                            relative(&bb.path, &dossier.root),
                            format!(
                                "{} leaves — {}",
                                bb.leaf_count,
                                bb.parse_error.as_deref().unwrap_or("OK")
                            ),
                        ));
                    }
                    if seq.backbones.is_empty() {
                        details.warnings.push("No backbone XMLs found.".into());
                    }
                }
            }
            NodeKind::Dossier => {
                details.overview = vec![
                    ("Path".into(), dossier.root.display().to_string()),
                    ("Sequences".into(), dossier.sequences.len().to_string()),
                    ("Backbones".into(), dossier.backbones.len().to_string()),
                    ("Leaves".into(), dossier.leaves.len().to_string()),
                ];
                details.warnings = dossier.warnings.clone();
            }
            NodeKind::Orphan(path) => {
                details.path = Some(path.clone());
                details.overview = vec![
                    ("Status".into(), "ORPHAN (not referenced in XML)".into()),
                    ("Path".into(), path.display().to_string()),
                ];
            }
            NodeKind::Group => {}
        }
    }

    fn detail_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        theme::eyebrow(ui, "INSPECTOR");
        ui.add_space(3.0);
        ui.add(egui::Label::new(RichText::new(&self.details.title).size(18.0).strong()).wrap());
        ui.horizontal_wrapped(|ui| {
            for key in ["Sequence", "Operation", "Status"] {
                if let Some((_, value)) = self.details.overview.iter().find(|(name, _)| name == key)
                {
                    if key == "Sequence" {
                        theme::badge(ui, &format!("SEQ {value}"), theme::MUTED, theme::BACKGROUND);
                    } else {
                        theme::status(ui, value);
                    }
                }
            }
        });
        ui.add_space(5.0);
        if self.details.path.is_some() || self.details.predecessor.is_some() {
            ui.horizontal_wrapped(|ui| {
                let path = self.details.path.as_ref();
                if ui
                    .add_enabled(path.is_some_and(|p| p.exists()), theme::primary("Open"))
                    .clicked()
                {
                    actions.push(Action::Open(path.unwrap().clone()));
                }
                if ui
                    .add_enabled(
                        self.details
                            .predecessor
                            .as_ref()
                            .is_some_and(|p| p.is_file()),
                        egui::Button::new("Open Predecessor"),
                    )
                    .clicked()
                {
                    actions.push(Action::Open(self.details.predecessor.clone().unwrap()));
                }
                ui.menu_button("More", |ui| {
                    if ui
                        .add_enabled(path.is_some(), egui::Button::new("Show in File Manager"))
                        .clicked()
                    {
                        if let Some(parent) = path.and_then(|p| {
                            if p.is_dir() {
                                Some(p.as_path())
                            } else {
                                p.parent()
                            }
                        }) {
                            actions.push(Action::Open(parent.into()));
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(path.is_some(), egui::Button::new("Copy Path"))
                        .clicked()
                    {
                        actions.push(Action::Copy(path.unwrap().display().to_string()));
                        ui.close();
                    }
                });
            });
        }
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(theme::BACKGROUND)
            .corner_radius(7)
            .inner_margin(3)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (tab, name) in [
                        (Tab::Overview, "Overview"),
                        (Tab::Metadata, "Metadata"),
                        (Tab::Xml, "XML"),
                        (Tab::Versions, "Versions"),
                    ] {
                        if theme::choice(ui, self.tab == tab, name).clicked() {
                            self.tab = tab;
                        }
                    }
                });
            });
        ui.add_space(12.0);
        match self.tab {
            Tab::Overview => {
                egui::ScrollArea::vertical()
                    .id_salt("overview")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (key, value) in &self.details.overview {
                            ui.label(RichText::new(key).size(11.0).strong().color(theme::MUTED));
                            if key == "Operation" || key == "Status" {
                                theme::status(ui, value);
                            } else {
                                let text = if matches!(
                                    key.as_str(),
                                    "Backbone"
                                        | "Leaf ID"
                                        | "Href"
                                        | "modified-file"
                                        | "File (opens)"
                                        | "Path"
                                ) {
                                    RichText::new(value).font(egui::FontId::monospace(11.5))
                                } else {
                                    RichText::new(value)
                                };
                                ui.add(egui::Label::new(text).selectable(true).wrap());
                            }
                            ui.add_space(9.0);
                        }
                        if !self.details.warnings.is_empty() {
                            warning_card(ui, &self.details.warnings);
                        }
                    });
            }
            Tab::Metadata => {
                if self.details.metadata.is_empty() {
                    ui.label(RichText::new("No metadata for this selection.").color(theme::MUTED));
                } else {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    TableBuilder::new(ui)
                        .id_salt("metadata")
                        .min_scrolled_height(80.0)
                        .striped(true)
                        .resizable(true)
                        .column(Column::initial(130.0).at_least(85.0))
                        .column(Column::remainder().at_least(90.0))
                        .auto_shrink([false, false])
                        .header(28.0, |mut header| {
                            for name in ["KEY", "VALUE"] {
                                header.col(|ui| {
                                    theme::eyebrow(ui, name);
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(24.0, self.details.metadata.len(), |mut row| {
                                let (key, value) = &self.details.metadata[row.index()];
                                row.col(|ui| {
                                    copy_label(ui, key, actions);
                                });
                                row.col(|ui| {
                                    copy_label(ui, value, actions);
                                });
                            });
                        });
                }
            }
            Tab::Xml => {
                if self.details.xml.is_empty() {
                    ui.label(
                        RichText::new("Select a document or backbone to inspect its XML.")
                            .color(theme::MUTED),
                    );
                } else {
                    ui.horizontal(|ui| {
                        theme::eyebrow(ui, "XML SOURCE");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Copy XML").clicked() {
                                actions.push(Action::Copy(self.details.xml.clone()));
                            }
                        });
                    });
                    egui::Frame::new()
                        .fill(theme::BACKGROUND)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(7)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            egui::ScrollArea::both()
                                .id_salt("xml")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let mut text = self.details.xml.as_str();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text)
                                            .font(egui::FontId::monospace(11.5))
                                            .code_editor()
                                            .frame(false)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        });
                }
            }
            Tab::Versions => {
                ui.label(
                    RichText::new("Document history · oldest first")
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.add_space(5.0);
                egui::ScrollArea::vertical()
                    .id_salt("versions_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.details.versions.is_empty() {
                            ui.label(
                                RichText::new("No version history for this selection.")
                                    .color(theme::MUTED),
                            );
                        }
                        for version in &self.details.versions {
                            let card = egui::Frame::new()
                                .fill(theme::SURFACE)
                                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                                .corner_radius(7)
                                .inner_margin(12)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal_wrapped(|ui| {
                                        theme::icon(ui, theme::Icon::Document, theme::MUTED, 16.0);
                                        ui.label(
                                            RichText::new(format!("Sequence {}", version.sequence))
                                                .strong(),
                                        );
                                        theme::status(ui, &version.operation);
                                    });
                                    if let Some(path) = &version.path {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(
                                                    path.file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy(),
                                                )
                                                .size(12.0),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(path.display().to_string());
                                    }
                                    ui.horizontal(|ui| {
                                        let exists =
                                            version.path.as_ref().is_some_and(|p| p.is_file());
                                        ui.label(
                                            RichText::new(format!(
                                                "Depth {} · {}",
                                                version.depth,
                                                if exists {
                                                    "File available"
                                                } else {
                                                    "File unavailable"
                                                }
                                            ))
                                            .size(10.5)
                                            .color(theme::MUTED),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add_enabled(
                                                        exists,
                                                        egui::Button::new("Open version").small(),
                                                    )
                                                    .clicked()
                                                {
                                                    actions.push(Action::Open(
                                                        version.path.clone().unwrap(),
                                                    ));
                                                }
                                            },
                                        );
                                    });
                                });
                            if card.response.interact(Sense::click()).double_clicked()
                                && let Some(path) = &version.path
                            {
                                actions.push(Action::Open(path.clone()));
                            }
                            ui.add_space(7.0);
                        }
                        let cycles: Vec<_> = self
                            .details
                            .warnings
                            .iter()
                            .filter(|warning| warning.contains("Cycle"))
                            .cloned()
                            .collect();
                        if !cycles.is_empty() {
                            warning_card(ui, &cycles);
                        }
                    });
            }
        }
    }
}

impl ViewerApp {
    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                    .inner_margin(egui::Margin::symmetric(18, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.job.is_some() {
                        ui.spinner();
                        ui.label(RichText::new(&self.busy).size(11.5).color(theme::ACCENT));
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(9.0, 9.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 3.0, theme::GREEN);
                        let summary = self.dossier.as_ref().map_or_else(
                            || "Ready to open a dossier".to_owned(),
                            |dossier| {
                                format!(
                                    "{} sequences   ·   {} backbones   ·   {} XML entries",
                                    dossier.sequences.len(),
                                    dossier.backbones.len(),
                                    dossier.leaves.len()
                                )
                            },
                        );
                        ui.label(RichText::new(summary).size(11.5).color(theme::MUTED));
                    }
                    if let Some(dossier) = &self.dossier {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let missing = dossier.leaves.iter().filter(|leaf| leaf.missing).count();
                            let errors = dossier
                                .backbones
                                .iter()
                                .filter(|backbone| backbone.parse_error.is_some())
                                .count();
                            let (color, fill) = if missing + errors == 0 {
                                (theme::GREEN, theme::GREEN_SOFT)
                            } else {
                                (theme::RED, theme::RED_SOFT)
                            };
                            theme::badge(
                                ui,
                                &format!("{missing} missing · {errors} XML errors"),
                                color,
                                fill,
                            );
                            if !dossier.warnings.is_empty() {
                                theme::badge(
                                    ui,
                                    &format!("{} warnings", dossier.warnings.len()),
                                    theme::AMBER,
                                    theme::AMBER_SOFT,
                                );
                            }
                        });
                    }
                });
            });
    }

    fn render(&mut self, ctx: &egui::Context) {
        self.poll();
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped
            && self.job.is_none()
        {
            self.load(path, ctx);
        }
        self.controls(ctx);
        if let (Some(tree), Some(dossier)) = (&mut self.tree, &self.dossier) {
            tree.search(dossier, &self.search);
        }
        self.status_bar(ctx);
        let mut actions = vec![];
        if self.dossier.is_some() {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(390.0)
                .min_width(300.0)
                .max_width((ctx.content_rect().width() * 0.45).clamp(300.0, 620.0))
                .frame(
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .inner_margin(16)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE)),
                )
                .show(ctx, |ui| self.detail_panel(ui, &mut actions));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16))
            .show(ctx, |ui| {
                if let (Some(tree), Some(dossier)) = (&self.tree, &self.dossier) {
                    egui::Frame::new().fill(theme::SURFACE).stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(9).inner_margin(14).show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Documents").size(17.0).strong());
                                theme::badge(ui, &format!("{} items", tree.visible().len()), theme::MUTED, theme::BACKGROUND);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if let Some(sequence) = self.selected_sequence() {
                                        theme::badge(ui, &format!("SEQ {sequence}"), theme::ACCENT, theme::ACCENT_SOFT);
                                    }
                                });
                            });
                            ui.label(RichText::new("Double-click to open · Right-click for document actions").size(11.0).color(theme::MUTED));
                            ui.add_space(9.0);
                            render_tree(ui, tree, dossier, self.selected, &self.columns, &mut actions);
                        });
                } else {
                    let available = ui.available_size();
                    ui.vertical_centered(|ui| {
                        ui.add_space(((available.y - 370.0) * 0.42).max(15.0));
                        egui::Frame::new().fill(theme::SURFACE).stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                            .corner_radius(12).inner_margin(30).show(ui, |ui| {
                                ui.set_width((available.x - 70.0).clamp(250.0, 460.0));
                                ui.vertical_centered(|ui| {
                                    egui::Frame::new().fill(theme::ACCENT_SOFT).corner_radius(16).inner_margin(17)
                                        .show(ui, |ui| theme::icon(ui, theme::Icon::Folder, theme::ACCENT, 40.0));
                                    ui.add_space(15.0);
                                    ui.label(RichText::new("Explore your dossier").size(25.0).strong());
                                    ui.add_space(3.0);
                                    ui.add(egui::Label::new(RichText::new("Browse sequences, follow document history and inspect XML metadata in one workspace.")
                                        .size(14.0).color(theme::MUTED)).wrap());
                                    ui.add_space(16.0);
                                    if ui.add_enabled(self.job.is_none(), theme::primary("Open Dossier…").min_size(egui::vec2(180.0, 38.0))).clicked() {
                                        self.open_dialog(ctx);
                                    }
                                    ui.add_space(6.0);
                                    ui.label(RichText::new("or drop a dossier folder into this window").size(11.5).color(theme::MUTED));
                                    ui.add_space(15.0);
                                    ui.label(RichText::new("SEQUENCES   ·   DOCUMENTS   ·   XML METADATA").size(10.0).color(theme::MUTED));
                                });
                            });
                    });
                }
            });
        self.actions(actions, ctx);
        if let Some((title, text)) = &self.message {
            let mut close = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(true)
                .default_width(520.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(450.0)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(text).selectable(true).wrap());
                        });
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(theme::primary("OK")).clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.message = None;
            }
        }
    }
}

fn warning_card(ui: &mut egui::Ui, warnings: &[String]) {
    egui::Frame::new()
        .fill(theme::AMBER_SOFT)
        .corner_radius(7)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Needs attention")
                    .strong()
                    .color(theme::AMBER),
            );
            for warning in warnings {
                ui.add(
                    egui::Label::new(RichText::new(warning).size(12.0).color(theme::AMBER)).wrap(),
                );
            }
        });
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
}

fn copy_label(ui: &mut egui::Ui, text: &str, actions: &mut Vec<Action>) {
    ui.add(egui::Label::new(text).truncate().selectable(true))
        .on_hover_text(text)
        .context_menu(|ui| {
            if ui.button("Copy").clicked() {
                actions.push(Action::Copy(text.into()));
                ui.close();
            }
        });
}

fn node_path(kind: &NodeKind, dossier: &Dossier) -> Option<PathBuf> {
    match kind {
        NodeKind::Leaf(id) => dossier.open_path(*id).map(Path::to_owned),
        NodeKind::Backbone(id) => Some(dossier.backbones[*id].path.clone()),
        NodeKind::Orphan(path) => Some(path.clone()),
        _ => None,
    }
}

fn render_tree(
    ui: &mut egui::Ui,
    tree: &Tree,
    dossier: &Dossier,
    selected: Option<usize>,
    columns: &[bool; 12],
    actions: &mut Vec<Action>,
) {
    let rows = tree.visible();
    if rows.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(55.0);
            theme::icon(ui, theme::Icon::Search, theme::MUTED, 32.0);
            ui.add_space(10.0);
            ui.label(RichText::new("No matching documents").size(16.0).strong());
            ui.label(
                RichText::new("Try a different search or clear the search field.")
                    .color(theme::MUTED),
            );
        });
        return;
    }
    let shown: Vec<_> = (0..12)
        .filter(|&i| columns[i] && !(i == 11 && tree.options.toc))
        .collect();
    let narrow = ui.available_width() < 650.0;
    let widths = [
        330.0,
        85.0,
        if narrow { 78.0 } else { 90.0 },
        if narrow { 55.0 } else { 65.0 },
        if narrow { 95.0 } else { 125.0 },
        180.0,
        160.0,
        230.0,
        240.0,
        240.0,
        240.0,
        230.0,
    ];
    let remaining: f32 = shown
        .iter()
        .filter(|&&col| col != 0)
        .map(|&col| widths[col] + 10.0)
        .sum();
    let name_width = (ui.available_width() - remaining - 20.0).max(170.0);
    egui::ScrollArea::horizontal().id_salt("tree_horizontal").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        let mut table = TableBuilder::new(ui).id_salt("dossier_tree").striped(true).resizable(true).sense(Sense::click())
            .auto_shrink([false, false]).min_scrolled_height(70.0).cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for &col in &shown {
            table = table.column(Column::initial(if col == 0 { name_width } else { widths[col] }).at_least(if col == 0 { 170.0 } else { 55.0 }));
        }
        table.header(28.0, |mut header| {
            for &col in &shown { header.col(|ui| {
                let heading = match col { 0 => "DOCUMENT / SECTION", 2 => "OPERATION", 3 => "SEQ", 7 if tree.options.toc => "FILENAME", _ => HEADERS[col] };
                theme::eyebrow(ui, &heading.to_uppercase());
            }); }
        }).body(|body| { body.rows(24.0, rows.len(), |mut row| {
            let (id, depth) = rows[row.index()];
            let node = &tree.nodes[id];
            row.set_selected(selected == Some(id));
            let color = match node.kind {
                NodeKind::Leaf(leaf) => {
                    let leaf = &dossier.leaves[leaf];
                    if leaf.missing { Some(theme::RED) }
                    else if !leaf.warnings.is_empty() { Some(theme::AMBER) }
                    else if !tree.options.consolidated && leaf.operation == "delete" { Some(theme::MUTED) }
                    else { None }
                }
                NodeKind::Orphan(_) => Some(theme::BLUE),
                NodeKind::Backbone(bb) if dossier.backbones[bb].parse_error.is_some() => Some(theme::RED),
                _ => None,
            };
            for &col in &shown { row.col(|ui| {
                if col == 0 {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(depth as f32 * 16.0);
                    if !node.children.is_empty() || !node.populated {
                        let open = tree.expanded.contains(&id);
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), Sense::click());
                        let center = rect.center();
                        let points = if open {
                            vec![center + egui::vec2(-4.0,-2.0), center + egui::vec2(0.0,2.0), center + egui::vec2(4.0,-2.0)]
                        } else {
                            vec![center + egui::vec2(-2.0,-4.0), center + egui::vec2(2.0,0.0), center + egui::vec2(-2.0,4.0)]
                        };
                        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5_f32, theme::MUTED)));
                        if response.clicked() { actions.push(Action::Toggle(id)); }
                    } else { ui.add_space(16.0); }
                    let (icon, icon_color) = match node.kind {
                        NodeKind::Dossier | NodeKind::Group => (theme::Icon::Folder, theme::ACCENT),
                        NodeKind::Sequence(_) => (theme::Icon::Layers, theme::BLUE),
                        NodeKind::Backbone(_) => (theme::Icon::Code, theme::BLUE),
                        _ => (theme::Icon::Document, color.unwrap_or(theme::MUTED)),
                    };
                    theme::icon(ui, icon, icon_color, 16.0);
                }
                if matches!(col, 2 | 4) && !node.columns[col].is_empty() {
                    theme::status(ui, &node.columns[col]);
                } else if col == 3 && !node.columns[col].is_empty() {
                    ui.label(RichText::new(&node.columns[col]).font(egui::FontId::monospace(11.5)).color(theme::MUTED));
                } else {
                    let mut text = RichText::new(&node.columns[col]);
                    if let Some(color) = color { text = text.color(color); }
                    if col == 0 && matches!(node.kind, NodeKind::Dossier | NodeKind::Sequence(_)) { text = text.strong(); }
                    if col == 0 && tree.options.toc && matches!(node.kind, NodeKind::Leaf(leaf) if dossier.leaves[leaf].title.is_empty()) { text = text.italics(); }
                    ui.add(egui::Label::new(text).truncate()).on_hover_text(&node.columns[col]);
                }
            }); }
            let response = row.response();
            if response.clicked() || response.secondary_clicked() { actions.push(Action::Select(id)); }
            if response.double_clicked() {
                if let Some(path) = node_path(&node.kind, dossier) { actions.push(Action::Open(path)); }
                else { actions.push(Action::Toggle(id)); }
            }
            let response = if let NodeKind::Leaf(leaf) = node.kind { response.on_hover_text(dossier.leaves[leaf].warnings.join("\n")) } else { response };
            response.context_menu(|ui| {
                let path = node_path(&node.kind, dossier);
                if ui.add_enabled(path.as_ref().is_some_and(|p| p.exists()), egui::Button::new("Open")).clicked() { actions.push(Action::Open(path.clone().unwrap())); ui.close(); }
                if let NodeKind::Leaf(id) = node.kind {
                    let leaf = &dossier.leaves[id];
                    let prev = leaf.predecessor.and_then(|p| dossier.open_path(p));
                    if ui.add_enabled(prev.is_some_and(Path::is_file), egui::Button::new("Open Predecessor")).clicked() { actions.push(Action::Open(prev.unwrap().into())); ui.close(); }
                    for (label, text) in [("Copy Href", &leaf.href), ("Copy modified-file", &leaf.modified_file), ("Copy Leaf ID", &leaf.id)] {
                        if ui.add_enabled(!text.is_empty(), egui::Button::new(label)).clicked() { actions.push(Action::Copy(text.clone())); ui.close(); }
                    }
                }
                if let Some(path) = path {
                    if ui.button("Show in File Manager").clicked() && let Some(parent) = path.parent() { actions.push(Action::Open(parent.into())); ui.close(); }
                    if ui.button("Copy Path").clicked() { actions.push(Action::Copy(path.display().to_string())); ui.close(); }
                }
            });
        }); });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_app() -> ViewerApp {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo-dossier");
        let dossier = Arc::new(Dossier::load(&root).unwrap());
        let tree = Tree::new(&dossier, ViewOptions::default());
        let mut app = ViewerApp {
            dossier: Some(dossier),
            tree: Some(tree),
            selected: Some(0),
            ..ViewerApp::empty()
        };
        app.update_details();
        app
    }

    fn frame(app: &mut ViewerApp, ctx: &egui::Context) {
        frame_at_size(app, ctx, egui::vec2(1500.0, 850.0));
    }

    fn frame_at_size(app: &mut ViewerApp, ctx: &egui::Context, size: egui::Vec2) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let output = ctx.run(input, |ctx| {
            app.render(ctx);
            // Read this pass before end_pass swaps the widget collections.
            if app.dossier.is_some() {
                let search = ctx
                    .read_response(egui::Id::new("document_search"))
                    .expect("The document search must remain available");
                assert!(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                        .expand(0.5)
                        .contains_rect(search.rect),
                    "Search field must stay inside the window: {:?} at {:?}",
                    search.rect,
                    size
                );
            }
        });
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn all_views_render_with_real_dossier_data() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = ViewerApp::empty();
        frame(&mut app, &ctx);
        let mut app = demo_app();
        app.columns.fill(true);
        for toc in [false, true] {
            for consolidated in [false, true] {
                app.options = ViewOptions {
                    toc,
                    consolidated,
                    show_backbones: true,
                };
                app.rebuild();
                let dossier = app.dossier.clone().unwrap();
                let tree = app.tree.as_mut().unwrap();
                for seq in &dossier.sequences {
                    tree.expand_sequence(&dossier, &seq.name);
                }
                let node_ids: Vec<_> = (0..tree.nodes.len())
                    .filter(|&id| {
                        matches!(
                            tree.nodes[id].kind,
                            NodeKind::Leaf(_) | NodeKind::Backbone(_) | NodeKind::Orphan(_)
                        )
                    })
                    .collect();
                for node in node_ids {
                    app.selected = Some(node);
                    app.update_details();
                    for tab in [Tab::Overview, Tab::Metadata, Tab::Xml, Tab::Versions] {
                        app.tab = tab;
                        frame(&mut app, &ctx);
                    }
                }
            }
        }
        app.search = "quality".into();
        frame(&mut app, &ctx);
        app.message = Some(("Export completed".into(), "Copied files: 4".into()));
        frame(&mut app, &ctx);
        app.message = None;
        for tab in [Tab::Overview, Tab::Metadata, Tab::Xml, Tab::Versions] {
            app.tab = tab;
            frame_at_size(&mut app, &ctx, egui::vec2(900.0, 550.0));
        }
        frame_at_size(&mut ViewerApp::empty(), &ctx, egui::vec2(900.0, 550.0));
    }

    #[test]
    fn selecting_inherited_document_keeps_export_snapshot_sequence() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = demo_app();
        app.options.consolidated = true;
        app.rebuild();
        let dossier = app.dossier.clone().unwrap();
        let tree = app.tree.as_mut().unwrap();
        tree.expand_sequence(&dossier, "0002");
        let node = tree
            .nodes
            .iter()
            .position(
                |n| matches!(n.kind, NodeKind::Leaf(id) if dossier.leaves[id].id == "summary-v2"),
            )
            .unwrap();
        app.actions(vec![Action::Select(node)], &ctx);
        assert_eq!(app.selected_sequence().as_deref(), Some("0002"));
        assert_eq!(app.details.title, "Revised quality overview");
        assert_eq!(app.details.versions.len(), 2);
        assert!(
            app.details
                .predecessor
                .as_ref()
                .unwrap()
                .ends_with("0000/m2/summary.txt")
        );
        app.options.toc = true;
        app.rebuild();
        assert_eq!(app.selected_sequence().as_deref(), Some("0002"));
        assert_eq!(app.details.title, "Revised quality overview");
        assert!(matches!(
            app.tree.as_ref().unwrap().nodes[app.selected.unwrap()].kind,
            NodeKind::Leaf(id) if dossier.leaves[id].id == "summary-v2"
        ));
    }
}
