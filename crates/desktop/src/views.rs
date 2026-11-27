//! Views for the three modes. The photograph is the visual focus; controls are compact and
//! progressively disclosed through collapsible groups and menus.
use crate::app::*;
use crate::theme::{self, FONT, FONT_SMALL, FONT_TITLE};
use crate::widgets::{Overlay, group, label_value, slider_row};
use iced::widget::{
    Column, Space, button, checkbox, column, container, horizontal_rule, horizontal_space, image,
    mouse_area, pane_grid, pick_list, progress_bar, responsive, row, scrollable, slider, stack,
    text, text_input, vertical_rule,
};
use iced::{Alignment, ContentFit, Element, Length, Point};
use photo_edit::{AspectRatio, BlendMode, Operation, Selection};
use photo_log::{Component, Level};

type El<'a> = Element<'a, Message>;

fn t<'a>(s: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(s).size(FONT)
}
fn small<'a>(s: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(s).size(FONT_SMALL)
}
fn btn<'a>(label: &'a str, msg: Option<Message>) -> iced::widget::Button<'a, Message> {
    button(text(label).size(FONT))
        .padding([3, 8])
        .style(theme::tool)
        .on_press_maybe(msg)
}
fn toggle<'a>(label: &'a str, on: bool, msg: Message) -> iced::widget::Button<'a, Message> {
    button(text(label).size(FONT))
        .padding([3, 8])
        .style(if on { theme::tool_active } else { theme::tool })
        .on_press(msg)
}
/// Fixed-height separator for toolbars (a Fill-height rule would stretch the row).
fn sep<'a>() -> El<'a> {
    container(Space::new(1, 18))
        .style(|t: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(theme::line(t))),
            ..Default::default()
        })
        .into()
}
/// Text field with its frame drawn by a container and the text inside a clipped layer.
/// iced 0.13 (tiny-skia) does not clip text-input text to the box once the field loses focus,
/// so long values spilled over neighbouring widgets. Here the border belongs to the outer
/// container and the text is confined to the inner area, so it can never cross the frame.
fn field<'a>(input: iced::widget::TextInput<'a, Message>) -> iced::widget::Container<'a, Message> {
    container(crate::clip::clip(input.padding(0).style(theme::input_bare)))
        .padding([3, 6])
        .style(theme::field_frame)
}
/// Shortens a path from the left so its most specific part stays visible.
fn ellipsize_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        format!("...{}", s.chars().skip(n - (max - 3)).collect::<String>())
    }
}
fn mib(b: u64) -> String {
    if b < 1 << 20 {
        format!("{:.1} KiB", b as f64 / 1024.)
    } else if b >= 1 << 30 {
        format!("{:.2} GiB", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MiB", b as f64 / 1_048_576.)
    }
}

pub fn view(app: &App) -> El<'_> {
    let mut body = column![menubar(app)];
    if let Some(p) = &app.autosave_offer {
        body = body.push(
            container(
                row![
                    t(format!("Recovered work from your last session ({} photographs with edits). Restore it?", p.entries.len())),
                    horizontal_space(),
                    button(t("Restore")).padding([3, 10]).style(theme::primary).on_press(Message::RestoreAutosave),
                    btn("Discard", Some(Message::DiscardAutosave)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
            .style(theme::banner),
        );
    } else if app.unclean_shutdown {
        body = body.push(container(small("The previous session ended unexpectedly. No autosaved work was found. A sanitized crash note is in the Diagnostics Center.")).padding([4, 10]).style(theme::banner));
    }
    if app.safe_mode {
        body = body.push(
            container(small(
                "Safe mode: catalog, autosave restore and caches are disabled for this session.",
            ))
            .padding([4, 10])
            .style(theme::warning_box),
        );
    }
    let content: El = match app.mode {
        Mode::Essential => essential(app),
        Mode::Professional if app.fullscreen => viewer(app),
        Mode::Professional => professional(app),
        Mode::Technical => technical(app),
    };
    body = body
        .push(container(content).height(Length::Fill))
        .push(statusbar(app));
    let base: El = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    match app.menu {
        Some(m) => stack![
            base,
            mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::MenuClose),
            menu_popup(app, m)
        ]
        .into(),
        None => base,
    }
}

// ------------------------------------------------------------------ chrome
fn menubar(app: &App) -> El<'_> {
    let m = |label: &'static str, menu: Menu| {
        button(text(label).size(FONT))
            .padding([4, 10])
            .style(if app.menu == Some(menu) {
                theme::flat_selected
            } else {
                theme::flat
            })
            .on_press(Message::MenuToggle(menu))
    };
    let modes = Mode::ALL.iter().fold(row![].spacing(0), |r, md| {
        r.push(
            button(text(md.to_string()).size(FONT))
                .padding([3, 12])
                .style(if *md == app.mode {
                    theme::tool_active
                } else {
                    theme::tool
                })
                .on_press(Message::Mode(*md)),
        )
    });
    container(
        row![
            m("File", Menu::File),
            m("Edit", Menu::Edit),
            m("Photo", Menu::Photo),
            m("View", Menu::View),
            m("Help", Menu::Help),
            horizontal_space(),
            text(
                app.project_path
                    .as_deref()
                    .map(short)
                    .unwrap_or_else(|| "Untitled session".into())
            )
            .size(FONT_SMALL)
            .style(|t: &iced::Theme| text::Style {
                color: Some(theme::muted(t))
            }),
            Space::with_width(16),
            modes,
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    )
    .padding([2, 6])
    .style(theme::bar)
    .into()
}

fn menu_popup(app: &App, m: Menu) -> El<'_> {
    let has = app.selected.is_some();
    let idle = app.busy.is_none();
    let item = |label: &'static str, key: &'static str, msg: Option<Message>| -> El<'static> {
        button(
            row![
                text(label).size(FONT),
                horizontal_space(),
                text(key).size(FONT_SMALL)
            ]
            .width(Length::Fill),
        )
        .width(260)
        .padding([4, 10])
        .style(theme::flat)
        .on_press_maybe(msg)
        .into()
    };
    let sep = || -> El<'static> { container(horizontal_rule(1)).padding([2, 0]).into() };
    let (offset, items): (f32, Vec<El>) = match m {
        Menu::File => (
            6.,
            vec![
                item(
                    "Add photographs...",
                    "Ctrl+O",
                    idle.then_some(Message::OpenDialog),
                ),
                item(
                    "Add folder...",
                    "Ctrl+Shift+O",
                    idle.then_some(Message::OpenFolderDialog),
                ),
                sep(),
                item("Open project...", "", idle.then_some(Message::OpenProject)),
                item("Save project", "Ctrl+S", Some(Message::SaveProject)),
                sep(),
                item(
                    "Export selected...",
                    "Ctrl+E",
                    (has && idle).then_some(Message::Export {
                        accept_unresolved: false,
                        all: false,
                    }),
                ),
                item(
                    "Export all...",
                    "Ctrl+Shift+E",
                    idle.then_some(Message::Export {
                        accept_unresolved: false,
                        all: true,
                    }),
                ),
                sep(),
                item("Clear collection", "", idle.then_some(Message::ClearAll)),
            ],
        ),
        Menu::Edit => (
            48.,
            vec![
                item("Undo", "Ctrl+Z", has.then_some(Message::Undo)),
                item("Redo", "Ctrl+Y", has.then_some(Message::Redo)),
                sep(),
                item("Clear selection", "Esc", Some(Message::Escape)),
                item("Reset active layer", "", has.then_some(Message::ResetLayer)),
            ],
        ),
        Menu::Photo => (
            88.,
            vec![
                item("Privacy scan", "Ctrl+P", has.then_some(Message::Scan)),
                item(
                    "Conceal located findings",
                    "",
                    has.then_some(Message::ConcealFindings),
                ),
                sep(),
                item("Auto tone", "", has.then_some(Message::AutoTone)),
                item(
                    "Rotate left",
                    "",
                    has.then_some(Message::Apply(Operation::Rotate270)),
                ),
                item(
                    "Rotate right",
                    "",
                    has.then_some(Message::Apply(Operation::Rotate90)),
                ),
                item(
                    "Flip horizontal",
                    "",
                    has.then_some(Message::Apply(Operation::FlipHorizontal)),
                ),
                sep(),
                item(
                    "Rate 5 stars",
                    "5",
                    app.selected.map(|i| Message::Rating(i, 5)),
                ),
                item(
                    "Clear rating",
                    "0",
                    app.selected.map(|i| Message::Rating(i, 0)),
                ),
                item("Toggle flag", "", app.selected.map(Message::Flag)),
            ],
        ),
        Menu::View => (
            136.,
            vec![
                item(
                    "Essential mode",
                    "Ctrl+1",
                    Some(Message::Mode(Mode::Essential)),
                ),
                item(
                    "Professional mode",
                    "Ctrl+2",
                    Some(Message::Mode(Mode::Professional)),
                ),
                item(
                    "Technical mode",
                    "Ctrl+3",
                    Some(Message::Mode(Mode::Technical)),
                ),
                sep(),
                item(
                    "Before / after",
                    "\\",
                    Some(Message::View(if app.view == ViewMode::Before {
                        ViewMode::Edited
                    } else {
                        ViewMode::Before
                    })),
                ),
                item(
                    "Side by side",
                    "Y",
                    Some(Message::View(if app.view == ViewMode::Split {
                        ViewMode::Edited
                    } else {
                        ViewMode::Split
                    })),
                ),
                item("Clipping warnings", "J", Some(Message::Clipping)),
                item("Rule-of-thirds guide", "O", Some(Message::Thirds)),
                item(
                    "Full screen viewer",
                    "F / F11",
                    Some(Message::Fullscreen(!app.fullscreen)),
                ),
                sep(),
                item(
                    "Maximize viewer pane",
                    "",
                    Some(Message::PaneMaximize(Pane::Viewer)),
                ),
                item("Reset panel layout", "", Some(Message::ResetLayout)),
                item(
                    "Graphite theme",
                    "",
                    Some(Message::Appearance(theme::Appearance::Graphite)),
                ),
                item(
                    "Paper theme",
                    "",
                    Some(Message::Appearance(theme::Appearance::Paper)),
                ),
            ],
        ),
        Menu::Help => (
            178.,
            vec![
                item("Keyboard shortcuts", "", Some(Message::MenuClose)),
                item(
                    "Diagnostics Center",
                    "",
                    Some(Message::Page(TechPage::Diagnostics)),
                ),
                item("Copy diagnostic report", "", Some(Message::CopyReport)),
            ],
        ),
    };
    let mut col = Column::with_children(items).spacing(0).width(260);
    if m == Menu::Help {
        col = col.push(container(small(
            "Tools: I inspect, C crop, M rect, E ellipse, P polygon (Enter closes), L lasso, B brush, G gradient, R radial.\nEnter applies, Esc cancels, \\ before/after, Y side by side, J clipping, O thirds, 0-5 rating, arrows navigate.\nAI Photo Defense development preview. Processing is local. No validated AI protection.",
        )).padding(10).width(260));
    }
    container(container(col).style(theme::menu_popup).padding(2))
        .padding(iced::Padding {
            top: 28.,
            left: offset,
            right: 0.,
            bottom: 0.,
        })
        .into()
}

