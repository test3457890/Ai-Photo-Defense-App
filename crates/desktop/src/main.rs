//! Native desktop workstation. Three interface modes (Essential, Professional, Technical) share
//! one collection, one edit stack per photograph and the same Rust engine. Image work runs on
//! blocking worker threads; the UI thread only holds small previews.
use iced::widget::{
    self, Column, Space, button, checkbox, column, container, horizontal_rule, image, mouse_area,
    pick_list, row, scrollable, slider, stack, text, text_input, vertical_rule,
};
use iced::{
    Border, Color, Element, Event, Length, Point, Subscription, Task, Theme, keyboard, window,
};
use photo_core::{ExportFormat, Quality};
use photo_edit::{AspectRatio, Concealment, EditStack, Operation, Rect, Selection};
use photo_privacy::{Finding, Profile, ScanReport};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Edit previews are shown at native size so pointer positions map exactly to pixels.
const EDIT_PREVIEW: (u32, u32) = (820, 560);
const QUEUE_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Essential,
    Professional,
    Technical,
}
impl Mode {
    const ALL: [Self; 3] = [Self::Essential, Self::Professional, Self::Technical];
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?} mode")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Workspace {
    #[default]
    Export,
    Privacy,
    Edit,
    Defense,
    History,
    Lab,
    Models,
    Settings,
}
impl Workspace {
    fn label(self) -> &'static str {
        match self {
            Self::Export => "Batch Export",
            Self::Privacy => "Privacy Scan",
            Self::Edit => "Edit and Redact",
            Self::Defense => "AI Defense",
            Self::History => "Protection History",
            Self::Lab => "Defense Lab",
            Self::Models => "Model Library",
            Self::Settings => "Settings",
        }
    }
    fn for_mode(m: Mode) -> &'static [Workspace] {
        use Workspace::*;
        match m {
            Mode::Essential => &[Export, Privacy, Edit, Defense, Settings],
            Mode::Professional => &[Export, Privacy, Edit, Defense, History, Settings],
            Mode::Technical => &[
                Export, Privacy, Edit, Defense, History, Lab, Models, Settings,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
impl std::fmt::Display for Appearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Black,
    White,
    Blur,
    Pixelate,
    Mosaic,
}
impl Method {
    const ALL: [Self; 5] = [
        Self::Black,
        Self::White,
        Self::Blur,
        Self::Pixelate,
        Self::Mosaic,
    ];
    fn concealment(self, strength: u32) -> Concealment {
        match self {
            Self::Black => Concealment::BLACK,
            Self::White => Concealment::WHITE,
            Self::Blur => Concealment::GaussianBlur {
                sigma: strength as f32,
            },
            Self::Pixelate => Concealment::Pixelate {
                block: strength.max(2),
            },
            Self::Mosaic => Concealment::Mosaic {
                block: strength.max(2),
            },
        }
    }
}
impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Black => "Opaque black (secure)",
            Self::White => "Opaque white (secure)",
            Self::Blur => "Gaussian blur (visual only)",
            Self::Pixelate => "Pixelate (visual only)",
            Self::Mosaic => "Mosaic (visual only)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Rectangle,
    Ellipse,
}
impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Configurable social export presets: fit-within boxes, never upscaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SizePreset(&'static str, u32, u32);
impl std::fmt::Display for SizePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}x{})", self.0, self.1, self.2)
    }
}
const SIZE_PRESETS: [SizePreset; 6] = [
    SizePreset("Portrait post", 1080, 1350),
    SizePreset("Square post", 1080, 1080),
    SizePreset("Story / reel", 1080, 1920),
    SizePreset("Landscape post", 1600, 900),
    SizePreset("Full HD", 1920, 1080),
    SizePreset("Web medium", 1280, 1280),
];

#[derive(Debug, Clone)]
struct Edited {
    handle: image::Handle,
    full: (u32, u32),
}

#[derive(Debug, Clone)]
struct ScanState {
    report: ScanReport,
    dismissed: Vec<bool>,
    dims: (u32, u32),
    generation: u64,
}

#[derive(Debug, Clone)]
enum ExportState {
    Done {
        path: PathBuf,
        passed: bool,
        warnings: Vec<String>,
        quality: Quality,
        preview: image::Handle,
        difference: image::Handle,
    },
    Blocked(Vec<String>),
}

struct Entry {
    path: PathBuf,
    width: u32,
    height: u32,
    original: image::Handle,
    edits: EditStack,
    generation: u64,
    edited: Option<Edited>,
    scan: Option<ScanState>,
    export: Option<ExportState>,
}

#[derive(Debug, Clone)]
enum Message {
    Open,
    InputPath(String),
    ImportPath,
    OutputDirectory(String),
    Picked(Vec<PathBuf>),
    Imported(Result<Vec<(PathBuf, u32, u32, image::Handle)>, String>),
    Select(usize),
    Mode(Mode),
    Workspace(Workspace),
    Appearance(Appearance),
    Format(ExportFormat),
    JpegQuality(u8),
    Profile(String),
    Export { accept_unresolved: bool },
    Destination(Option<PathBuf>, bool),
    Exported(Vec<(usize, Result<ExportState, String>)>),
    Cancel,
    Clear,
    // Editing
    Apply(Operation),
    Undo,
    Redo,
    StraightenAngle(f32),
    Rendered(usize, u64, Result<Edited, String>),
    MethodPicked(Method),
    ShapePicked(Shape),
    Strength(u32),
    InvertSelection(bool),
    RegionField(usize, String),
    ConcealSelection,
    PointerMove(Point),
    PointerDown,
    PointerUp,
    ClearSelection,
    // Privacy scan
    Scan,
    Scanned(usize, u64, Result<(ScanReport, (u32, u32)), String>),
    FocusFinding(usize),
    Dismiss(usize, bool),
    ConcealFindings,
    // Lab
    Lab,
    LabDone(Result<photo_lab::Report, String>),
    SaveReport,
    ReportSaved(Result<Option<PathBuf>, String>),
    Event(Event),
    Menu(&'static str),
    Difference(bool),
    PanelWidth(f32),
}

struct App {
    input_path: String,
    output_directory: String,
    entries: Vec<Entry>,
    selected: Option<usize>,
    mode: Mode,
    workspace: Workspace,
    appearance: Appearance,
    format: ExportFormat,
    jpeg_quality: u8,
    profile: String,
    busy: bool,
    cancel: Arc<AtomicBool>,
    status: String,
    logs: Vec<String>,
    report: Option<photo_lab::Report>,
    menu: Option<&'static str>,
    difference: bool,
    panel_width: f32,
    // Edit tools
    straighten: f32,
    method: Method,
    shape: Shape,
    strength: u32,
    invert: bool,
    region: [String; 4],
    pointer: Point,
    drag_from: Option<Point>,
    focused_finding: Option<usize>,
}
impl Default for App {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            output_directory: String::new(),
            entries: vec![],
            selected: None,
            mode: Mode::Essential,
            workspace: Workspace::Export,
            appearance: Appearance::System,
            format: ExportFormat::Png,
            jpeg_quality: 92,
            profile: "Social Media".into(),
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            status: "Add photographs to begin. Processing stays on this computer.".into(),
            logs: vec!["Ready. No validated AI defense model is installed.".into()],
            report: None,
            menu: None,
            difference: false,
            panel_width: 230.,
            straighten: 0.,
            method: Method::Black,
            shape: Shape::Rectangle,
            strength: 12,
            invert: false,
            region: Default::default(),
            pointer: Point::ORIGIN,
            drag_from: None,
            focused_finding: None,
        }
    }
}

