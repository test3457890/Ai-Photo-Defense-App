//! Three workspaces over one document queue. All image work runs off the UI thread.
use iced::widget::{
    self, Column, Space, button, column, container, horizontal_rule, image, pick_list, row,
    scrollable, slider, text, text_input,
};
use iced::{Element, Event, Length, Subscription, Task, Theme, keyboard, window};
use photo_core::{ExportFormat, Quality};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

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
        write!(f, "{self:?}")
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
#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    preview: image::Handle,
    width: u32,
    height: u32,
}
#[derive(Debug, Clone)]
struct Exported {
    path: PathBuf,
    preview: image::Handle,
    difference: image::Handle,
    quality: Quality,
}
#[derive(Debug, Clone)]
enum Message {
    Open,
    InputPath(String),
    ImportPath,
    OutputDirectory(String),
    Picked(Vec<PathBuf>),
    Imported(Result<Vec<Entry>, String>),
    Select(usize),
    Mode(Mode),
    Appearance(Appearance),
    Format(ExportFormat),
    JpegQuality(u8),
    Export,
    Destination(Option<PathBuf>),
    Exported(Vec<(usize, Result<Exported, String>)>),
    Cancel,
    Clear,
    Lab,
    LabDone(Result<photo_lab::Report, String>),
    SaveReport,
    ReportSaved(Result<(), String>),
    Event(Event),
    Menu(&'static str),
    ToggleInspector,
    PanelWidth(f32),
    Difference(bool),
}
struct App {
    input_path: String,
    output_directory: String,
    entries: Vec<Entry>,
    results: Vec<Option<Exported>>,
    selected: Option<usize>,
    mode: Mode,
    appearance: Appearance,
    format: ExportFormat,
    jpeg_quality: u8,
    busy: bool,
    cancel: Arc<AtomicBool>,
    status: String,
    logs: Vec<String>,
    report: Option<photo_lab::Report>,
    menu: Option<&'static str>,
    inspector: bool,
    panel_width: f32,
    difference: bool,
}
impl Default for App {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            output_directory: String::new(),
            entries: vec![],
            results: vec![],
            selected: None,
            mode: Mode::Essential,
            appearance: Appearance::System,
            format: ExportFormat::Png,
            jpeg_quality: 95,
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            status: "Add photographs to begin. Processing stays on this computer.".into(),
            logs: vec!["Ready. No AI defense model is installed.".into()],
            report: None,
            menu: None,
            inspector: true,
            panel_width: 240.,
            difference: false,
        }
    }
}
fn preview(pixels: &::image::DynamicImage) -> image::Handle {
    let p = pixels.thumbnail(900, 600).to_rgba8();
    image::Handle::from_rgba(p.width(), p.height(), p.into_raw())
}
fn short_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
impl App {
    fn note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.status = msg.clone();
        self.logs.push(msg);
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }
    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::InputPath(path) => self.input_path = path,
            Message::OutputDirectory(path) => self.output_directory = path,
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
            Message::Picked(paths) if !self.busy && !paths.is_empty() => {
                if self.entries.len() + paths.len() > 64 {
                    self.note("Queue limit is 64 images. Export or clear the current collection.");
                    return Task::none();
                }
                self.busy = true;
                self.note("Reading photographs...");
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut entries = vec![];
                            for path in paths {
                                let p = photo_core::load(&path)
                                    .map_err(|e| format!("{}: {e:#}", short_name(&path)))?;
                                entries.push(Entry {
                                    width: p.pixels.width(),
                                    height: p.pixels.height(),
                                    preview: preview(&p.pixels),
                                    path,
                                });
                            }
                            Ok(entries)
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
                    Ok(entries) => {
                        for e in entries {
                            if !self.entries.iter().any(|x| x.path == e.path) {
                                self.entries.push(e);
                                self.results.push(None);
                            }
                        }
                        if self.selected.is_none() && !self.entries.is_empty() {
                            self.selected = Some(0);
                        }
                        self.note(format!(
                            "{} photographs. Originals are preserved.",
                            self.entries.len()
                        ));
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::Select(i) => {
                if i < self.entries.len() {
                    self.selected = Some(i);
                    self.report = None;
                }
            }
            Message::Mode(m) => {
                self.mode = m;
                self.menu = None;
            }
            Message::Appearance(a) => self.appearance = a,
            Message::Format(f) => self.format = f,
            Message::JpegQuality(q) => self.jpeg_quality = q,
            Message::Menu(m) => self.menu = if self.menu == Some(m) { None } else { Some(m) },
            Message::ToggleInspector => self.inspector = !self.inspector,
            Message::PanelWidth(w) => self.panel_width = w,
            Message::Difference(d) => self.difference = d,
            Message::Export if !self.busy && !self.entries.is_empty() => {
                if !self.output_directory.trim().is_empty() {
                    return self.update(Message::Destination(Some(PathBuf::from(
                        self.output_directory.trim(),
                    ))));
                }
                self.note("Choose a folder in the dialog, or enter an output directory in Essential/Professional mode.");
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::Destination,
                );
            }
            Message::Destination(Some(dir)) if !self.busy => {
                if !dir.is_dir() {
                    self.note("Output directory must already exist.");
                    return Task::none();
                }
                self.busy = true;
                self.cancel.store(false, Ordering::Relaxed);
                let cancel = self.cancel.clone();
                let f = self.format;
                let q = self.jpeg_quality;
                let paths: Vec<_> = self.entries.iter().map(|e| e.path.clone()).collect();
                self.note("Exporting metadata-free copies. AI protection is not applied.");
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                    let mut results=vec![];
                    for (i,path) in paths.iter().enumerate() {
                        if cancel.load(Ordering::Relaxed){break;}
                        let work=||->anyhow_result::Result<Exported> {
                            let p=photo_core::load(path)?;let bytes=photo_core::encode(&p.pixels,f,q)?;let decoded=photo_core::decode_export(&bytes)?;
                            let quality=photo_core::quality(&p.pixels,&decoded)?;
                            let out=dir.join(format!("{}-private.{}",photo_core::safe_stem(path)?,f.extension()));
                            let diff=photo_core::difference(&p.pixels,&decoded,8)?;
                            let report=serde_json::json!({"schema_version":1,"operation":"metadata-free re-encode","ai_efficacy":"not evaluated","source_hash":p.fingerprint,"output_hash":photo_core::fingerprint(&bytes),"quality":quality,"warnings":["8-bit; ICC not preserved","First image/frame only"]});
                            photo_core::save_new(&out,&bytes)?;
                            photo_core::save_new(&out.with_extension(format!("{}.json",f.extension())),&serde_json::to_vec_pretty(&report)?)?;
                            Ok(Exported{path:out,preview:preview(&decoded),difference:preview(&diff),quality})
                        };
                        results.push((i,work().map_err(|e|format!("{}: {e:#}",short_name(path)))));
                    }results
                }).await.unwrap_or_else(|e|vec![(0,Err(e.to_string()))])
                    },
                    Message::Exported,
                );
            }
            Message::Exported(results) => {
                self.busy = false;
                let mut good = 0;
                let mut failed = 0;
                for (i, r) in results {
                    match r {
                        Ok(r) => {
                            if i < self.results.len() {
                                self.results[i] = Some(r);
                                good += 1;
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            self.note(e);
                        }
                    }
                }
                self.note(format!(
                    "Export finished: {good} saved, {failed} errors. {}",
                    if self.cancel.load(Ordering::Relaxed) {
                        "Cancelled between files."
                    } else {
                        "See log for details."
                    }
                ));
            }
            Message::Cancel => {
                self.cancel.store(true, Ordering::Relaxed);
                self.note("Cancellation requested. The current file will finish safely.");
            }
            Message::Clear if !self.busy => {
                self.entries.clear();
                self.results.clear();
                self.selected = None;
                self.report = None;
                self.note("Collection cleared.");
            }
            Message::Lab if !self.busy => {
                if let Some(i) = self.selected {
                    if let Some(exported) = &self.results[i] {
                        let a = self.entries[i].path.clone();
                        let b = exported.path.clone();
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
                    } else {
                        self.note(
                            "Export the selected image before running the transformation lab.",
                        );
                    }
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
                let report = self.report.clone().unwrap();
                return Task::perform(
                    async move {
                        if let Some(f) = rfd::AsyncFileDialog::new()
                            .set_file_name("lab-report.json")
                            .save_file()
                            .await
                        {
                            photo_core::save_new(
                                f.path(),
                                &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())
                        } else {
                            Ok(())
                        }
                    },
                    Message::ReportSaved,
                );
            }
            Message::ReportSaved(r) => match r {
                Ok(()) => {
                    self.note("Report dialog finished. Existing files are never overwritten.")
                }
                Err(e) => self.note(e),
            },
            Message::Event(Event::Window(window::Event::FileDropped(path))) => {
                return self.update(Message::Picked(vec![path]));
            }
            Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
                key, modifiers, ..
            })) => {
                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character("o") => return self.update(Message::Open),
                        keyboard::Key::Character("e") => return self.update(Message::Export),
                        keyboard::Key::Character("1") => self.mode = Mode::Essential,
                        keyboard::Key::Character("2") => self.mode = Mode::Professional,
                        keyboard::Key::Character("3") => self.mode = Mode::Technical,
                        _ => {}
                    }
                }
                if key == keyboard::Key::Named(keyboard::key::Named::Tab) {
                    return if modifiers.shift() {
                        widget::focus_previous()
                    } else {
                        widget::focus_next()
                    };
                }
                if key == keyboard::Key::Named(keyboard::key::Named::Escape) {
                    self.menu = None;
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
    fn files(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(2);
        for (i, e) in self.entries.iter().enumerate() {
            list = list.push(
                button(column![
                    text(short_name(&e.path)).size(13),
                    text(format!(
                        "{} x {}  |  {}",
                        e.width,
                        e.height,
                        if self.results[i].is_some() {
                            "Exported"
                        } else {
                            "Ready"
                        }
                    ))
                    .size(11)
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
            list = list.push(
                text("Drop JPEG, PNG, WebP or TIFF files here, or choose Add photographs.")
                    .size(14),
            );
        }
        scrollable(list).height(Length::Fill).into()
    }
    fn controls(&self) -> Element<'_, Message> {
        let mut c = column![
            text("EXPORT SETTINGS").size(12),
            horizontal_rule(1),
            text("Output directory").size(12),
            text_input("Existing folder, or blank to browse", &self.output_directory)
                .on_input(Message::OutputDirectory),
            text("Format").size(12),
            pick_list(ExportFormat::ALL, Some(self.format), Message::Format),
            text(format!("JPEG quality: {}", self.jpeg_quality)).size(12),
            slider(1..=100, self.jpeg_quality, Message::JpegQuality),
            text("Source GPS, EXIF, XMP and ICC are removed. Orientation is applied before export.").size(12),
            text("8-bit output. ICC color management is pending. Multi-frame files export the first frame only.").size(12),
            horizontal_rule(1),
            text("AI defenses: unavailable").size(13),
            text("Metadata removal does not prevent AI editing or training.").size(12),
        ].spacing(10);
        if let Some(i) = self.selected
            && let Some(r) = &self.results[i]
        {
            c = c
                .push(horizontal_rule(1))
                .push(text("MEASURED EXPORT QUALITY").size(12))
                .push(
                    text(format!(
                        "PSNR: {}\nSSIM: {:.6}\nMax channel delta: {}",
                        r.quality
                            .psnr_db
                            .map(|x| format!("{x:.2} dB"))
                            .unwrap_or("infinite (identical RGB)".into()),
                        r.quality.ssim_luma_8x8,
                        r.quality.max_channel_delta
                    ))
                    .size(12),
                );
        }
        c.push(Space::with_height(Length::Fill))
            .push(button("Export copies...").on_press_maybe(
                (!self.busy && !self.entries.is_empty()).then_some(Message::Export),
            ))
            .into()
    }
    fn comparison(&self) -> Element<'_, Message> {
        if let Some(i) = self.selected {
            let e = &self.entries[i];
            let mut left = column![
                text("ORIGINAL").size(12),
                image::viewer(e.preview.clone())
                    .content_fit(iced::ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .spacing(8)
            .width(Length::FillPortion(1));
            left = left.push(text(short_name(&e.path)).size(12));
            let right: Element<_> = if let Some(r) = &self.results[i] {
                column![
                    text(if self.difference {
                        "ABSOLUTE DIFFERENCE x8"
                    } else {
                        "EXPORTED COPY"
                    })
                    .size(12),
                    image::viewer(if self.difference {
                        r.difference.clone()
                    } else {
                        r.preview.clone()
                    })
                    .content_fit(iced::ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill),
                    widget::checkbox("Show difference x8", self.difference)
                        .on_toggle(Message::Difference)
                ]
                .spacing(8)
                .width(Length::FillPortion(1))
                .into()
            } else {
                container(text("Export a copy to compare decoded pixels.").size(14))
                    .center_x(Length::FillPortion(1))
                    .center_y(Length::Fill)
                    .into()
            };
            row![left, right].spacing(16).height(Length::Fill).into()
        } else {
            container(text("Add a photograph to open the workspace."))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        }
    }
    fn view(&self) -> Element<'_, Message> {
        let menu = row![
            button("File")
                .style(button::text)
                .on_press(Message::Menu("File")),
            button("View")
                .style(button::text)
                .on_press(Message::Menu("View")),
            button("Tools")
                .style(button::text)
                .on_press(Message::Menu("Tools")),
            button("Help")
                .style(button::text)
                .on_press(Message::Menu("Help")),
            Space::with_width(Length::Fill),
            pick_list(Mode::ALL, Some(self.mode), Message::Mode),
            pick_list(
                [Appearance::System, Appearance::Light, Appearance::Dark],
                Some(self.appearance),
                Message::Appearance
            )
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        let mut root = column![menu, horizontal_rule(1)].spacing(8);
        if let Some(m) = self.menu {
            let submenu:Element<_>=match m {
            "File"=>row![button("Add photographs...  Ctrl+O").on_press_maybe((!self.busy).then_some(Message::Open)),button("Export copies...  Ctrl+E").on_press_maybe((!self.busy).then_some(Message::Export)),button("Clear collection").on_press_maybe((!self.busy).then_some(Message::Clear))].spacing(8).into(),
            "View"=>row![button("Essential  Ctrl+1").on_press(Message::Mode(Mode::Essential)),button("Professional  Ctrl+2").on_press(Message::Mode(Mode::Professional)),button("Technical  Ctrl+3").on_press(Message::Mode(Mode::Technical)),button("Toggle inspector").on_press(Message::ToggleInspector)].spacing(8).into(),
            "Tools"=>row![button("Run transformation lab").on_press_maybe((!self.busy).then_some(Message::Lab)),button("Save lab report...").on_press_maybe(self.report.as_ref().map(|_|Message::SaveReport))].spacing(8).into(),
            _=>text("AI Photo Defense 0.1.0 | Research preview | No validated AI defenses | Ctrl+O import, Ctrl+E export, Ctrl+1/2/3 workspace").size(12).into(),
        };
            root = root.push(submenu).push(horizontal_rule(1));
        }
        let toolbar = row![
            button("Add photographs...").on_press_maybe((!self.busy).then_some(Message::Open)),
            button("Clear").on_press_maybe((!self.busy).then_some(Message::Clear)),
            text(format!("{} images", self.entries.len())).size(12),
            Space::with_width(Length::Fill),
            button("Cancel batch").on_press_maybe(self.busy.then_some(Message::Cancel))
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        root = root.push(toolbar).push(
            row![
                text_input("Image path (dialog fallback)", &self.input_path)
                    .on_input(Message::InputPath)
                    .on_submit(Message::ImportPath),
                button("Import path").on_press_maybe((!self.busy).then_some(Message::ImportPath))
            ]
            .spacing(8),
        );
        let body: Element<_> = match self.mode {
            Mode::Essential => row![
                container(self.files()).padding(8).width(Length::Fill),
                container(self.controls()).padding(8).width(280)
            ]
            .spacing(16)
            .height(Length::Fill)
            .into(),
            Mode::Professional => {
                let mut r = row![
                    container(self.files()).padding(8).width(self.panel_width),
                    container(self.comparison()).padding(8).width(Length::Fill)
                ]
                .spacing(8);
                if self.inspector {
                    r = r.push(container(self.controls()).padding(8).width(260));
                }
                column![
                    r.height(Length::Fill),
                    row![
                        text("Browser width").size(11),
                        slider(160.0..=360.0, self.panel_width, Message::PanelWidth).width(180),
                        button(if self.inspector {
                            "Hide inspector"
                        } else {
                            "Show inspector"
                        })
                        .on_press(Message::ToggleInspector)
                    ]
                    .spacing(8)
                ]
                .spacing(8)
                .into()
            }
            Mode::Technical => {
                let mut table = column![
                    row![
                        text("TRANSFORMATION LAB").size(13),
                        Space::with_width(Length::Fill),
                        button("Run tests").on_press_maybe((!self.busy).then_some(Message::Lab)),
                        button("Save report...")
                            .on_press_maybe(self.report.as_ref().map(|_| Message::SaveReport))
                    ]
                    .spacing(8),
                    text("Models: none installed | Compute: CPU | GPU inference: not implemented")
                        .size(12),
                    horizontal_rule(1),
                    text("Test                         PSNR (dB)       SSIM          Time (ms)")
                        .font(iced::Font::MONOSPACE)
                        .size(12)
                ]
                .spacing(8);
                if let Some(report) = &self.report {
                    for m in &report.measurements {
                        table = table.push(
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
                    table = table.push(
                        text("No results. Export a copy, select it, then run tests.").size(13),
                    );
                }
                table=table.push(text("Metrics measure pixel differences after transforms. AI resistance is not evaluated.").size(12)).push(horizontal_rule(1)).push(text("PROCESSING LOG").size(12));
                let logs = self.logs.iter().fold(Column::new().spacing(4), |c, s| {
                    c.push(text(s).font(iced::Font::MONOSPACE).size(11))
                });
                table = table.push(scrollable(logs).height(Length::Fill));
                row![
                    container(self.files()).padding(8).width(210),
                    container(table).padding(8).width(Length::Fill)
                ]
                .spacing(12)
                .height(Length::Fill)
                .into()
            }
        };
        root.push(body).push(horizontal_rule(1)).push(text(&self.status).size(12)).push(text("Research preview: no validated AI protection. Originals are never overwritten.").size(11)).padding(12).height(Length::Fill).into()
    }
}
// Keep error detail in worker results without a public dependency alias in every closure.
mod anyhow_result {
    pub type Result<T> = ResultInner<T, Box<dyn std::error::Error + Send + Sync>>;
    type ResultInner<T, E> = std::result::Result<T, E>;
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
        .window_size((1280., 820.))
        .run_with(|| {
            let mut app = App::default();
            let mut paths = Vec::new();
            let mut args = std::env::args_os().skip(1);
            while let Some(arg) = args.next() {
                if arg == "--workspace" {
                    let m = args.next().unwrap_or_default();
                    app.mode = match m.to_str() {
                        Some("professional") => Mode::Professional,
                        Some("technical") => Mode::Technical,
                        _ => Mode::Essential,
                    };
                } else {
                    paths.push(PathBuf::from(arg));
                }
            }
            let import = app.update(Message::Picked(paths));
            (
                app,
                Task::batch([import, window::get_latest().and_then(window::gain_focus)]),
            )
        })
}