fn statusbar(app: &App) -> El<'_> {
    let mut r = row![small(app.status.as_str()).width(Length::Fill)]
        .spacing(14)
        .align_y(Alignment::Center);
    if let Some(b) = &app.busy {
        r = r.push(
            row![
                small(b.as_str()),
                progress_bar(0.0..=1.0, 0.5).width(80).height(6)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        r = r.push(
            button(small("Cancel"))
                .padding([1, 6])
                .style(theme::tool)
                .on_press(Message::Cancel),
        );
    }
    if let Some((x, y, p)) = app.pixel_probe
        && app.mode == Mode::Professional
    {
        r = r.push(
            small(format!("{x},{y}  R{} G{} B{}", p[0], p[1], p[2])).font(iced::Font::MONOSPACE),
        );
    }
    if app.mode == Mode::Technical {
        if let Some(c) = app.perf.cpu {
            r = r.push(small(format!("CPU {c:.0}%")));
        }
        if let (Some(u), Some(tot)) = (app.perf.mem_used, app.perf.mem_total) {
            r = r.push(small(format!("RAM {} / {}", mib(u), mib(tot))));
        }
    }
    r = r.push(
        small("Local processing").style(|t: &iced::Theme| text::Style {
            color: Some(theme::muted(t)),
        }),
    );
    container(r).padding([3, 10]).style(theme::bar).into()
}

// ------------------------------------------------------------------ Essential
fn essential(app: &App) -> El<'_> {
    let idle = app.busy.is_none();
    let aspects: Vec<String> = std::iter::once("No crop".to_string())
        .chain(AspectRatio::PRESETS[1..].iter().map(|a| a.to_string()))
        .collect();
    let chosen = app
        .essential_aspect
        .map(|a| a.to_string())
        .unwrap_or("No crop".into());
    let toolbar = row![
        btn("Add photos...", idle.then_some(Message::OpenDialog)),
        btn("Add folder...", idle.then_some(Message::OpenFolderDialog)),
        sep(),
        small("Profile"),
        pick_list(
            photo_privacy::profiles::BUILTIN.map(String::from).to_vec(),
            Some(app.profile.clone()),
            Message::Profile
        )
        .text_size(FONT)
        .padding([3, 6]),
        small("Crop"),
        pick_list(aspects, Some(chosen), |s: String| Message::EssentialAspect(
            AspectRatio::PRESETS
                .iter()
                .copied()
                .find(|a| a.to_string() == s)
        ))
        .text_size(FONT)
        .padding([3, 6]),
        checkbox("Redact QR/barcodes", app.auto_redact)
            .on_toggle(Message::AutoRedact)
            .size(14)
            .text_size(FONT),
        horizontal_space(),
        pick_list(
            photo_core::ExportFormat::ALL,
            Some(app.format),
            Message::Format
        )
        .text_size(FONT)
        .padding([3, 6]),
        // A label, not a text field: iced 0.13 on tiny-skia can paint a long unfocused
        // text-field value over neighbouring widgets. Typed entry is in Professional > Export.
        button(small(if app.output_dir.trim().is_empty() {
            "Choose output folder...".to_string()
        } else {
            ellipsize_left(app.output_dir.trim(), 36)
        }))
        .width(260)
        .padding([4, 8])
        .style(theme::tool)
        .on_press(Message::BrowseOutput),
        button(text("Process all").size(FONT))
            .padding([3, 14])
            .style(theme::primary)
            .on_press_maybe(
                (idle && !app.entries.is_empty()).then_some(Message::Export {
                    accept_unresolved: false,
                    all: true
                })
            ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let header = row![
        small("").width(18),
        small("File").width(Length::FillPortion(4)),
        small("Dimensions").width(110),
        small("Size").width(80),
        small("Privacy").width(Length::FillPortion(2)),
        small("Result").width(Length::FillPortion(2))
    ]
    .spacing(8);
    let mut rows = Column::new().spacing(0);
    for (i, e) in app.entries.iter().enumerate() {
        let (dot, findings) = match &e.scan {
            Some(s) if s.report.findings.is_empty() => ("-", "No findings".to_string()),
            Some(s) => ("!", format!("{} finding(s)", s.report.findings.len())),
            None => ("·", "Not scanned".into()),
        };
        let line = row![
            small(dot).width(18),
            small(e.name.as_str()).width(Length::FillPortion(4)),
            small(format!("{} x {}", e.width, e.height)).width(110),
            small(mib(e.bytes)).width(80),
            small(findings).width(Length::FillPortion(2)),
            small(e.status.as_str()).width(Length::FillPortion(2)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        rows = rows.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .style(if app.selected == Some(i) {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::Select(i)),
        );
    }
    for (i, m) in app.media.iter().enumerate() {
        let kind = format!("{:?}", m.kind());
        let line = row![
            small(if m.info.as_ref().is_some_and(|i| !i.findings.is_empty()) {
                "!"
            } else {
                "-"
            })
            .width(18),
            small(m.name.as_str()).width(Length::FillPortion(4)),
            small(format!(
                "{kind} {}{}",
                m.info.as_ref().map(|i| i.container.as_str()).unwrap_or(""),
                m.info
                    .as_ref()
                    .and_then(|i| i.duration_s)
                    .map(|d| format!(" · {d:.1} s"))
                    .unwrap_or_default()
            ))
            .width(110),
            small(mib(m.bytes)).width(80),
            small(match m.info.as_ref().map(|i| i.findings.len()) {
                None => "Not inspected".to_string(),
                Some(0) => "No findings".to_string(),
                Some(n) => format!("{n} finding(s)"),
            })
            .width(Length::FillPortion(2)),
            small(m.status.as_str()).width(Length::FillPortion(2)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        rows = rows.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .style(if app.focus_media && app.selected_media == Some(i) {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::SelectMedia(i)),
        );
    }
    let list: El = if app.entries.is_empty() && app.media.is_empty() {
        container(column![t("Drop photographs or folders here"), small("Photographs (JPEG, PNG, WebP, TIFF, GIF, BMP, camera RAW preview), audio (WAV, FLAC, MP3, AAC/M4A, Ogg Vorbis), video (MP4, MOV, 3GP metadata removal; MKV/WebM inspection) and SRT/WebVTT subtitles. Originals are never modified.")].spacing(4).align_x(Alignment::Center)).center(Length::Fill).into()
    } else {
        scrollable(rows).height(Length::Fill).into()
    };
    let blocked = app
        .entries
        .iter()
        .filter(|e| matches!(e.export, Some(ExportState::Blocked(_))))
        .count();
    let mut col = column![
        container(toolbar).padding([6, 10]).style(theme::bar),
        container(header)
            .padding([4, 18])
            .style(theme::section_header),
        container(list).padding([0, 10]).height(Length::Fill)
    ];
    if blocked > 0 {
        col = col.push(
            container(
                row![
                    t(format!("{blocked} photograph(s) were blocked by final verification (unredacted findings or uncertain matches). Review them in Professional mode, or confirm you reviewed them.")).width(Length::Fill),
                    btn("Open in Professional", Some(Message::Mode(Mode::Professional))),
                    button(text("Export anyway").size(FONT)).padding([3, 8]).style(theme::danger).on_press_maybe(idle.then_some(Message::Export { accept_unresolved: true, all: true })),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(8)
            .style(theme::warning_box),
        );
    }
    let mut r = row![col.width(Length::Fill)];
    if let Some(m) = app
        .focus_media
        .then(|| app.selected_media.and_then(|i| app.media.get(i)))
        .flatten()
    {
        r = r.push(vertical_rule(1)).push(
            container(scrollable(media_panel(app, m, true)))
                .padding(10)
                .width(380)
                .style(theme::panel),
        );
    } else if let Some(e) = app.current() {
        let preview: El = match &e.edited {
            Some(ed) => image(ed.handle.clone())
                .width(Length::Fill)
                .content_fit(ContentFit::Contain)
                .into(),
            None => image(e.thumb.clone())
                .width(Length::Fill)
                .content_fit(ContentFit::Contain)
                .into(),
        };
        let mut side = column![preview, t(e.name.as_str())].spacing(6);
        if let Some(s) = &e.scan {
            for f in s.report.findings.iter().take(6) {
                side = side.push(small(format!("{:?} [{:?}]", f.category, f.evidence)));
            }
            side = side.push(small(format!(
                "Not examined: {} categories (no model installed)",
                s.report.not_examined.len()
            )));
        }
        side = side.push(small("Metadata (EXIF, GPS, XMP, IPTC, ICC, thumbnails) is always removed. AI protection: unavailable (no validated model)."));
        side = side.push(
            row![
                btn(
                    "Edit in Professional mode",
                    Some(Message::Mode(Mode::Professional))
                ),
                btn("Remove", app.selected.map(Message::Remove))
            ]
            .spacing(4),
        );
        r = r.push(vertical_rule(1)).push(
            container(scrollable(side))
                .padding(10)
                .width(280)
                .style(theme::panel),
        );
    }
    r.into()
}

// ------------------------------------------------------------------ Professional
fn professional(app: &App) -> El<'_> {
    pane_grid(&app.panes, |_, kind, maximized| {
        let (title, body): (&str, El) = match kind {
            Pane::Library => ("LIBRARY", library(app)),
            Pane::Viewer => ("", viewer(app)),
            Pane::Inspector => ("", inspector(app)),
            Pane::Filmstrip => ("", filmstrip(app)),
        };
        let mut c =
            pane_grid::Content::new(container(body).width(Length::Fill).height(Length::Fill))
                .style(if *kind == Pane::Viewer {
                    theme::viewer
                } else {
                    theme::panel
                });
        if !title.is_empty() || *kind == Pane::Viewer {
            let controls = button(small(if maximized { "Restore" } else { "Maximize" }))
                .padding([1, 6])
                .style(theme::flat)
                .on_press(Message::PaneMaximize(*kind));
            let bar = pane_grid::TitleBar::new(small(if *kind == Pane::Viewer {
                "VIEWER"
            } else {
                title
            }))
            .controls(pane_grid::Controls::new(controls))
            .padding([3, 8])
            .style(theme::section_header);
            c = c.title_bar(bar);
        }
        c
    })
    .on_resize(6, Message::PaneResized)
    .on_drag(Message::PaneDragged)
    .spacing(1)
    .into()
}

fn library(app: &App) -> El<'_> {
    let mut col = column![
        row![
            btn("Add...", app.busy.is_none().then_some(Message::OpenDialog)),
            btn(
                "Folder...",
                app.busy.is_none().then_some(Message::OpenFolderDialog)
            )
        ]
        .spacing(4),
        row![
            field(
                text_input("Path to photo or folder", &app.input_path)
                    .on_input(Message::InputPath)
                    .on_submit(Message::ImportPath)
                    .size(FONT)
                    .padding([3, 6])
                    .style(theme::input)
            ),
            btn("Add", app.busy.is_none().then_some(Message::ImportPath))
        ]
        .spacing(4),
        small("COLLECTION"),
    ]
    .spacing(6)
    .padding(8);
    let mut list = Column::new().spacing(0);
    for (i, e) in app.entries.iter().enumerate() {
        let stars = "*".repeat(e.rating as usize);
        list = list.push(
            button(column![
                small(e.name.as_str()),
                small(format!(
                    "{} x {}  {}{}",
                    e.width,
                    e.height,
                    stars,
                    if e.flagged { "  [F]" } else { "" }
                ))
            ])
            .width(Length::Fill)
            .padding([3, 6])
            .style(if app.selected == Some(i) {
                theme::flat_selected
            } else {
                theme::flat
            })
            .on_press(Message::Select(i)),
        );
    }
    col = col.push(scrollable(list).height(Length::FillPortion(3)));
    if !app.media.is_empty() {
        let mut ml = Column::new().spacing(0);
        for (i, m) in app.media.iter().enumerate() {
            ml = ml.push(
                button(column![
                    small(m.name.as_str()),
                    small(format!("{:?} · {}", m.kind(), m.status))
                ])
                .width(Length::Fill)
                .padding([3, 6])
                .style(if app.focus_media && app.selected_media == Some(i) {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::SelectMedia(i)),
            );
        }
        col = col
            .push(horizontal_rule(1))
            .push(small("MEDIA"))
            .push(scrollable(ml).height(Length::FillPortion(1)));
    }
    col = col.push(horizontal_rule(1)).push(small("CATALOG"));
    if app.catalog.is_none() {
        col = col.push(small("Catalog unavailable (safe mode or database error)."));
    } else {
        col = col.push(field(
            text_input("Search name, camera, tag", &app.lib_search)
                .on_input(Message::LibSearch)
                .size(FONT)
                .padding([3, 6])
                .style(theme::input),
        ));
        col = col.push(
            row![
                small("Min rating"),
                pick_list(
                    [0u8, 1, 2, 3, 4, 5],
                    Some(app.lib_rating),
                    Message::LibRating
                )
                .text_size(FONT)
                .padding([2, 6])
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        let mut rows = Column::new().spacing(0);
        for r in app.lib_rows.iter().take(300) {
            let open = app
                .entries
                .iter()
                .position(|e| e.path == r.path)
                .map(Message::Select)
                .unwrap_or_else(|| Message::Picked(vec![r.path.clone()]));
            rows = rows.push(
                button(column![
                    small(short(&r.path)),
                    small(format!(
                        "{} · {} · {}",
                        r.format,
                        if r.camera.is_empty() {
                            "no camera data"
                        } else {
                            r.camera.as_str()
                        },
                        "*".repeat(r.rating as usize)
                    ))
                ])
                .width(Length::Fill)
                .padding([2, 6])
                .style(theme::flat)
                .on_press(open),
            );
        }
        col = col.push(scrollable(rows).height(Length::FillPortion(2)));
        if app.current().and_then(|e| e.catalog_id).is_some() {
            col = col.push(
                row![
                    field(
                        text_input("Add tag to photo", &app.tag_input)
                            .on_input(Message::TagInput)
                            .on_submit(Message::ApplyTag)
                            .size(FONT)
                            .padding([3, 6])
                            .style(theme::input)
                    ),
                    btn("Tag", Some(Message::ApplyTag))
                ]
                .spacing(4),
            );
        }
    }
    col.into()
}

fn tool_bar(app: &App) -> El<'_> {
    let tools = Tool::ALL.iter().fold(row![].spacing(2), |r, tl| {
        r.push(
            button(text(tl.label()).size(FONT_SMALL))
                .padding([3, 7])
                .style(if app.tool == *tl {
                    theme::tool_active
                } else {
                    theme::tool
                })
                .on_press(Message::Tool(*tl)),
        )
    });
    let views = row![
        toggle(
            "Edited",
            app.view == ViewMode::Edited,
            Message::View(ViewMode::Edited)
        ),
        toggle(
            "Before",
            app.view == ViewMode::Before,
            Message::View(ViewMode::Before)
        ),
        toggle(
            "Split",
            app.view == ViewMode::Split,
            Message::View(ViewMode::Split)
        ),
    ]
    .spacing(2);
    let main = row![
        tools,
        sep(),
        views,
        sep(),
        toggle("Clip", app.clipping, Message::Clipping),
        toggle("Thirds", app.thirds, Message::Thirds)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut col = column![main].spacing(4);
    if app.selection.is_some() || !app.points.is_empty() || app.gradient.is_some() {
        let mut r = row![small(match app.tool {
            Tool::Linear | Tool::Radial => "Gradient mask ready:",
            Tool::Crop => "Crop area:",
            Tool::Polygon if app.selection.is_none() => "Polygon: click points, Enter closes.",
            _ => "Selection:",
        })]
        .spacing(6)
        .align_y(Alignment::Center);
        if matches!(app.tool, Tool::Linear | Tool::Radial) {
            r = r.push(
                button(text("Add masked adjustment  Enter").size(FONT_SMALL))
                    .padding([3, 8])
                    .style(theme::primary)
                    .on_press(Message::NewLayer),
            );
        } else if app.tool == Tool::Crop {
            r = r.push(
                button(text("Apply crop  Enter").size(FONT_SMALL))
                    .padding([3, 8])
                    .style(theme::primary)
                    .on_press(Message::Enter),
            );
        } else if app.selection.is_some() {
            r = r.push(
                button(text(format!("Redact ({})  Enter", app.method)).size(FONT_SMALL))
                    .padding([3, 8])
                    .style(theme::primary)
                    .on_press(Message::ConcealSelection),
            );
            r = r.push(
                button(text("Use as adjustment mask").size(FONT_SMALL))
                    .padding([3, 8])
                    .style(theme::tool)
                    .on_press(Message::NewLayer),
            );
        }
        r = r.push(
            button(text("Cancel  Esc").size(FONT_SMALL))
                .padding([3, 8])
                .style(theme::tool)
                .on_press(Message::Escape),
        );
        col = col.push(r);
    }
    container(col)
        .padding([4, 6])
        .style(theme::bar)
        .width(Length::Fill)
        .into()
}

pub fn viewer(app: &App) -> El<'_> {
    if let Some(m) = app
        .focus_media
        .then(|| app.selected_media.and_then(|i| app.media.get(i)))
        .flatten()
    {
        return container(scrollable(media_panel(app, m, false)))
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let Some(e) = app.current() else {
        return column![
            tool_bar(app),
            container(
                column![
                    t("No photograph selected"),
                    small(
                        "Add photographs with File > Add photographs (Ctrl+O) or drop files here."
                    )
                ]
                .spacing(4)
                .align_x(Alignment::Center)
            )
            .center(Length::Fill)
        ]
        .into();
    };
    let Some(ed) = e.edited.clone() else {
        return column![
            tool_bar(app),
            container(small("Rendering preview...")).center(Length::Fill)
        ]
        .into();
    };
    let info = row![
        small(e.name.as_str()),
        small(format!("{} x {}", ed.full.0, ed.full.1)),
        small(
            e.raw
                .as_ref()
                .map(|k| format!("RAW {k} (embedded camera preview)"))
                .unwrap_or_default()
        ),
        horizontal_space(),
        small(format!("preview {:.0} ms", ed.ms)),
    ]
    .spacing(12);
    let canvas_area = responsive(move |size| {
        let (fw, fh) = (ed.full.0 as f32, ed.full.1 as f32);
        let split = app.view == ViewMode::Split;
        let avail_w = if split {
            (size.width - 8.) / 2.
        } else {
            size.width
        };
        let s = (avail_w / fw).min(size.height / fh).clamp(0.01, 8.);
        let (dw, dh) = ((fw * s).floor().max(1.), (fh * s).floor().max(1.));
        if split {
            let bw = (dw * e.width as f32 / fw).min(avail_w);
            return container(
                row![
                    image(ed.before.clone())
                        .width(bw)
                        .height(dh)
                        .content_fit(ContentFit::Contain),
                    image(ed.handle.clone())
                        .width(dw)
                        .height(dh)
                        .content_fit(ContentFit::Fill)
                ]
                .spacing(8),
            )
            .center(Length::Fill)
            .into();
        }
        let handle = if app.view == ViewMode::Before {
            ed.before.clone()
        } else {
            ed.handle.clone()
        };
        let mut ov = Overlay {
            scale: s,
            thirds: app.thirds,
            ..Default::default()
        };
        if let Some(sc) = &e.scan
            && sc.dims == ed.full
            && app.view == ViewMode::Edited
        {
            for (k, (f, d)) in sc.report.findings.iter().zip(&sc.dismissed).enumerate() {
                if let (Some(r), false) = (f.region, *d) {
                    ov.findings.push((
                        r.x as f32,
                        r.y as f32,
                        r.width as f32,
                        r.height as f32,
                        app.focused_finding == Some(k),
                    ));
                }
            }
        }
        match &app.selection {
            Some(Selection::Rect(r)) => {
                ov.rect = Some((
                    r.x as f32,
                    r.y as f32,
                    r.width as f32,
                    r.height as f32,
                    false,
                ))
            }
            Some(Selection::Ellipse(r)) => {
                ov.rect = Some((
                    r.x as f32,
                    r.y as f32,
                    r.width as f32,
                    r.height as f32,
                    true,
                ))
            }
            Some(Selection::Polygon(p)) => {
                ov.points = p.clone();
                ov.closed = true;
            }
            Some(Selection::Brush { points, radius }) => {
                ov.points = points.clone();
                ov.brush_radius = Some(*radius);
            }
            _ => {}
        }
        if app.selection.is_none() && !app.points.is_empty() {
            ov.points = app.points.clone();
            ov.brush_radius = (app.tool == Tool::Brush).then_some(app.brush_radius);
        }
        ov.line = app.gradient;
        let mut layers = stack![
            image(handle)
                .width(dw)
                .height(dh)
                .content_fit(ContentFit::Fill)
        ]
        .width(dw)
        .height(dh);
        if !ov.is_empty() {
            let p = app.theme_accent();
            let img = crate::widgets::overlay_image(&ov, dw as u32, dh as u32, p);
            layers = layers.push(
                image(image::Handle::from_rgba(
                    img.width(),
                    img.height(),
                    img.into_raw(),
                ))
                .width(dw)
                .height(dh)
                .content_fit(ContentFit::Fill),
            );
        }
        let area = mouse_area(layers)
            .on_move(move |p: Point| {
                Message::PointerMove(Point::new((p.x / s).clamp(0., fw), (p.y / s).clamp(0., fh)))
            })
            .on_press(Message::PointerDown)
            .on_release(Message::PointerUp)
            .interaction(if app.tool == Tool::Inspect {
                iced::mouse::Interaction::Idle
            } else {
                iced::mouse::Interaction::Crosshair
            });
        container(area).center(Length::Fill).into()
    });
    let mut col = column![];
    if !app.fullscreen {
        col = col.push(tool_bar(app));
    }
    col.push(container(canvas_area).padding(10).height(Length::Fill))
        .push(container(info).padding([2, 8]).style(theme::bar))
        .into()
}

fn filmstrip(app: &App) -> El<'_> {
    let mut r = row![].spacing(4).padding(6);
    for (i, e) in app.entries.iter().enumerate() {
        let tile = column![
            image(e.thumb.clone())
                .height(84)
                .content_fit(ContentFit::Contain),
            row![
                small("*".repeat(e.rating as usize)),
                horizontal_space(),
                small(if e.flagged { "F" } else { "" }),
                small(
                    if e.scan
                        .as_ref()
                        .is_some_and(|s| !s.report.findings.is_empty())
                    {
                        "!"
                    } else {
                        ""
                    }
                )
            ]
            .width(120),
        ]
        .spacing(2)
        .width(124);
        r = r.push(
            mouse_area(
                button(tile)
                    .padding(2)
                    .style(if app.selected == Some(i) {
                        theme::tool_active
                    } else {
                        theme::flat
                    })
                    .on_press(Message::Select(i)),
            )
            .on_right_press(Message::Flag(i)),
        );
    }
    scrollable(r)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(6).scroller_width(6),
        ))
        .into()
}

fn adj_row<'a>(
    app: &App,
    label: &'a str,
    f: Field,
    range: std::ops::RangeInclusive<f32>,
) -> El<'a> {
    let a = app.layer_adjustments();
    slider_row(
        label,
        field_value(&a, f, app.band),
        range,
        move |v| Message::Adjust(f, v),
        Message::Commit,
    )
}

fn inspector(app: &App) -> El<'_> {
    let Some(e) = app.current() else {
        return container(small(if app.focus_media { "Media files are inspected, redacted and exported in the viewer. Photo adjustments do not apply to audio, video or subtitles." } else { "Select a photograph to edit." }))
            .padding(12)
            .into();
    };
    let mut col = column![].spacing(0);
    // Histogram and info
    let hist = e.edited.as_ref().map(|d| d.hist.clone());
    let hist_img = e.edited.as_ref().map(|d| d.hist_img.clone());
    let clip = hist
        .as_ref()
        .map(|h| {
            format!(
                "Shadows clipped {:.2}%  Highlights clipped {:.2}%",
                h.clipped_shadows as f64 * 100. / h.pixels.max(1) as f64,
                h.clipped_highlights as f64 * 100. / h.pixels.max(1) as f64
            )
        })
        .unwrap_or_default();
    col = col.push(group(
        "Histogram",
        app.group_open("Histogram"),
        Message::Group("Histogram"),
        move || {
            let graph: El = match &hist_img {
                Some(h) => image(h.clone())
                    .width(Length::Fill)
                    .height(90)
                    .content_fit(ContentFit::Fill)
                    .into(),
                None => Space::new(Length::Fill, 90).into(),
            };
            column![
                graph,
                small(clip.clone()),
                row![
                    btn("Auto tone", Some(Message::AutoTone)),
                    toggle("Clipping J", app.clipping, Message::Clipping)
                ]
                .spacing(4)
            ]
            .spacing(6)
            .into()
        },
    ));
    // Layers
    col = col.push(group(
        "Layers",
        app.group_open("Layers"),
        Message::Group("Layers"),
        || layers_panel(app),
    ));
    let layer_name = match app.layer() {
        Some((_, Operation::Adjust { name, .. })) => name.clone(),
        _ => "Basic (created on first change)".into(),
    };
    col = col.push(container(small(format!("Editing layer: {layer_name}"))).padding([4, 8]));
    col = col.push(group(
        "Basic",
        app.group_open("Basic"),
        Message::Group("Basic"),
        || {
            column![
                adj_row(app, "Temperature", Field::Temperature, -1.0..=1.0),
                adj_row(app, "Tint", Field::Tint, -1.0..=1.0),
                adj_row(app, "Exposure", Field::Exposure, -5.0..=5.0),
                adj_row(app, "Contrast", Field::Contrast, -1.0..=1.0),
                adj_row(app, "Highlights", Field::Highlights, -1.0..=1.0),
                adj_row(app, "Shadows", Field::Shadows, -1.0..=1.0),
                adj_row(app, "Whites", Field::Whites, -1.0..=1.0),
                adj_row(app, "Blacks", Field::Blacks, -1.0..=1.0),
                adj_row(app, "Brightness", Field::Brightness, -1.0..=1.0),
                row![btn("Reset layer", Some(Message::ResetLayer))].spacing(4),
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Presence",
        app.group_open("Presence"),
        Message::Group("Presence"),
        || {
            column![
                adj_row(app, "Clarity", Field::Clarity, -1.0..=1.0),
                adj_row(app, "Texture", Field::Texture, -1.0..=1.0),
                adj_row(app, "Dehaze", Field::Dehaze, -1.0..=1.0),
                adj_row(app, "Vibrance", Field::Vibrance, -1.0..=1.0),
                adj_row(app, "Saturation", Field::Saturation, -1.0..=1.0),
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Tone Curve",
        app.group_open("Tone Curve"),
        Message::Group("Tone Curve"),
        || {
            column![
                adj_row(app, "Shadows", Field::CurveShadows, -1.0..=1.0),
                adj_row(app, "Darks", Field::CurveDarks, -1.0..=1.0),
                adj_row(app, "Lights", Field::CurveLights, -1.0..=1.0),
                adj_row(app, "Highlights", Field::CurveHighlights, -1.0..=1.0),
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "HSL",
        app.group_open("HSL"),
        Message::Group("HSL"),
        || {
            let bands = photo_edit::color::HSL_BANDS.iter().enumerate().fold(
                row![].spacing(1),
                |r, (i, b)| {
                    r.push(
                        button(text(&b[..3]).size(FONT_SMALL))
                            .padding([2, 4])
                            .style(if app.band == i {
                                theme::tool_active
                            } else {
                                theme::tool
                            })
                            .on_press(Message::Band(i)),
                    )
                },
            );
            column![
                bands,
                adj_row(app, "Hue", Field::HslHue, -1.0..=1.0),
                adj_row(app, "Saturation", Field::HslSat, -1.0..=1.0),
                adj_row(app, "Luminance", Field::HslLum, -1.0..=1.0)
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Color Grading",
        app.group_open("Color Grading"),
        Message::Group("Color Grading"),
        || {
            column![
                adj_row(app, "Shadow hue", Field::SplitShadowHue, 0.0..=360.0),
                adj_row(app, "Shadow amt", Field::SplitShadowAmount, 0.0..=1.0),
                adj_row(app, "Highlight hue", Field::SplitHighlightHue, 0.0..=360.0),
                adj_row(app, "Highlight amt", Field::SplitHighlightAmount, 0.0..=1.0),
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Detail",
        app.group_open("Detail"),
        Message::Group("Detail"),
        || {
            column![
                adj_row(app, "Sharpening", Field::Sharpness, 0.0..=1.0),
                adj_row(app, "Noise reduc.", Field::Noise, 0.0..=1.0)
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Effects",
        app.group_open("Effects"),
        Message::Group("Effects"),
        || {
            column![
                adj_row(app, "Vignette", Field::Vignette, -1.0..=1.0),
                adj_row(app, "Grain", Field::Grain, 0.0..=1.0)
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Levels",
        app.group_open("Levels"),
        Message::Group("Levels"),
        || {
            column![
                adj_row(app, "Black point", Field::BlackPoint, 0.0..=0.5),
                adj_row(app, "White point", Field::WhitePoint, 0.5..=1.0),
                adj_row(app, "Gamma", Field::Gamma, 0.2..=3.0)
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "LUT",
        app.group_open("LUT"),
        Message::Group("LUT"),
        || {
            let loaded = app
                .layer_adjustments()
                .lut
                .map(|l| format!("{0}x{0}x{0} LUT loaded", l.size))
                .unwrap_or("No LUT".into());
            column![
                small(loaded),
                adj_row(app, "Strength", Field::LutStrength, 0.0..=1.0),
                row![
                    btn("Import .cube...", Some(Message::ImportLut)),
                    btn("Export as .cube", Some(Message::ExportLut))
                ]
                .spacing(4)
            ]
            .spacing(4)
            .into()
        },
    ));
    col = col.push(group(
        "Presets",
        app.group_open("Presets"),
        Message::Group("Presets"),
        || {
            photo_edit::color::presets()
                .into_iter()
                .fold(Column::new().spacing(2), |c, (n, _)| {
                    c.push(
                        button(small(n))
                            .width(Length::Fill)
                            .padding([2, 6])
                            .style(theme::flat)
                            .on_press(Message::Preset(n.to_string())),
                    )
                })
                .into()
        },
    ));
    col = col.push(group(
        "Geometry",
        app.group_open("Geometry"),
        Message::Group("Geometry"),
        || geometry_panel(app),
    ));
    col = col.push(group("Redaction", app.group_open("Redaction"), Message::Group("Redaction"), || {
        let mut c = column![pick_list(Method::ALL, Some(app.method), Message::Method).text_size(FONT).padding([3, 6])].spacing(6);
        if matches!(app.method, Method::Blur | Method::Pixelate | Method::Mosaic) {
            c = c.push(row![small("Strength").width(78), slider(2..=64, app.strength, Message::Strength).height(14), small(app.strength.to_string()).width(40)].spacing(6));
            c = c.push(small("Visual concealment can leave recoverable detail. Use opaque redaction for sensitive information."));
            if app.method.concealment(app.strength).is_weak() {
                c = c.push(container(small("This strength is weak: text underneath may stay readable. Use 8 or more for blur, 12 or more for blocks, or opaque redaction.")).padding(4).style(theme::warning_box));
            }
        }
        c = c.push(row![small("Brush radius").width(78), slider(1.0..=200.0, app.brush_radius, Message::BrushRadius).height(14), small(format!("{:.0}", app.brush_radius)).width(40)].spacing(6));
        c = c.push(btn("Conceal selection  Enter", app.selection.is_some().then_some(Message::ConcealSelection)));
        c.into()
    }));
    col = col.push(group(
        "Privacy Scan",
        app.group_open("Privacy Scan"),
        Message::Group("Privacy Scan"),
        || privacy_panel(app),
    ));
    col = col.push(group(
        "Export",
        app.group_open("Export"),
        Message::Group("Export"),
        || export_panel(app),
    ));
    col = col.push(group(
        "Info",
        app.group_open("Info"),
        Message::Group("Info"),
        || {
            let i = app.selected.unwrap_or(0);
            let stars = (0..=5u8).fold(row![].spacing(1), |r, n| {
                r.push(
                    button(small(if n == 0 {
                        "0".to_string()
                    } else {
                        n.to_string()
                    }))
                    .padding([1, 4])
                    .style(if n > 0 && n <= e.rating {
                        theme::tool_active
                    } else {
                        theme::tool
                    })
                    .on_press(Message::Rating(i, n)),
                )
            });
            column![
                label_value("File", e.name.clone()),
                label_value("Pixels", format!("{} x {}", e.width, e.height)),
                label_value("Size", mib(e.bytes)),
                label_value(
                    "Source",
                    e.raw
                        .clone()
                        .map(|k| format!("RAW {k}: embedded preview only"))
                        .unwrap_or("Raster image".into())
                ),
                row![small("Rating").width(110), stars].spacing(6),
                toggle(
                    if e.flagged { "Flagged" } else { "Flag" },
                    e.flagged,
                    Message::Flag(i)
                ),
            ]
            .spacing(4)
            .into()
        },
    ));
    scrollable(col).height(Length::Fill).into()
}

fn layers_panel(app: &App) -> El<'_> {
    let Some(e) = app.current() else {
        return Space::new(0, 0).into();
    };
    let ops = e.edits.active();
    let mut c = Column::new().spacing(1);
    if ops.is_empty() {
        c = c.push(small("No layers. The original is shown unchanged."));
    }
    for (k, op) in ops.iter().enumerate().rev() {
        let kind = match op {
            Operation::Adjust { .. } => "ADJ",
            Operation::Conceal { method, .. } if method.is_secure_redaction() => "RED",
            Operation::Conceal { .. } => "CON",
            Operation::Fill { .. } => "SHP",
            _ => "GEO",
        };
        let active = app.active_layer == Some(k)
            || (app.active_layer.is_none() && app.layer().map(|l| l.0) == Some(k));
        let mut r = row![
            small(kind).width(28).font(iced::Font::MONOSPACE),
            button(small(op.label()))
                .width(Length::Fill)
                .padding([2, 4])
                .style(if active {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::Layer(Some(k)))
        ]
        .spacing(2)
        .align_y(Alignment::Center);
        if let Operation::Adjust { visible, .. } = op {
            r = r.push(
                button(small(if *visible { "on" } else { "off" }))
                    .padding([1, 4])
                    .style(if *visible { theme::tool } else { theme::flat })
                    .on_press(Message::LayerToggle(k)),
            );
        }
        r = r
            .push(
                button(small("Up"))
                    .padding([1, 4])
                    .style(theme::tool)
                    .on_press(Message::LayerMove(k, false)),
            )
            .push(
                button(small("Dn"))
                    .padding([1, 4])
                    .style(theme::tool)
                    .on_press(Message::LayerMove(k, true)),
            )
            .push(
                button(small("Del"))
                    .padding([1, 4])
                    .style(theme::tool)
                    .on_press(Message::LayerDelete(k)),
            );
        c = c.push(r);
    }
    let mut col = column![
        c,
        row![
            btn("+ Adjustment layer", Some(Message::NewLayer)),
            btn("Undo", e.undo.last().map(|_| Message::Undo)),
            btn("Redo", e.redo.last().map(|_| Message::Redo))
        ]
        .spacing(4)
    ]
    .spacing(6);
    if let Some((_, Operation::Adjust { opacity, blend, .. })) = app.layer() {
        col = col.push(slider_row(
            "Opacity",
            *opacity,
            0.0..=1.0,
            |v| Message::Adjust(Field::Opacity, v),
            Message::Commit,
        ));
        col = col.push(
            row![
                small("Blend").width(78),
                pick_list(BlendMode::ALL, Some(*blend), Message::LayerBlend)
                    .text_size(FONT)
                    .padding([2, 6])
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    col = col.push(small("Redaction layers always render above adjustments and are flattened into exported pixels. Project layers stay reversible."));
    col.into()
}

fn geometry_panel(app: &App) -> El<'_> {
    let sizes = [
        ("Portrait post", 1080, 1350),
        ("Square post", 1080, 1080),
        ("Story", 1080, 1920),
        ("Landscape", 1600, 900),
        ("Full HD", 1920, 1080),
    ];
    let size_names: Vec<String> = sizes
        .iter()
        .map(|(n, w, h)| format!("{n} {w}x{h}"))
        .collect();
    let perspective = match &app.selection {
        Some(Selection::Polygon(p)) if p.len() == 4 => {
            let w = ((p[1].0 - p[0].0)
                .hypot(p[1].1 - p[0].1)
                .max((p[2].0 - p[3].0).hypot(p[2].1 - p[3].1)))
            .round()
            .max(1.) as u32;
            let h = ((p[3].0 - p[0].0)
                .hypot(p[3].1 - p[0].1)
                .max((p[2].0 - p[1].0).hypot(p[2].1 - p[1].1)))
            .round()
            .max(1.) as u32;
            Some(Message::Apply(Operation::Perspective {
                corners: [p[0], p[1], p[2], p[3]],
                width: w,
                height: h,
            }))
        }
        _ => None,
    };
    column![
        pick_list(AspectRatio::PRESETS.to_vec(), None::<AspectRatio>, |a| Message::Apply(Operation::CropAspect(a))).placeholder("Crop to aspect ratio").text_size(FONT).padding([3, 6]),
        row![btn("Rotate L", Some(Message::Apply(Operation::Rotate270))), btn("Rotate R", Some(Message::Apply(Operation::Rotate90))), btn("Flip H", Some(Message::Apply(Operation::FlipHorizontal))), btn("Flip V", Some(Message::Apply(Operation::FlipVertical)))].spacing(3),
        row![small("Straighten").width(78), slider(-15.0..=15.0, app.straighten, Message::Straighten).step(0.1_f32).height(14), btn("Apply", (app.straighten != 0.).then_some(Message::Apply(Operation::Straighten { degrees: app.straighten })))].spacing(6).align_y(Alignment::Center),
        row![small("Lens (k1)").width(78), slider(-0.3..=0.3, app.lens, Message::Lens).step(0.005_f32).height(14), btn("Apply", (app.lens != 0.).then_some(Message::Apply(Operation::LensDistortion { k1: app.lens })))].spacing(6).align_y(Alignment::Center),
        pick_list(size_names.clone(), None::<String>, move |s: String| {
            let (_, w, h) = sizes[size_names.iter().position(|x| *x == s).unwrap_or(0)];
            Message::Apply(Operation::FitWithin { max_width: w, max_height: h })
        })
        .placeholder("Resize for platform")
        .text_size(FONT)
        .padding([3, 6]),
        btn("Perspective from 4-point polygon", perspective),
        small("Perspective: draw a 4-point polygon (P) clockwise from the top-left corner of the plane to straighten."),
    ]
    .spacing(6)
    .into()
}

fn privacy_panel(app: &App) -> El<'_> {
    let mut c = column![
        row![
            pick_list(
                photo_privacy::profiles::BUILTIN.map(String::from).to_vec(),
                Some(app.profile.clone()),
                Message::Profile
            )
            .text_size(FONT)
            .padding([3, 6]),
            btn(
                "Scan  Ctrl+P",
                app.selected.is_some().then_some(Message::Scan)
            )
        ]
        .spacing(4),
    ]
    .spacing(4);
    let Some(e) = app.current() else {
        return c.into();
    };
    let Some(s) = &e.scan else {
        return c
            .push(small(
                "Not scanned. All detectors run locally; nothing is uploaded.",
            ))
            .into();
    };
    let finding = |k: usize, f: &photo_privacy::Finding, d: bool| -> El {
        let mut r = row![
            button(column![
                small(format!("{:?}", f.category)),
                small(f.description.clone())
            ])
            .width(Length::Fill)
            .padding([2, 4])
            .style(if app.focused_finding == Some(k) {
                theme::flat_selected
            } else {
                theme::flat
            })
            .on_press(Message::FocusFinding(k)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);
        if f.category == photo_privacy::Category::Face
            && let Some(reg) = f.region
        {
            r = r.push(btn("Select", Some(Message::SelectFace(reg))));
        }
        r.push(
            checkbox("", d)
                .on_toggle(move |v| Message::Dismiss(k, v))
                .size(13),
        )
        .into()
    };
    let (mut certain, mut uncertain) = (Column::new().spacing(1), Column::new().spacing(1));
    let (mut nc, mut nu) = (0, 0);
    for (k, (f, d)) in s.report.findings.iter().zip(&s.dismissed).enumerate() {
        if f.evidence == photo_privacy::Evidence::Contextual {
            uncertain = uncertain.push(finding(k, f, *d));
            nu += 1;
        } else {
            certain = certain.push(finding(k, f, *d));
            nc += 1;
        }
    }
    c = c
        .push(small(format!("CONFIRMED / PATTERN ({nc})")))
        .push(certain)
        .push(small(format!("UNCERTAIN ({nu})")))
        .push(uncertain);
    let has_faces = s
        .report
        .findings
        .iter()
        .any(|f| f.category == photo_privacy::Category::Face && f.region.is_some());
    c = c.push(
        row![
            pick_list(Method::ALL, Some(app.method), Message::Method)
                .text_size(FONT)
                .padding([2, 6]),
            btn("Conceal located", Some(Message::ConcealFindings)),
            btn(
                "Select all faces",
                has_faces.then_some(Message::SelectAllFaces)
            ),
        ]
        .spacing(4),
    );
    c = c.push(small(
        "Tick to dismiss false positives. Metadata findings are removed by every export.",
    ));
    c = c.push(small(format!(
        "NOT EXAMINED ({} categories)",
        s.report.not_examined.len()
    )));
    for d in s.report.detectors.iter().filter(|d| !d.available) {
        c = c.push(small(format!("- {}", d.note)));
    }
    c.into()
}

fn export_panel(app: &App) -> El<'_> {
    let idle = app.busy.is_none();
    let mut c = column![
        row![field(text_input("Output folder", &app.output_dir).on_input(Message::OutputDir).size(FONT).padding([3, 6]).style(theme::input)), btn("...", Some(Message::BrowseOutput))].spacing(4),
        row![pick_list(photo_core::ExportFormat::ALL, Some(app.format), Message::Format).text_size(FONT).padding([3, 6]), small(format!("JPEG quality {}", app.quality)), slider(40..=100, app.quality, Message::Quality).height(14)].spacing(6).align_y(Alignment::Center),
        checkbox("High-assurance review (block on unresolved findings)", app.high_assurance).on_toggle(Message::HighAssurance).size(13).text_size(FONT_SMALL),
        row![
            button(text("Export selected").size(FONT)).padding([3, 10]).style(theme::primary).on_press_maybe((idle && app.selected.is_some()).then_some(Message::Export { accept_unresolved: false, all: false })),
            btn("Export all", idle.then_some(Message::Export { accept_unresolved: false, all: true })),
        ]
        .spacing(4),
        small("Pipeline: edits, redaction flattening, metadata-free encode, reopen and verify. AI protection: unavailable."),
    ]
    .spacing(6);
    if let Some(e) = app.current() {
        match &e.export {
            Some(ExportState::Done {
                path,
                passed,
                report,
                ..
            }) => {
                let v = &report.verification;
                c = c.push(small(format!(
                    "{} {}\nmetadata findings {} · remaining {} · redactions checked {} · PSNR vs edit {}",
                    if *passed { "Verified:" } else { "Exported with accepted findings:" },
                    short(path),
                    v.metadata_findings.len(),
                    v.remaining_findings.len(),
                    v.redactions.len(),
                    v.encoded_vs_render.psnr_db.map(|x| format!("{x:.1} dB")).unwrap_or("lossless".into())
                )));
            }
            Some(ExportState::Blocked(w)) => {
                let mut b = column![small("EXPORT BLOCKED")].spacing(2);
                for x in w.iter().take(8) {
                    b = b.push(small(format!("- {x}")));
                }
                b = b.push(
                    button(text("I reviewed these: export anyway").size(FONT))
                        .padding([3, 8])
                        .style(theme::danger)
                        .on_press_maybe(idle.then_some(Message::Export {
                            accept_unresolved: true,
                            all: false,
                        })),
                );
                c = c.push(container(b).padding(6).style(theme::warning_box));
            }
            None => {}
        }
    }
    c.into()
}

// ------------------------------------------------------------------ Technical
fn technical(app: &App) -> El<'_> {
    let nav = TechPage::ALL
        .iter()
        .fold(column![small("TOOLS")].spacing(1).padding(6), |c, p| {
            c.push(
                button(text(p.label()).size(FONT))
                    .width(Length::Fill)
                    .padding([4, 8])
                    .style(if app.page == *p {
                        theme::flat_selected
                    } else {
                        theme::flat
                    })
                    .on_press(Message::Page(*p)),
            )
        });
    let mut files = column![small("COLLECTION")].spacing(1).padding(6);
    for (i, e) in app.entries.iter().enumerate() {
        files = files.push(
            button(small(e.name.as_str()))
                .width(Length::Fill)
                .padding([2, 8])
                .style(if app.selected == Some(i) {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::Select(i)),
        );
    }
    for (i, m) in app.media.iter().enumerate() {
        files = files.push(
            button(small(format!("{} ({:?})", m.name, m.kind())))
                .width(Length::Fill)
                .padding([2, 8])
                .style(if app.focus_media && app.selected_media == Some(i) {
                    theme::flat_selected
                } else {
                    theme::flat
                })
                .on_press(Message::SelectMedia(i)),
        );
    }
    let page: El = match app.page {
        TechPage::Diagnostics => diagnostics(app),
        TechPage::Performance => performance(app),
        TechPage::Validation => validation(app),
        TechPage::Forensics => forensics(app),
        TechPage::Lab => lab(app),
        TechPage::Models => models(app),
        TechPage::History => history(app),
        TechPage::Formats => formats(app),
    };
    row![
        container(column![
            nav,
            horizontal_rule(1),
            scrollable(files).height(Length::Fill)
        ])
        .width(210)
        .style(theme::panel),
        vertical_rule(1),
        container(column![
            container(text(app.page.label()).size(FONT_TITLE))
                .padding([6, 12])
                .style(theme::section_header)
                .width(Length::Fill),
            container(page).padding(12).height(Length::Fill)
        ])
        .width(Length::Fill),
    ]
    .into()
}

fn diagnostics(app: &App) -> El<'_> {
    let records = photo_log::recent(app.log_level, app.log_component, &app.log_search);
    let comps: Vec<String> = std::iter::once("All components".to_string())
        .chain(Component::ALL.iter().map(|c| c.to_string()))
        .collect();
    let chosen = app
        .log_component
        .map(|c| c.to_string())
        .unwrap_or("All components".into());
    let filters = row![
        pick_list(Level::ALL, Some(app.log_level), Message::LogLevel)
            .text_size(FONT)
            .padding([3, 6]),
        pick_list(comps, Some(chosen), |s: String| Message::LogComponent(
            Component::ALL.iter().copied().find(|c| c.to_string() == s)
        ))
        .text_size(FONT)
        .padding([3, 6]),
        field(
            text_input("Search sanitized messages and codes", &app.log_search)
                .on_input(Message::LogSearch)
                .size(FONT)
                .padding([3, 6])
                .style(theme::input)
        )
        .width(260),
        horizontal_space(),
        btn("Copy report", Some(Message::CopyReport)),
        toggle(
            "Preview report",
            app.report_preview.is_some(),
            Message::PreviewReport
        ),
        btn("Save report...", Some(Message::SaveReport)),
        button(text("Clear logs").size(FONT))
            .padding([3, 8])
            .style(theme::danger)
            .on_press(Message::ClearLogs),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let header = row![
        small("Time (UTC ms)").width(120),
        small("Level").width(70),
        small("Component").width(90),
        small("Code").width(80),
        small("Description"),
        horizontal_space(),
        small("ms").width(60)
    ]
    .spacing(8);
    let mut list = Column::new().spacing(0);
    for r in records.iter().rev().take(500) {
        list = list.push(
            row![
                small(r.ts_ms.to_string())
                    .width(120)
                    .font(iced::Font::MONOSPACE),
                small(r.level.to_string()).width(70),
                small(r.component.to_string()).width(90),
                small(r.code.clone()).width(80).font(iced::Font::MONOSPACE),
                small(r.description.clone()).width(Length::Fill),
                small(r.duration_ms.map(|d| format!("{d:.1}")).unwrap_or_default()).width(60),
            ]
            .spacing(8),
        );
    }
    let cfg = photo_log::config();
    let lookup = photo_log::Code::lookup(&app.code_lookup)
        .map(|c| format!("{}: {}", c.id(), c.explain()))
        .unwrap_or_else(|| {
            if app.code_lookup.is_empty() {
                String::new()
            } else {
                "Unknown code".into()
            }
        });
    let crash = std::fs::read_to_string(app.data_dir.join("last-crash"))
        .ok()
        .map(|s| {
            format!(
                "Last recorded internal error at {} (UTC ms). No memory dump was collected.",
                s.trim()
            )
        })
        .unwrap_or("No crash recorded.".into());
    let settings = row![
        small("Verbosity"),
        pick_list(Level::ALL, Some(cfg.min_level), Message::Verbosity)
            .text_size(FONT)
            .padding([2, 6]),
        small(format!("Retention {} days", cfg.retention_days)),
        slider(1..=90u32, cfg.retention_days, Message::Retention)
            .width(120)
            .height(14),
        small(format!(
            "Rotation {} / cap {} (local only, never uploaded)",
            mib(cfg.max_file_bytes),
            mib(cfg.max_total_bytes)
        )),
        horizontal_space(),
        field(
            text_input("Look up code (e.g. APD-3002)", &app.code_lookup)
                .on_input(Message::CodeLookup)
                .size(FONT)
                .padding([2, 6])
                .width(200)
                .style(theme::input)
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut col = column![
        filters,
        settings,
        small(lookup),
        row![
            small(format!(
                "Version {VERSION} · session {} · {}-{}",
                photo_log::session_id(),
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
            horizontal_space(),
            small(format!(
                "Unclean previous shutdown: {} · {crash}",
                if app.unclean_shutdown { "yes" } else { "no" }
            )),
        ],
        small(format!("Dependencies: {DEPENDENCIES}")),
        small("Models: none installed · GPU: not used (CPU renderer, tiny-skia) · Network: none"),
        horizontal_rule(1),
        header,
    ]
    .spacing(6);
    if let Some(r) = &app.report_preview {
        col = col.push(
            container(
                scrollable(text(r.clone()).size(FONT_SMALL).font(iced::Font::MONOSPACE))
                    .height(220),
            )
            .padding(8)
            .style(theme::panel),
        );
    }
    col.push(scrollable(list).height(Length::Fill)).into()
}

fn performance(app: &App) -> El<'_> {
    let p = &app.perf;
    let stat = |label: &'static str, v: Option<String>| -> El<'_> {
        label_value(
            label,
            v.unwrap_or_else(|| "not available on this system".into()),
        )
    };
    let mut by_kind: std::collections::BTreeMap<&str, (f64, f64, usize)> = Default::default();
    for (k, ms) in &p.samples {
        let e = by_kind.entry(k.as_str()).or_insert((0., 0., 0));
        e.0 += ms;
        e.1 = e.1.max(*ms);
        e.2 += 1;
    }
    let mut ops = column![
        row![
            small("Operation").width(220),
            small("Count").width(60),
            small("Mean ms").width(90),
            small("Max ms").width(90)
        ]
        .spacing(8)
    ]
    .spacing(2);
    for (k, (sum, max, n)) in by_kind {
        ops = ops.push(
            row![
                small(k).width(220),
                small(n.to_string()).width(60),
                small(format!("{:.1}", sum / n as f64)).width(90),
                small(format!("{max:.1}")).width(90)
            ]
            .spacing(8),
        );
    }
    let queue = app.entries.iter().filter(|e| e.rendering).count();
    column![
        small("Measured locally with sysinfo (CPU and memory refresh every 2 s while Technical mode is open). Nothing is estimated."),
        stat("CPU usage", p.cpu.map(|c| format!("{c:.1}% across {} logical cores", p.cores))),
        stat("System memory", p.mem_used.zip(p.mem_total).map(|(u, t)| format!("{} used of {}", mib(u), mib(t)))),
        stat("This process", p.process_mem.map(mib)),
        stat("Free disk (output)", p.disk_free.map(mib)),
        label_value("GPU", "Not used: the interface renders with tiny-skia on the CPU and no GPU compute backend is built in.".into()),
        label_value("GPU memory", "Not measured (no GPU backend)".into()),
        label_value("Render workers", format!("{} threads (scoped, one band per core)", p.cores.min(16))),
        label_value("Preview queue", format!("{queue} rendering")),
        label_value("Thumbnail cache", if app.cache_enabled { "enabled".into() } else { "disabled (default)".to_string() }),
        row![checkbox("Enable thumbnail cache (may store previews of sensitive photos)", app.cache_enabled).on_toggle(Message::Cache).size(13).text_size(FONT_SMALL), btn("Securely clear cache", Some(Message::ClearCache))].spacing(8),
        horizontal_rule(1),
        small("OPERATION TIMINGS (this session)"),
        ops,
    ]
    .spacing(6)
    .into()
}

fn validation(app: &App) -> El<'_> {
    let Some(e) = app.current() else {
        return small("Select a photograph.").into();
    };
    match &e.export {
        Some(ExportState::Done { report, .. }) => {
            let json = serde_json::to_string_pretty(&**report).unwrap_or_default();
            column![small("Report of the last export of this photograph (reopened encoded bytes). Contains no sensitive values."), scrollable(text(json).size(FONT_SMALL).font(iced::Font::MONOSPACE)).height(Length::Fill)].spacing(6).into()
        }
        Some(ExportState::Blocked(w)) => w
            .iter()
            .fold(column![small("Export was blocked:")].spacing(2), |c, x| {
                c.push(small(format!("- {x}")))
            })
            .into(),
        None => small("No export yet for this photograph.").into(),
    }
}

fn forensics(app: &App) -> El<'_> {
    let Some(f) = &app.forensics else {
        return column![small("Read-only inspection of the selected file. Values are shown locally only and never logged."), btn("Inspect selected file", app.selected.map(|_| Message::Forensics))].spacing(6).into();
    };
    let mut c = column![
        small(f.disclaimer.clone()),
        label_value("Container", f.container.clone()),
        label_value("Size", mib(f.size_bytes as u64)),
        label_value("SHA-256", f.sha256.clone()),
        label_value("BLAKE3", f.blake3.clone()),
        label_value(
            "GPS metadata",
            if f.gps_present {
                "present (removed on export)".into()
            } else {
                "none found".into()
            }
        ),
        label_value(
            "Embedded thumbnail",
            f.embedded_thumbnail_bytes
                .map(|b| format!("{b} bytes"))
                .unwrap_or("none found".into())
        ),
        label_value(
            "ICC profile",
            if f.icc_profile_present {
                "present".into()
            } else {
                "none".into()
            }
        ),
        label_value("Provenance", f.c2pa.clone()),
    ]
    .spacing(4);
    if let Some(j) = &f.jpeg {
        c = c.push(label_value(
            "JPEG",
            format!(
                "{} · {} tables · est. quality {} · {}",
                if j.progressive {
                    "progressive"
                } else {
                    "baseline"
                },
                j.quant_tables,
                j.estimated_quality
                    .map(|q| q.to_string())
                    .unwrap_or("?".into()),
                j.chroma_subsampling.clone().unwrap_or_default()
            ),
        ));
    }
    c = c.push(small("STRUCTURE")).push(
        f.structure
            .iter()
            .fold(Column::new(), |c, s| c.push(small(format!("- {s}")))),
    );
    c = c.push(small("HEURISTIC INDICATORS (not proof)")).push(
        f.indicators
            .iter()
            .fold(Column::new(), |c, s| c.push(small(format!("- {s}")))),
    );
    c = c.push(small(format!("EXIF FIELDS ({})", f.exif.len())));
    for (k, v) in f.exif.iter().take(200) {
        c = c.push(row![small(k.clone()).width(260), small(v.clone())].spacing(8));
    }
    c = c.push(btn("Refresh", Some(Message::Forensics)));
    scrollable(c).height(Length::Fill).into()
}

fn lab(app: &App) -> El<'_> {
    let mut c = column![
        row![btn("Run transformation tests", app.busy.is_none().then_some(Message::Lab)), small("Measures pixel differences that survive transforms between the original and its export. AI resistance is not evaluated.")].spacing(8).align_y(Alignment::Center),
        text("Test                         PSNR (dB)       SSIM          Time (ms)").size(FONT_SMALL).font(iced::Font::MONOSPACE),
    ]
    .spacing(6);
    if let Some(r) = &app.lab {
        for m in &r.measurements {
            c = c.push(
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
                .size(FONT_SMALL)
                .font(iced::Font::MONOSPACE),
            );
        }
    }
    c.push(horizontal_rule(1))
        .push(small("ADVERSARIAL PROTECTION"))
        .push(small("Quick / Balanced / Maximum / Custom: unavailable. No validated defense model is installed and no preset has measured effectiveness. The experimental CLIP objective is available only through the CLI research feature and is not a protection claim."))
        .into()
}

fn formats(app: &App) -> El<'_> {
    let mut c = column![small("Every file is identified from its content (magic bytes and container structure), never from its extension.")].spacing(6);
    if app.media.is_empty() {
        c = c.push(small("No audio, video or subtitle files imported. Add them from Essential or Professional mode."));
    }
    for (i, m) in app.media.iter().enumerate() {
        let mut g = column![
            row![
                button(t(m.name.as_str()))
                    .padding([2, 6])
                    .style(theme::flat)
                    .on_press(Message::SelectMedia(i)),
                horizontal_space(),
                small(format!("{} · {}", mib(m.bytes), m.status))
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(3);
        if m.mismatch {
            g = g.push(
                container(small("Extension does not match the detected content."))
                    .padding(4)
                    .style(theme::warning_box),
            );
        }
        if let Some(info) = &m.info {
            g = g.push(small(format!(
                "Detected: {:?} / {}   Duration: {}",
                info.kind,
                info.container,
                info.duration_s
                    .map(|d| format!("{d:.3} s"))
                    .unwrap_or("unknown".into())
            )));
            for tr in &info.tracks {
                g = g.push(small(track_line(tr)));
            }
            for f in &info.findings {
                g = g.push(small(format!(
                    "Finding: {:?} [{:?}] {}",
                    f.category, f.source, f.description
                )));
            }
            for w in &info.warnings {
                g = g.push(small(format!("Note: {w}")));
            }
        }
        c = c.push(
            container(g)
                .padding(8)
                .width(Length::Fill)
                .style(theme::panel),
        );
    }
    c = c
        .push(horizontal_rule(1))
        .push(small("SUPPORT (this build)"));
    for line in [
        "Audio decode and analysis: WAV, FLAC, MP3, AAC / M4A, ALAC, Ogg Vorbis (Symphonia, pure Rust).",
        "Audio export: metadata-free WAV (16-bit PCM or 32-bit float) with mute / bleep redaction, reopened and verified.",
        "Video: MP4 / MOV / 3GP / M4A metadata removal without re-encoding (chunk offsets rewritten, chunks verified). MKV / WebM: inspection only.",
        "Subtitles: SRT and WebVTT Privacy Scan and cue redaction.",
        "Not available: video decoding, frame editing, re-encoding, AI voice or video protection (no validated model), speech-to-text, Opus / AIFF / WMA / AC-3 decoding.",
    ] {
        c = c.push(small(line));
    }
    scrollable(c).into()
}

fn track_line(tr: &photo_media::Track) -> String {
    let mut s = format!("Track {}: {} {}", tr.id, tr.kind, tr.codec);
    if let (Some(w), Some(h)) = (tr.width, tr.height) {
        s += &format!("  {w}x{h}");
    }
    if let Some(f) = tr.frame_rate {
        s += &format!("  {f:.3} fps");
    }
    if let Some(r) = tr.sample_rate {
        s += &format!("  {r} Hz");
    }
    if let Some(c) = tr.channels {
        s += &format!("  {c} ch");
    }
    if let Some(l) = &tr.language {
        s += &format!("  lang {l}");
    }
    s
}

fn opt_db(v: Option<f64>, unit: &str) -> String {
    v.filter(|x| x.is_finite())
        .map(|x| format!("{x:.1} {unit}"))
        .unwrap_or("n/a".into())
}

/// Information, analysis, redaction and export for one media file.
fn media_panel<'a>(app: &'a App, m: &'a MediaEntry, compact: bool) -> El<'a> {
    use photo_media::MediaKind;
    let idle = app.busy.is_none() && m.status != "Exporting";
    let mut c = column![text(m.name.as_str()).size(FONT_TITLE)].spacing(6);
    let Some(info) = &m.info else {
        return c.push(small(m.status.as_str())).into();
    };
    c = c.push(small(format!(
        "{:?} · {} · {} · {}",
        info.kind,
        info.container,
        mib(m.bytes),
        info.duration_s
            .map(|d| format!("{d:.2} s"))
            .unwrap_or("duration unknown".into())
    )));
    if m.mismatch {
        c = c.push(
            container(small(
                "The file extension does not match its content; it is handled by its real format.",
            ))
            .padding(4)
            .style(theme::warning_box),
        );
    }
    for tr in info.tracks.iter().take(if compact { 4 } else { 16 }) {
        c = c.push(small(track_line(tr)));
    }
    let mut priv_ = column![small("PRIVACY METADATA")].spacing(2);
    if info.findings.is_empty() {
        priv_ = priv_.push(small(
            "No privacy metadata found in the inspected structures.",
        ));
    }
    for f in info.findings.iter().take(12) {
        priv_ = priv_.push(small(format!("{:?}: {}", f.category, f.description)));
    }
    for w in &info.warnings {
        priv_ = priv_.push(small(format!("Note: {w}")));
    }
    c = c.push(
        container(priv_)
            .padding(6)
            .width(Length::Fill)
            .style(theme::section_header),
    );
    if let Some(a) = &m.analysis {
        if let Some(w) = &m.wave {
            c = c.push(small("WAVEFORM")).push(
                image(w.clone())
                    .width(Length::Fill)
                    .content_fit(ContentFit::Fill)
                    .height(if compact { 70 } else { 120 }),
            );
        }
        if let Some(sp) = &m.spectro {
            c = c
                .push(small(
                    "SPECTROGRAM (0 Hz bottom to Nyquist top, 70 dB range)",
                ))
                .push(
                    image(sp.clone())
                        .width(Length::Fill)
                        .content_fit(ContentFit::Fill)
                        .height(if compact { 70 } else { 128 }),
                );
        }
        c = c.push(label_value(
            "Format",
            format!(
                "{} Hz · {} ch · {} frames",
                a.sample_rate, a.channels, a.frames
            ),
        ));
        c = c.push(label_value(
            "Integrated loudness",
            opt_db(a.integrated_lufs, "LUFS"),
        ));
        c = c.push(label_value(
            "Loudness range",
            opt_db(a.loudness_range_lu, "LU"),
        ));
        c = c.push(label_value("True peak", opt_db(a.true_peak_dbtp, "dBTP")));
        c = c.push(label_value(
            "Sample peak",
            opt_db(Some(a.peak_dbfs), "dBFS"),
        ));
        c = c.push(label_value(
            "Clipped samples",
            a.clipped_samples.to_string(),
        ));
        c = c.push(label_value(
            "Silence",
            format!("{:.1} %", a.silent_fraction * 100.),
        ));
        if a.decode_errors > 0 {
            c = c.push(
                container(small(format!(
                    "{} packet(s) failed to decode and were skipped.",
                    a.decode_errors
                )))
                .padding(4)
                .style(theme::warning_box),
            );
        }
        // Segment editor
        let mut seg = column![small("REDACT SPOKEN CONTENT (time ranges in seconds)")].spacing(4);
        for (k, sgm) in m.segments.iter().enumerate() {
            seg = seg.push(
                row![
                    small(format!(
                        "{:?} {:.2} s - {:.2} s",
                        sgm.kind, sgm.start_s, sgm.end_s
                    ))
                    .width(Length::Fill),
                    btn("Remove", Some(Message::RemoveSegment(k)))
                ]
                .align_y(Alignment::Center),
            );
        }
        seg = seg.push(
            row![
                field(
                    text_input("start", &app.seg_start)
                        .on_input(Message::SegStart)
                        .size(FONT)
                        .width(70)
                        .padding([3, 6])
                        .style(theme::input)
                ),
                field(
                    text_input("end", &app.seg_end)
                        .on_input(Message::SegEnd)
                        .size(FONT)
                        .width(70)
                        .padding([3, 6])
                        .style(theme::input)
                ),
                btn(
                    "Mute",
                    Some(Message::AddSegment(photo_media::audio::RedactKind::Mute))
                ),
                btn(
                    "Bleep",
                    Some(Message::AddSegment(photo_media::audio::RedactKind::Bleep))
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
        seg = seg.push(small("Spoken names and numbers cannot be found automatically: no local speech-to-text model is installed. Listen and mark ranges by hand."));
        c = c.push(
            container(seg)
                .padding(6)
                .width(Length::Fill)
                .style(theme::section_header),
        );
    }
    if info.kind == MediaKind::Subtitle {
        c = c.push(small("Export writes a copy with every cue that contains a Privacy Scan finding replaced by [redacted]."));
    }
    let exportable = matches!(info.kind, MediaKind::Audio | MediaKind::Subtitle)
        || (info.kind == MediaKind::Video
            && matches!(info.container.as_str(), "MP4" | "MOV" | "3GP" | "M4A"));
    let label = match info.kind {
        MediaKind::Audio => "Export clean WAV",
        MediaKind::Video => "Remove metadata (no re-encode)",
        MediaKind::Subtitle => "Export redacted subtitles",
        _ => "Export",
    };
    c = c.push(
        row![
            button(text(label).size(FONT))
                .padding([3, 12])
                .style(theme::primary)
                .on_press_maybe((idle && exportable).then_some(Message::MediaExport)),
            small(if exportable {
                ""
            } else {
                "Export is not available for this format."
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    c = c.push(small("AI voice / video protection: unavailable. No validated model is installed, so no adversarial changes are made and none are claimed."));
    if let Some(out) = &m.output {
        c = c.push(
            container(small(format!("Saved and verified: {}", out.display())))
                .padding(4)
                .style(theme::banner),
        );
    }
    if let Some(cmp) = &m.comparison {
        c = c.push(small("COMPARISON (original vs export)"));
        c = c.push(label_value(
            "Frames compared",
            cmp.frames_compared.to_string(),
        ));
        c = c.push(label_value(
            "Length difference",
            format!("{} frames", cmp.length_difference_frames),
        ));
        c = c.push(label_value("SNR", opt_db(cmp.snr_db, "dB")));
        c = c.push(label_value(
            "Segmental SNR",
            opt_db(cmp.segmental_snr_db, "dB"),
        ));
        if !m.segments.is_empty() {
            c = c.push(small("SNR includes the muted or bleeped ranges, so a low overall value is expected; segmental SNR reflects the untouched audio."));
        }
        c = c.push(small(cmp.note.as_str()));
    }
    if info.kind == MediaKind::Audio {
        c = c.push(small(
            "A/B LISTENING (both loudness-matched to -23 LUFS, first 2 minutes)",
        ));
        let playing = |p: bool| app.playing == Some((app.selected_media.unwrap_or(usize::MAX), p));
        let i = app.selected_media.unwrap_or(0);
        c = c.push(
            row![
                toggle("A: Original", playing(false), Message::Play(i, false)),
                button(text("B: Export").size(FONT))
                    .padding([3, 8])
                    .style(if playing(true) {
                        theme::tool_active
                    } else {
                        theme::tool
                    })
                    .on_press_maybe(m.output.is_some().then_some(Message::Play(i, true))),
                btn("Stop", Some(Message::StopAudio)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    if !compact && let Some(r) = &m.report {
        c = c.push(small("EXPORT REPORT")).push(
            container(
                text(r.as_str())
                    .size(FONT_SMALL)
                    .font(iced::Font::MONOSPACE),
            )
            .padding(6)
            .width(Length::Fill)
            .style(theme::section_header),
        );
    }
    c.into()
}

fn models(app: &App) -> El<'_> {
    let face_model = app
        .face_model_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "none".into());
    column![
        row![
            btn("Choose face model (.onnx)", Some(Message::ChooseFaceModel)),
            small(format!("Face model: {face_model}")),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        small("Face, person, object, document, segmentation and OCR detectors need licence-reviewed local models (see docs/research/MODEL_LICENSES.md)."),
        small("Accepted formats: SafeTensors (and ONNX for face detection) with an independently reviewed manifest (SHA-256, size, code and weight licences). Pickle and executable formats are refused. Verify with: photo-defense verify-model MODEL MANIFEST."),
        small("Downloads and updates: not implemented. The application makes no network connections."),
    ]
    .spacing(6)
    .into()
}

fn history(app: &App) -> El<'_> {
    let mut c = Column::new().spacing(4);
    for e in &app.entries {
        c = c.push(t(format!(
            "{}  ({} layers, {} undo steps)",
            e.name,
            e.edits.active().len(),
            e.undo.len()
        )));
        for op in e.edits.active() {
            c = c.push(small(format!("    {}", op.label())));
        }
    }
    if app.entries.is_empty() {
        c = c.push(small("No photographs in this session."));
    }
    scrollable(c.push(small("Edit history is part of the project and autosave journal. It contains photo paths but never pixels."))).height(Length::Fill).into()
}