fn thumb(pixels: &::image::DynamicImage, w: u32, h: u32) -> ::image::RgbaImage {
    pixels.thumbnail(w, h).to_rgba8()
}
fn handle(p: &::image::RgbaImage) -> image::Handle {
    image::Handle::from_rgba(p.width(), p.height(), p.clone().into_raw())
}
fn short_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn profile_by_name(n: &str) -> Profile {
    Profile::builtin(n).unwrap_or_else(|| Profile::builtin("Maximum Privacy").expect("builtin"))
}
fn box_style(color: Color, width: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: Border {
            color,
            width,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
fn section(title: &str) -> Element<'_, Message> {
    column![text(title).size(11), horizontal_rule(1)]
        .spacing(4)
        .into()
}

impl App {
    fn note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.status = msg.clone();
        self.logs.push(msg);
        if self.logs.len() > 300 {
            self.logs.remove(0);
        }
    }
    fn current(&self) -> Option<&Entry> {
        self.selected.and_then(|i| self.entries.get(i))
    }
    /// Re-renders the selected photograph's edit stack off the UI thread.
    fn rerender(&mut self, i: usize) -> Task<Message> {
        let Some(e) = self.entries.get_mut(i) else {
            return Task::none();
        };
        e.generation += 1;
        let generation = e.generation;
        let path = e.path.clone();
        let edits = e.edits.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || -> Result<Edited, String> {
                    let p = photo_core::load(&path).map_err(|e| format!("{e:#}"))?;
                    let out = edits.render(&p.pixels).map_err(|e| format!("{e:#}"))?;
                    let preview = thumb(&out, EDIT_PREVIEW.0, EDIT_PREVIEW.1);
                    Ok(Edited {
                        handle: handle(&preview),
                        full: (out.width(), out.height()),
                    })
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |r| Message::Rendered(i, generation, r),
        )
    }
    fn after_edit(&mut self, i: usize) -> Task<Message> {
        if let Some(e) = self.entries.get_mut(i) {
            e.export = None;
        }
        self.rerender(i)
    }
    /// Current selection in full-resolution edited-image coordinates.
    fn selection_rect(&self) -> Option<Rect> {
        let v: Vec<u32> = self
            .region
            .iter()
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        (v.len() == 4 && v[2] > 0 && v[3] > 0).then(|| Rect::new(v[0], v[1], v[2], v[3]))
    }
    fn set_region(&mut self, r: Rect) {
        self.region = [r.x, r.y, r.width, r.height].map(|v| v.to_string());
    }
    /// Pointer messages already carry full-resolution image coordinates.
    fn preview_to_full(&self, p: Point) -> Option<(f32, f32)> {
        self.current()?.edited.as_ref()?;
        Some((p.x, p.y))
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::InputPath(p) => self.input_path = p,
            Message::OutputDirectory(p) => self.output_directory = p,
            Message::ImportPath if !self.busy && !self.input_path.trim().is_empty() => {
                return self.update(Message::Picked(vec![PathBuf::from(self.input_path.trim())]));
            }
            Message::Open if !self.busy => {
                self.menu = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter(
                                "Photographs",
                                &["jpg", "jpeg", "png", "webp", "tif", "tiff"],
                            )
                            .pick_files()
                            .await
                            .map(|v| v.into_iter().map(|f| f.path().to_path_buf()).collect())
                            .unwrap_or_default()
                    },
                    Message::Picked,
                );
            }
            Message::Picked(paths) if !paths.is_empty() => {
                if self.busy {
                    self.note("Busy: wait for the current task before adding photographs.");
                    return Task::none();
                }
                // Folders are expanded with the same safe walker used by batch processing.
                let collected = photo_pipeline::batch::collect(
                    &paths,
                    &photo_pipeline::batch::CollectOptions {
                        recursive: true,
                        ..Default::default()
                    },
                );
                let paths = match collected {
                    Ok(c) => {
                        for (p, why) in c.skipped {
                            self.note(format!("Skipped {}: {why}", short_name(&p)));
                        }
                        c.files
                    }
                    Err(e) => {
                        self.note(format!("{e:#}"));
                        return Task::none();
                    }
                };
                if self.entries.len() + paths.len() > QUEUE_LIMIT {
                    self.note(format!("Collection limit is {QUEUE_LIMIT} photographs. Use the CLI batch command for larger folders."));
                    return Task::none();
                }
                self.busy = true;
                self.note("Reading photographs...");
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut v = vec![];
                            for path in paths {
                                let p = photo_core::load(&path)
                                    .map_err(|e| format!("{}: {e:#}", short_name(&path)))?;
                                v.push((
                                    path,
                                    p.pixels.width(),
                                    p.pixels.height(),
                                    handle(&thumb(&p.pixels, 900, 600)),
                                ));
                            }
                            Ok(v)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::Imported,
                );
            }
            Message::Imported(result) => {
                self.busy = false;
                match result {
                    Ok(list) => {
                        for (path, width, height, original) in list {
                            if !self.entries.iter().any(|x| x.path == path) {
                                self.entries.push(Entry {
                                    path,
                                    width,
                                    height,
                                    original,
                                    edits: EditStack::new(),
                                    generation: 0,
                                    edited: None,
                                    scan: None,
                                    export: None,
                                });
                            }
                        }
                        self.note(format!(
                            "{} photographs. Originals are never modified.",
                            self.entries.len()
                        ));
                        if self.selected.is_none() && !self.entries.is_empty() {
                            return self.update(Message::Select(0));
                        }
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::Select(i) if i < self.entries.len() => {
                self.selected = Some(i);
                self.report = None;
                self.focused_finding = None;
                self.region = Default::default();
                if self.entries[i].edited.is_none() {
                    return self.rerender(i);
                }
            }
            Message::Mode(m) => {
                self.mode = m;
                self.menu = None;
                if !Workspace::for_mode(m).contains(&self.workspace) {
                    self.workspace = Workspace::Export;
                }
            }
            Message::Workspace(w) => {
                self.workspace = w;
                self.menu = None;
            }
            Message::Appearance(a) => self.appearance = a,
            Message::Format(f) => self.format = f,
            Message::JpegQuality(q) => self.jpeg_quality = q,
            Message::Profile(p) => self.profile = p,
            Message::Menu(m) => self.menu = if self.menu == Some(m) { None } else { Some(m) },
            Message::Difference(d) => self.difference = d,
            Message::PanelWidth(w) => self.panel_width = w,
            // ---------- Editing ----------
            Message::Apply(op) if !self.busy => {
                if let Some(i) = self.selected {
                    if let Err(e) = self.entries[i].edits.push(op.clone()) {
                        self.note(format!("{e:#}"));
                        return Task::none();
                    }
                    self.note(format!("Applied: {}", op.label()));
                    self.region = Default::default();
                    return self.after_edit(i);
                }
            }
            Message::Undo => {
                if let Some(i) = self.selected
                    && self.entries[i].edits.undo()
                {
                    self.note("Undo");
                    return self.after_edit(i);
                }
            }
            Message::Redo => {
                if let Some(i) = self.selected
                    && self.entries[i].edits.redo()
                {
                    self.note("Redo");
                    return self.after_edit(i);
                }
            }
            Message::StraightenAngle(a) => self.straighten = a,
            Message::Rendered(i, generation, r) => {
                if let Some(e) = self.entries.get_mut(i)
                    && e.generation == generation
                {
                    match r {
                        Ok(ed) => e.edited = Some(ed),
                        Err(err) => self.note(err),
                    }
                }
            }
            Message::MethodPicked(m) => {
                self.method = m;
                self.strength = match m {
                    Method::Blur => 12,
                    Method::Pixelate | Method::Mosaic => 16,
                    _ => self.strength,
                };
            }
            Message::ShapePicked(s) => self.shape = s,
            Message::Strength(s) => self.strength = s,
            Message::InvertSelection(b) => self.invert = b,
            Message::RegionField(k, v) => {
                if k < 4 {
                    self.region[k] = v.chars().filter(char::is_ascii_digit).take(6).collect();
                }
            }
            Message::ConcealSelection => {
                if let Some(r) = self.selection_rect() {
                    let selection = match self.shape {
                        Shape::Rectangle => Selection::Rect(r),
                        Shape::Ellipse => Selection::Ellipse(r),
                    };
                    return self.update(Message::Apply(Operation::Conceal {
                        selection,
                        method: self.method.concealment(self.strength),
                        invert: self.invert,
                    }));
                }
                self.note("Drag on the photograph or enter X, Y, width and height first.");
            }
            Message::PointerMove(p) => {
                self.pointer = p;
                if let (Some(a), Some(b)) = (
                    self.drag_from.and_then(|a| self.preview_to_full(a)),
                    self.preview_to_full(p),
                ) {
                    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
                    let r = Rect::new(
                        x0 as u32,
                        y0 as u32,
                        ((a.0 - b.0).abs() as u32).max(1),
                        ((a.1 - b.1).abs() as u32).max(1),
                    );
                    self.set_region(r);
                }
            }
            Message::PointerDown => self.drag_from = Some(self.pointer),
            Message::PointerUp => self.drag_from = None,
            Message::ClearSelection => {
                self.region = Default::default();
                self.focused_finding = None;
            }
            // ---------- Privacy scan ----------
            Message::Scan if !self.busy => {
                let Some(i) = self.selected else {
                    return Task::none();
                };
                let e = &self.entries[i];
                let (path, edits, generation) = (e.path.clone(), e.edits.clone(), e.generation);
                let profile = profile_by_name(&self.profile);
                self.busy = true;
                self.note(format!("Scanning with the {} profile...", profile.name));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(
                            move || -> Result<(ScanReport, (u32, u32)), String> {
                                let p = photo_core::load(&path).map_err(|e| format!("{e:#}"))?;
                                let pixels =
                                    edits.render(&p.pixels).map_err(|e| format!("{e:#}"))?;
                                let dims = (pixels.width(), pixels.height());
                                let staged = photo_core::Photo {
                                    pixels,
                                    fingerprint: p.fingerprint,
                                    format: p.format,
                                    bytes: p.bytes,
                                };
                                Ok((photo_pipeline::scan_photo(&staged, &profile), dims))
                            },
                        )
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::Scanned(i, generation, r),
                );
            }
            Message::Scanned(i, generation, r) => {
                self.busy = false;
                match r {
                    Ok((report, dims)) => {
                        let n = report.findings.len();
                        let missing = report.not_examined.len();
                        if let Some(e) = self.entries.get_mut(i) {
                            e.scan = Some(ScanState {
                                dismissed: vec![false; n],
                                report,
                                dims,
                                generation,
                            });
                        }
                        self.focused_finding = None;
                        self.note(format!("Scan finished: {n} finding(s). {missing} profile categories could not be examined without additional models."));
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::FocusFinding(k) => {
                self.focused_finding = Some(k);
                if let Some(r) = self
                    .current()
                    .and_then(|e| e.scan.as_ref())
                    .and_then(|s| s.report.findings.get(k))
                    .and_then(|f| f.region)
                {
                    self.set_region(Rect::new(r.x, r.y, r.width, r.height));
                }
            }
            Message::Dismiss(k, d) => {
                if let Some(i) = self.selected
                    && let Some(s) = self.entries[i].scan.as_mut()
                    && k < s.dismissed.len()
                {
                    s.dismissed[k] = d;
                }
            }
            Message::ConcealFindings => {
                let Some(i) = self.selected else {
                    return Task::none();
                };
                let Some(s) = &self.entries[i].scan else {
                    return Task::none();
                };
                if s.generation != self.entries[i].generation {
                    self.note(
                        "Edits changed since the scan. Scan again before concealing findings.",
                    );
                    return Task::none();
                }
                let chosen: Vec<Finding> = s
                    .report
                    .findings
                    .iter()
                    .zip(&s.dismissed)
                    .filter(|(f, d)| !**d && f.region.is_some())
                    .map(|(f, _)| f.clone())
                    .collect();
                let pref = match self.method {
                    Method::White => photo_privacy::profiles::RedactionPreference::OpaqueWhite,
                    Method::Blur => photo_privacy::profiles::RedactionPreference::Blur,
                    Method::Pixelate | Method::Mosaic => {
                        photo_privacy::profiles::RedactionPreference::Pixelate
                    }
                    Method::Black => photo_privacy::profiles::RedactionPreference::OpaqueBlack,
                };
                match photo_pipeline::conceal_findings(&chosen, pref, 4) {
                    Some(op) => return self.update(Message::Apply(op)),
                    None => self.note("No located, undismissed findings to conceal. Metadata findings are always removed on export."),
                }
            }
            // ---------- Export ----------
            Message::Export { accept_unresolved } if !self.busy && !self.entries.is_empty() => {
                if !self.output_directory.trim().is_empty() {
                    return self.update(Message::Destination(
                        Some(PathBuf::from(self.output_directory.trim())),
                        accept_unresolved,
                    ));
                }
                self.note("Choose an output folder, or type one in the output directory field.");
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |d| Message::Destination(d, accept_unresolved),
                );
            }
            Message::Destination(Some(dir), accept) if !self.busy => {
                if !dir.is_dir() {
                    self.note("Output directory must already exist.");
                    return Task::none();
                }
                self.busy = true;
                self.cancel.store(false, Ordering::Relaxed);
                let cancel = self.cancel.clone();
                let (f, q) = (self.format, self.jpeg_quality);
                let profile = profile_by_name(&self.profile);
                let jobs: Vec<(PathBuf, EditStack)> = self
                    .entries
                    .iter()
                    .map(|e| (e.path.clone(), e.edits.clone()))
                    .collect();
                self.note(if accept {
                    "Exporting, accepting unresolved findings you reviewed..."
                } else {
                    "Exporting through the verified pipeline..."
                });
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut out = vec![];
                            for (i, (path, edits)) in jobs.iter().enumerate() {
                                if cancel.load(Ordering::Relaxed) {
                                    break;
                                }
                                let work = || -> anyhow::Result<ExportState> {
                                    let p = photo_core::load(path)?;
                                    let prepared =
                                        photo_pipeline::prepare(&p, edits, f, q, &profile)?;
                                    let v = &prepared.report.verification;
                                    if !v.passed && !accept {
                                        let mut w = v.warnings.clone();
                                        w.extend(
                                            v.metadata_findings
                                                .iter()
                                                .chain(&v.remaining_findings)
                                                .map(|f| {
                                                    format!("{:?}: {}", f.category, f.description)
                                                }),
                                        );
                                        return Ok(ExportState::Blocked(w));
                                    }
                                    let saved =
                                        photo_pipeline::publish(&prepared, path, &dir, f, accept)?;
                                    let decoded = photo_core::decode_export(&prepared.bytes)?;
                                    let diff = if prepared.rendered.width() == p.pixels.width()
                                        && prepared.rendered.height() == p.pixels.height()
                                    {
                                        photo_core::difference(&p.pixels, &decoded, 8)?
                                    } else {
                                        photo_core::difference(&prepared.rendered, &decoded, 8)?
                                    };
                                    Ok(ExportState::Done {
                                        path: saved.image,
                                        passed: saved.verification_passed,
                                        warnings: v.warnings.clone(),
                                        quality: v.encoded_vs_render.clone(),
                                        preview: handle(&thumb(&decoded, 900, 600)),
                                        difference: handle(&thumb(&diff, 900, 600)),
                                    })
                                };
                                out.push((
                                    i,
                                    work().map_err(|e| format!("{}: {e:#}", short_name(path))),
                                ));
                            }
                            out
                        })
                        .await
                        .unwrap_or_else(|e| vec![(0, Err(e.to_string()))])
                    },
                    Message::Exported,
                );
            }
            Message::Exported(results) => {
                self.busy = false;
                let (mut good, mut blocked, mut failed) = (0, 0, 0);
                for (i, r) in results {
                    match r {
                        Ok(s) => {
                            match &s {
                                ExportState::Done { .. } => good += 1,
                                ExportState::Blocked(_) => blocked += 1,
                            }
                            if let Some(e) = self.entries.get_mut(i) {
                                e.export = Some(s);
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            self.note(e);
                        }
                    }
                }
                self.note(format!(
                    "Export: {good} saved, {blocked} blocked by verification, {failed} errors.{}",
                    if self.cancel.load(Ordering::Relaxed) {
                        " Cancelled between files."
                    } else {
                        ""
                    }
                ));
            }
            Message::Cancel => {
                self.cancel.store(true, Ordering::Relaxed);
                self.note("Cancellation requested. The current file will finish safely.");
            }
            Message::Clear if !self.busy => {
                self.entries.clear();
                self.selected = None;
                self.report = None;
                self.note("Collection cleared. Exported files were kept.");
            }
            // ---------- Lab ----------
            Message::Lab if !self.busy => {
                if let Some(e) = self.current() {
                    if let Some(ExportState::Done { path, .. }) = &e.export {
                        let (a, b) = (e.path.clone(), path.clone());
                        self.busy = true;
                        self.note("Measuring transformations...");
                        return Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    let a = photo_core::load(&a).map_err(|e| e.to_string())?;
                                    let b = photo_core::load(&b).map_err(|e| e.to_string())?;
                                    photo_lab::evaluate(&a.pixels, &b.pixels)
                                        .map_err(|e| e.to_string())
                                })
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                            },
                            Message::LabDone,
                        );
                    }
                    self.note("Export the selected photograph without geometry edits before running the lab.");
                }
            }
            Message::LabDone(r) => {
                self.busy = false;
                match r {
                    Ok(r) => {
                        self.report = Some(r);
                        self.note(
                            "Eight transformations measured. AI efficacy has not been evaluated.",
                        );
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::SaveReport if self.report.is_some() => {
                let report = self.report.clone().expect("checked");
                let directory = self.output_directory.trim().to_string();
                return Task::perform(
                    async move {
                        let dir = if directory.is_empty() {
                            rfd::AsyncFileDialog::new()
                                .pick_folder()
                                .await
                                .map(|f| f.path().to_path_buf())
                        } else {
                            Some(PathBuf::from(directory))
                        };
                        match dir {
                            Some(d) => photo_core::save_unique(
                                &d,
                                "lab-report",
                                "json",
                                &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
                            )
                            .map(Some)
                            .map_err(|e| e.to_string()),
                            None => Ok(None),
                        }
                    },
                    Message::ReportSaved,
                );
            }
            Message::ReportSaved(r) => match r {
                Ok(Some(p)) => self.note(format!("Lab report saved: {}", p.display())),
                Ok(None) => self.note("Report save cancelled."),
                Err(e) => self.note(e),
            },
            // ---------- Input ----------
            Message::Event(Event::Window(window::Event::FileDropped(path))) => {
                return self.update(Message::Picked(vec![path]));
            }
            Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
                key, modifiers, ..
            })) => {
                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character("o") => return self.update(Message::Open),
                        keyboard::Key::Character("e") => {
                            return self.update(Message::Export {
                                accept_unresolved: false,
                            });
                        }
                        keyboard::Key::Character("z") if modifiers.shift() => {
                            return self.update(Message::Redo);
                        }
                        keyboard::Key::Character("z") => return self.update(Message::Undo),
                        keyboard::Key::Character("y") => return self.update(Message::Redo),
                        keyboard::Key::Character("p") => return self.update(Message::Scan),
                        keyboard::Key::Character("1") => {
                            return self.update(Message::Mode(Mode::Essential));
                        }
                        keyboard::Key::Character("2") => {
                            return self.update(Message::Mode(Mode::Professional));
                        }
                        keyboard::Key::Character("3") => {
                            return self.update(Message::Mode(Mode::Technical));
                        }
                        _ => {}
                    }
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::Tab) => {
                        return if modifiers.shift() {
                            widget::focus_previous()
                        } else {
                            widget::focus_next()
                        };
                    }
                    keyboard::Key::Named(keyboard::key::Named::Escape) => {
                        self.menu = None;
                        return self.update(Message::ClearSelection);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                        if self.workspace == Workspace::Edit
                            && self.selection_rect().is_some()
                            && !modifiers.command() =>
                    {
                        return self.update(Message::ConcealSelection);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn theme(&self) -> Theme {
        match self.appearance {
            Appearance::Light => Theme::Light,
            Appearance::Dark => Theme::Dark,
            Appearance::System => Theme::default(),
        }
    }

    // ---------------------------------------------------------------- views
    fn files(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(2);
        for (i, e) in self.entries.iter().enumerate() {
            let state = match &e.export {
                Some(ExportState::Done { passed: true, .. }) => "Exported, verified",
                Some(ExportState::Done { .. }) => "Exported with findings",
                Some(ExportState::Blocked(_)) => "Blocked: review",
                None if !e.edits.is_empty() => "Edited",
                None => "Ready",
            };
            let findings = e
                .scan
                .as_ref()
                .map(|s| format!("  |  {} findings", s.report.findings.len()))
                .unwrap_or_default();
            list = list.push(
                button(column![
                    text(short_name(&e.path)).size(13),
                    text(format!("{} x {}  |  {state}{findings}", e.width, e.height)).size(11)
                ])
                .width(Length::Fill)
                .style(if self.selected == Some(i) {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::Select(i)),
            );
        }
        if self.entries.is_empty() {
            list = list.push(text("Drop JPEG, PNG, WebP or TIFF photographs or folders here, or use File > Add photographs.").size(13));
        }
        column![section("COLLECTION"), scrollable(list).height(Length::Fill)]
            .spacing(6)
            .into()
    }

    /// Native-size edited preview with selection and detection overlays.
    fn canvas(&self) -> Element<'_, Message> {
        let Some(e) = self.current() else {
            return container(text("Add a photograph to open the workspace."))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        let Some(ed) = &e.edited else {
            return container(text("Rendering preview..."))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        let footer = text(format!(
            "{}  |  {} x {} after edits  |  drag to select",
            short_name(&e.path),
            ed.full.0,
            ed.full.1
        ))
        .size(11);
        column![widget::responsive(move |size| self.canvas_at(size)), footer]
            .spacing(6)
            .into()
    }

    /// Lays the preview out at a scale that fits the available space. Pointer positions are
    /// converted to full-resolution image coordinates here, where the scale is known.
    fn canvas_at(&self, size: iced::Size) -> Element<'_, Message> {
        let Some(e) = self.current() else {
            return Space::new(0, 0).into();
        };
        let Some(ed) = &e.edited else {
            return Space::new(0, 0).into();
        };
        let (fw, fh) = (ed.full.0 as f32, ed.full.1 as f32);
        let s = (size.width / fw).min(size.height / fh).clamp(0.01, 4.);
        let (pw, ph) = ((fw * s).floor(), (fh * s).floor());
        let mut layers = stack![
            image(ed.handle.clone())
                .width(pw)
                .height(ph)
                .content_fit(iced::ContentFit::Fill)
        ]
        .width(pw)
        .height(ph);
        let overlay = |r: Rect, color: Color, w: f32| -> Element<'_, Message> {
            let (x, y) = (r.x as f32 * s, r.y as f32 * s);
            let (rw, rh) = ((r.width as f32 * s).max(2.), (r.height as f32 * s).max(2.));
            container(container(Space::new(rw, rh)).style(box_style(color, w)))
                .padding(iced::Padding {
                    top: y,
                    left: x,
                    right: 0.,
                    bottom: 0.,
                })
                .into()
        };
        if let Some(scan) = &e.scan
            && scan.dims == ed.full
            && scan.generation == e.generation
        {
            for (k, (f, d)) in scan.report.findings.iter().zip(&scan.dismissed).enumerate() {
                if let (Some(r), false) = (f.region, *d) {
                    let focused = self.focused_finding == Some(k);
                    layers = layers.push(overlay(
                        Rect::new(r.x, r.y, r.width, r.height),
                        if focused {
                            Color::from_rgb(1., 0.75, 0.)
                        } else {
                            Color::from_rgb(0.9, 0.2, 0.2)
                        },
                        if focused { 3. } else { 2. },
                    ));
                }
            }
        }
        if let Some(r) = self.selection_rect() {
            layers = layers.push(overlay(r, Color::from_rgb(0.2, 0.6, 1.), 2.));
        }
        let area = mouse_area(layers)
            .on_move(move |p| {
                Message::PointerMove(Point::new((p.x / s).clamp(0., fw), (p.y / s).clamp(0., fh)))
            })
            .on_press(Message::PointerDown)
            .on_release(Message::PointerUp)
            .interaction(iced::mouse::Interaction::Crosshair);
        container(area)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn comparison(&self) -> Element<'_, Message> {
        let Some(e) = self.current() else {
            return self.canvas();
        };
        let left = column![
            text("ORIGINAL (never modified)").size(11),
            image::viewer(e.original.clone())
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .spacing(6)
        .width(Length::FillPortion(1));
        let right: Element<_> = match &e.export {
            Some(ExportState::Done {
                preview,
                difference,
                ..
            }) => column![
                text(if self.difference {
                    "ABSOLUTE DIFFERENCE x8"
                } else {
                    "EXPORTED FILE (reopened)"
                })
                .size(11),
                image::viewer(if self.difference {
                    difference.clone()
                } else {
                    preview.clone()
                })
                .width(Length::Fill)
                .height(Length::Fill),
                checkbox("Show difference x8", self.difference).on_toggle(Message::Difference)
            ]
            .spacing(6)
            .width(Length::FillPortion(1))
            .into(),
            _ => match &e.edited {
                Some(ed) => column![
                    text("EDITED (not yet exported)").size(11),
                    image::viewer(ed.handle.clone())
                        .width(Length::Fill)
                        .height(Length::Fill)
                ]
                .spacing(6)
                .width(Length::FillPortion(1))
                .into(),
                None => container(text("Rendering..."))
                    .center_x(Length::FillPortion(1))
                    .into(),
            },
        };
        row![left, vertical_rule(1), right]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    fn export_panel(&self) -> Element<'_, Message> {
        let can = !self.busy && !self.entries.is_empty();
        let mut c = column![
            section("EXPORT"),
            text("Output directory").size(12),
            text_input("Existing folder, or blank to browse", &self.output_directory).on_input(Message::OutputDirectory),
            row![text("Format").size(12).width(90), pick_list(ExportFormat::ALL, Some(self.format), Message::Format)].spacing(6).align_y(iced::Alignment::Center),
            text(format!("JPEG quality: {}", self.jpeg_quality)).size(12),
            slider(1..=100, self.jpeg_quality, Message::JpegQuality),
            row![text("Privacy profile").size(12).width(90), pick_list(photo_privacy::profiles::BUILTIN.map(String::from).to_vec(), Some(self.profile.clone()), Message::Profile)].spacing(6).align_y(iced::Alignment::Center),
            text("Every export: edits applied, flattened, encoded from pixels only (no EXIF, GPS, XMP, IPTC, ICC or thumbnails), reopened and verified. Originals are never overwritten.").size(11),
            button("Export all (verified)  Ctrl+E").on_press_maybe(can.then_some(Message::Export { accept_unresolved: false })),
        ]
        .spacing(8);
        if let Some(e) = self.current() {
            match &e.export {
                Some(ExportState::Done {
                    path,
                    passed,
                    warnings,
                    quality,
                    ..
                }) => {
                    c = c.push(section("FINAL VERIFICATION")).push(text(format!(
                        "{}\nSaved: {}\nPSNR vs edited: {}\nSSIM: {:.5}",
                        if *passed { "Passed: no metadata, no remaining credential codes, redactions intact." } else { "Exported with unresolved findings you accepted." },
                        short_name(path),
                        quality.psnr_db.map(|x| format!("{x:.2} dB")).unwrap_or("lossless".into()),
                        quality.ssim_luma_8x8
                    )).size(11));
                    for w in warnings {
                        c = c.push(text(format!("! {w}")).size(11));
                    }
                }
                Some(ExportState::Blocked(w)) => {
                    c = c.push(section("EXPORT BLOCKED")).push(text("Final verification found unresolved items. Redact them, or review and accept explicitly.").size(11));
                    for x in w.iter().take(12) {
                        c = c.push(text(format!("! {x}")).size(11));
                    }
                    c = c.push(
                        button("I reviewed these: export anyway")
                            .style(button::danger)
                            .on_press_maybe(can.then_some(Message::Export {
                                accept_unresolved: true,
                            })),
                    );
                }
                None => {}
            }
        }
        scrollable(c).height(Length::Fill).into()
    }

    fn edit_panel(&self) -> Element<'_, Message> {
        let e = self.current();
        let can = e.is_some() && !self.busy;
        let on = |m: Message| can.then_some(m);
        let (undo, redo) = e
            .map(|e| (e.edits.can_undo(), e.edits.can_redo()))
            .unwrap_or((false, false));
        let mut c = column![
            section("CROP AND TRANSFORM"),
            pick_list(AspectRatio::PRESETS.to_vec(), None::<AspectRatio>, |a| {
                Message::Apply(Operation::CropAspect(a))
            })
            .placeholder("Crop to aspect ratio..."),
            row![
                button("Rotate left").on_press_maybe(on(Message::Apply(Operation::Rotate270))),
                button("Rotate right").on_press_maybe(on(Message::Apply(Operation::Rotate90))),
            ]
            .spacing(4),
            row![
                button("Flip H").on_press_maybe(on(Message::Apply(Operation::FlipHorizontal))),
                button("Flip V").on_press_maybe(on(Message::Apply(Operation::FlipVertical))),
                button("Crop to selection").on_press_maybe(
                    self.selection_rect()
                        .and_then(|r| on(Message::Apply(Operation::Crop(r))))
                ),
            ]
            .spacing(4),
            text(format!("Straighten: {:+.1} degrees", self.straighten)).size(12),
            row![
                slider(-15.0..=15.0, self.straighten, Message::StraightenAngle).step(0.1_f32),
                button("Apply").on_press_maybe((self.straighten != 0.).then_some(()).and_then(
                    |_| on(Message::Apply(Operation::Straighten {
                        degrees: self.straighten
                    }))
                )),
            ]
            .spacing(4),
            pick_list(SIZE_PRESETS.to_vec(), None::<SizePreset>, |p| {
                Message::Apply(Operation::FitWithin {
                    max_width: p.1,
                    max_height: p.2,
                })
            })
            .placeholder("Resize for platform..."),
            section("SELECTION AND REDACTION"),
            row![
                pick_list(
                    [Shape::Rectangle, Shape::Ellipse],
                    Some(self.shape),
                    Message::ShapePicked
                ),
                checkbox("Invert (background)", self.invert).on_toggle(Message::InvertSelection)
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
            row(["X", "Y", "W", "H"]
                .into_iter()
                .enumerate()
                .map(|(k, l)| text_input(l, &self.region[k])
                    .on_input(move |v| Message::RegionField(k, v))
                    .width(Length::Fill)
                    .into()))
            .spacing(4),
            pick_list(Method::ALL, Some(self.method), Message::MethodPicked),
        ]
        .spacing(8);
        if matches!(
            self.method,
            Method::Blur | Method::Pixelate | Method::Mosaic
        ) {
            c = c
                .push(text(format!("Strength: {}", self.strength)).size(12))
                .push(slider(2..=64, self.strength, Message::Strength))
                .push(text("Visual concealment can leave recoverable or inferable detail. Use opaque redaction for sensitive information.").size(11));
        } else {
            c = c.push(
                text("Secure redaction replaces every selected pixel and is flattened on export.")
                    .size(11),
            );
        }
        c = c
            .push(
                row![
                    button("Conceal selection  Enter").on_press_maybe(
                        self.selection_rect()
                            .and_then(|_| on(Message::ConcealSelection))
                    ),
                    button("Clear").on_press(Message::ClearSelection)
                ]
                .spacing(4),
            )
            .push(section("HISTORY"))
            .push(
                row![
                    button("Undo  Ctrl+Z").on_press_maybe(undo.then_some(Message::Undo)),
                    button("Redo  Ctrl+Y").on_press_maybe(redo.then_some(Message::Redo))
                ]
                .spacing(4),
            );
        if let Some(e) = e {
            for (label, active) in e.edits.history().into_iter().rev().take(8) {
                c = c.push(text(format!("{} {label}", if active { "*" } else { "-" })).size(11));
            }
        }
        scrollable(c).height(Length::Fill).into()
    }

    fn privacy_panel(&self) -> Element<'_, Message> {
        let mut c = column![
            section("PRIVACY SCAN"),
            pick_list(
                photo_privacy::profiles::BUILTIN.map(String::from).to_vec(),
                Some(self.profile.clone()),
                Message::Profile
            ),
            text(profile_by_name(&self.profile).description).size(11),
            button("Scan photograph  Ctrl+P")
                .on_press_maybe((self.selected.is_some() && !self.busy).then_some(Message::Scan)),
        ]
        .spacing(8);
        let Some(e) = self.current() else {
            return c.into();
        };
        let Some(s) = &e.scan else {
            return c
                .push(
                    text("Not scanned yet. Nothing is uploaded; all detectors run locally.")
                        .size(11),
                )
                .into();
        };
        if s.generation != e.generation {
            c = c.push(
                text("Edits changed since this scan; boxes are hidden until you scan again.")
                    .size(11),
            );
        }
        let mut list = Column::new().spacing(2);
        for (k, (f, d)) in s.report.findings.iter().zip(&s.dismissed).enumerate() {
            let label = format!(
                "{:?}  [{:?}]{}\n{}",
                f.category,
                f.evidence,
                f.value_length
                    .map(|n| format!("  {n} chars"))
                    .unwrap_or_default(),
                f.description
            );
            list = list.push(
                row![
                    button(text(label).size(11))
                        .width(Length::Fill)
                        .style(if self.focused_finding == Some(k) {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(Message::FocusFinding(k)),
                    checkbox("Dismiss", *d)
                        .on_toggle(move |v| Message::Dismiss(k, v))
                        .size(14)
                        .text_size(11)
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            );
        }
        if s.report.findings.is_empty() {
            list = list.push(text("No findings from the available detectors.").size(11));
        }
        c = c
            .push(text(format!("{} finding(s). Confirmed = checksum/structure, Pattern = format match, Contextual = uncertain.", s.report.findings.len())).size(11))
            .push(scrollable(list).height(Length::FillPortion(3)))
            .push(row![pick_list(Method::ALL, Some(self.method), Message::MethodPicked), button("Conceal").on_press_maybe((!self.busy).then_some(Message::ConcealFindings))].spacing(4))
            .push(text("Metadata findings are removed by every export.").size(11))
            .push(section("NOT EXAMINED"));
        let mut missing = Column::new().spacing(2);
        for d in s.report.detectors.iter().filter(|d| !d.available) {
            missing = missing.push(text(format!("- {}", d.note)).size(11));
        }
        missing = missing.push(
            text(format!(
                "{} profile categories had no available detector.",
                s.report.not_examined.len()
            ))
            .size(11),
        );
        c.push(scrollable(missing).height(Length::FillPortion(1)))
            .into()
    }

    fn defense_panel(&self) -> Element<'_, Message> {
        column![
            section("ADVERSARIAL AI PROTECTION"),
            text("Status: unavailable. No validated defense model is installed, so no preset can apply protection.").size(13),
            text("Quick Protection        unavailable (not measured)").font(iced::Font::MONOSPACE).size(12),
            text("Balanced Protection     unavailable (not measured)").font(iced::Font::MONOSPACE).size(12),
            text("Maximum Protection      unavailable (not measured)").font(iced::Font::MONOSPACE).size(12),
            text("Custom Protection       unavailable (not measured)").font(iced::Font::MONOSPACE).size(12),
            horizontal_rule(1),
            text("The engine's joint projected-gradient optimizer and an experimental CLIP encoder objective exist for research (CLI feature experimental-clip). Encoder feature change is not demonstrated resistance to AI editing, face swapping, identity cloning or training.").size(12),
            text("Metadata removal, redaction and watermarks do not prevent AI manipulation. No photograph can be made immune.").size(12),
        ]
        .spacing(8)
        .into()
    }

    fn lab_panel(&self) -> Element<'_, Message> {
        let mut t = column![
            row![
                text("TRANSFORMATION LAB").size(13),
                Space::with_width(Length::Fill),
                button("Run tests").on_press_maybe((!self.busy).then_some(Message::Lab)),
                button("Save report...")
                    .on_press_maybe(self.report.as_ref().map(|_| Message::SaveReport))
            ]
            .spacing(8),
            text("Models: none installed | Compute: CPU | GPU inference: not implemented").size(12),
            horizontal_rule(1),
            text("Test                         PSNR (dB)       SSIM          Time (ms)")
                .font(iced::Font::MONOSPACE)
                .size(12)
        ]
        .spacing(8);
        if let Some(r) = &self.report {
            for m in &r.measurements {
                t = t.push(
                    text(format!(
                        "{:<25} {:>10} {:>12.6} {:>12.1}",
                        m.transformation,
                        m.quality
                            .psnr_db
                            .map(|x| format!("{x:.2}"))
                            .unwrap_or("inf".into()),
                        m.quality.ssim_luma_8x8,
                        m.elapsed_ms
                    ))
                    .font(iced::Font::MONOSPACE)
                    .size(12),
                );
            }
        } else {
            t = t.push(
                text(
                    "No results. Export a copy without geometry edits, select it, then run tests.",
                )
                .size(12),
            );
        }
        t = t.push(text("Metrics measure pixel differences that survive transforms. AI resistance is not evaluated.").size(11)).push(section("PROCESSING LOG"));
        let logs = self.logs.iter().fold(Column::new().spacing(2), |c, s| {
            c.push(text(s).font(iced::Font::MONOSPACE).size(11))
        });
        t.push(scrollable(logs).height(Length::Fill)).into()
    }

    fn history_panel(&self) -> Element<'_, Message> {
        let mut c = column![section("PROTECTION HISTORY (this session)")].spacing(4);
        for e in &self.entries {
            let state = match &e.export {
                Some(ExportState::Done { path, passed, .. }) => format!(
                    "exported to {} ({})",
                    short_name(path),
                    if *passed {
                        "verified"
                    } else {
                        "accepted findings"
                    }
                ),
                Some(ExportState::Blocked(_)) => "export blocked by verification".into(),
                None => "not exported".into(),
            };
            c = c.push(
                text(format!(
                    "{}: {} edit(s), {state}",
                    short_name(&e.path),
                    e.edits.active().len()
                ))
                .size(12),
            );
            for (l, active) in e.edits.history() {
                c = c.push(text(format!("    {} {l}", if active { "*" } else { "-" })).size(11));
            }
        }
        c = c.push(text("History is kept in memory only; nothing about your photographs is stored on disk by the application.").size(11));
        scrollable(c).height(Length::Fill).into()
    }

    fn models_panel(&self) -> Element<'_, Message> {
        column![
            section("MODEL LIBRARY"),
            text("Installed models: none").size(13),
            text("Face, person, object, document and OCR detectors require verified local models. None are bundled because their licences and redistribution terms have not been cleared (see docs/research/MODEL_LICENSES.md).").size(12),
            text("Only SafeTensors files with an independently reviewed manifest (SHA-256, size, code and weight licences) are accepted: photo-defense verify-model MODEL MANIFEST. Pickle and executable formats are refused.").size(12),
            text("Downloads: not implemented. The application makes no network requests.").size(12),
        ]
        .spacing(8)
        .into()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        column![
            section("SETTINGS"),
            row![text("Appearance").width(120), pick_list([Appearance::System, Appearance::Light, Appearance::Dark], Some(self.appearance), Message::Appearance)].spacing(8).align_y(iced::Alignment::Center),
            row![text("Interface mode").width(120), pick_list(Mode::ALL, Some(self.mode), Message::Mode)].spacing(8).align_y(iced::Alignment::Center),
            row![text("Collection width").width(120), slider(160.0..=360.0, self.panel_width, Message::PanelWidth).width(200)].spacing(8).align_y(iced::Alignment::Center),
            text("Network: none. Telemetry: none. Accounts: none. Settings are not persisted between sessions in this version.").size(12),
        ]
        .spacing(10)
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        let menu = row![
            button("File")
                .style(button::text)
                .on_press(Message::Menu("File")),
            button("Edit")
                .style(button::text)
                .on_press(Message::Menu("Edit")),
            button("View")
                .style(button::text)
                .on_press(Message::Menu("View")),
            button("Help")
                .style(button::text)
                .on_press(Message::Menu("Help")),
            Space::with_width(Length::Fill),
            pick_list(Mode::ALL, Some(self.mode), Message::Mode),
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center);
        let mut root = column![menu].spacing(6);
        if let Some(m) = self.menu {
            let b = |l: &'static str, msg: Option<Message>| {
                button(text(l).size(12))
                    .style(button::secondary)
                    .on_press_maybe(msg)
            };
            let sub: Element<_> = match m {
                "File" => row![b("Add photographs...  Ctrl+O", (!self.busy).then_some(Message::Open)), b("Export all...  Ctrl+E", (!self.busy).then_some(Message::Export { accept_unresolved: false })), b("Clear collection", (!self.busy).then_some(Message::Clear))].spacing(4).into(),
                "Edit" => row![b("Undo  Ctrl+Z", Some(Message::Undo)), b("Redo  Ctrl+Y", Some(Message::Redo)), b("Privacy scan  Ctrl+P", Some(Message::Scan)), b("Clear selection  Esc", Some(Message::ClearSelection))].spacing(4).into(),
                "View" => row![b("Essential  Ctrl+1", Some(Message::Mode(Mode::Essential))), b("Professional  Ctrl+2", Some(Message::Mode(Mode::Professional))), b("Technical  Ctrl+3", Some(Message::Mode(Mode::Technical)))].spacing(4).into(),
                _ => text("AI Photo Defense 0.2.0 | Development preview | Local processing only | No validated AI protection | Automated scanning never guarantees privacy").size(12).into(),
            };
            root = root.push(sub);
        }
        let tabs = Workspace::for_mode(self.mode)
            .iter()
            .fold(row![].spacing(2), |r, w| {
                r.push(
                    button(text(w.label()).size(12))
                        .style(if *w == self.workspace {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .on_press(Message::Workspace(*w)),
                )
            });
        let toolbar = row![
            tabs,
            Space::with_width(Length::Fill),
            text_input("Path to photo or folder", &self.input_path)
                .on_input(Message::InputPath)
                .on_submit(Message::ImportPath)
                .width(260),
            button(text("Add").size(12))
                .on_press_maybe((!self.busy).then_some(Message::ImportPath)),
            button(text("Cancel").size(12)).on_press_maybe(self.busy.then_some(Message::Cancel)),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        root = root
            .push(horizontal_rule(1))
            .push(toolbar)
            .push(horizontal_rule(1));
        let side = match self.workspace {
            Workspace::Edit => self.edit_panel(),
            Workspace::Privacy => self.privacy_panel(),
            _ => self.export_panel(),
        };
        let centre: Element<_> = match self.workspace {
            Workspace::Edit | Workspace::Privacy => self.canvas(),
            Workspace::Defense => self.defense_panel(),
            Workspace::History => self.history_panel(),
            Workspace::Lab => self.lab_panel(),
            Workspace::Models => self.models_panel(),
            Workspace::Settings => self.settings_panel(),
            Workspace::Export if self.mode == Mode::Essential => self.files(),
            Workspace::Export => self.comparison(),
        };
        let mut body = row![].spacing(10).height(Length::Fill);
        if !(self.workspace == Workspace::Export && self.mode == Mode::Essential) {
            body = body
                .push(container(self.files()).width(self.panel_width))
                .push(vertical_rule(1));
        }
        body = body.push(container(centre).width(Length::Fill));
        if matches!(
            self.workspace,
            Workspace::Export | Workspace::Edit | Workspace::Privacy
        ) {
            body = body.push(vertical_rule(1)).push(container(side).width(330));
        }
        root.push(body)
            .push(horizontal_rule(1))
            .push(row![
                text(&self.status).size(12),
                Space::with_width(Length::Fill),
                text("Local only | No validated AI protection | Originals never overwritten")
                    .size(11)
            ])
            .padding(10)
            .height(Length::Fill)
            .into()
    }
}

fn main() -> iced::Result {
    iced::application("AI Photo Defense", App::update, App::view)
        .subscription(|_| -> Subscription<Message> { iced::event::listen().map(Message::Event) })
        .theme(App::theme)
        .default_font(iced::Font::with_name(if cfg!(target_os = "windows") {
            "Segoe UI"
        } else if cfg!(target_os = "macos") {
            "Helvetica Neue"
        } else {
            "DejaVu Sans"
        }))
        .window_size((1280., 800.))
        .run_with(|| {
            let mut app = App::default();
            let mut paths = Vec::new();
            let mut args = std::env::args_os().skip(1);
            while let Some(arg) = args.next() {
                match arg.to_str() {
                    Some("--workspace") | Some("--mode") => {
                        app.mode = match args.next().and_then(|m| m.into_string().ok()).as_deref() {
                            Some("professional") => Mode::Professional,
                            Some("technical") => Mode::Technical,
                            _ => Mode::Essential,
                        }
                    }
                    Some("--tab") => {
                        app.workspace =
                            match args.next().and_then(|m| m.into_string().ok()).as_deref() {
                                Some("privacy") => Workspace::Privacy,
                                Some("edit") => Workspace::Edit,
                                Some("defense") => Workspace::Defense,
                                Some("history") => Workspace::History,
                                Some("lab") => Workspace::Lab,
                                Some("models") => Workspace::Models,
                                Some("settings") => Workspace::Settings,
                                _ => Workspace::Export,
                            }
                    }
                    Some("--output") => {
                        app.output_directory = args
                            .next()
                            .and_then(|m| m.into_string().ok())
                            .unwrap_or_default()
                    }
                    _ => paths.push(PathBuf::from(arg)),
                }
            }
            let import = app.update(Message::Picked(paths));
            (
                app,
                Task::batch([import, window::get_latest().and_then(window::gain_focus)]),
            )
        })
}
