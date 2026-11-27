//! Application state, messages and update logic.
use crate::{theme, views};
use iced::widget::{image, pane_grid};
use iced::{Event, Point, Subscription, Task, keyboard, window};
use photo_core::ExportFormat;
use photo_edit::color::{Adjustments, Curve, Histogram, Lut3d};
use photo_edit::{
    AspectRatio, BlendMode, Concealment, EditStack, LayerMask, Operation, Rect, Selection,
};
use photo_log::{Code, Component, Level};
use photo_privacy::{Finding, Profile, ScanReport};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub const PROXY_EDGE: u32 = 1600;
pub const QUEUE_LIMIT: usize = 500;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEPENDENCIES: &str = env!("APD_DEPENDENCIES");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Home,
    Essential,
    #[default]
    Professional,
    Technical,
    Tutorial,
}
impl Mode {
    pub const ALL: [Self; 5] = [
        Self::Home,
        Self::Essential,
        Self::Professional,
        Self::Technical,
        Self::Tutorial,
    ];
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Home => write!(f, "Main Menu"),
            _ => write!(f, "{self:?}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub notes: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    Photo,
    View,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Inspect,
    Crop,
    Rect,
    Ellipse,
    Polygon,
    Lasso,
    Brush,
    Linear,
    Radial,
}
impl Tool {
    pub const ALL: [Self; 9] = [
        Self::Inspect,
        Self::Crop,
        Self::Rect,
        Self::Ellipse,
        Self::Polygon,
        Self::Lasso,
        Self::Brush,
        Self::Linear,
        Self::Radial,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Inspect => "Inspect",
            Self::Crop => "Crop",
            Self::Rect => "Rect",
            Self::Ellipse => "Ellipse",
            Self::Polygon => "Polygon",
            Self::Lasso => "Lasso",
            Self::Brush => "Brush",
            Self::Linear => "Gradient",
            Self::Radial => "Radial",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Inspect => "I",
            Self::Crop => "C",
            Self::Rect => "M",
            Self::Ellipse => "E",
            Self::Polygon => "P",
            Self::Lasso => "L",
            Self::Brush => "B",
            Self::Linear => "G",
            Self::Radial => "R",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Edited,
    Before,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Black,
    White,
    Blur,
    Pixelate,
    Mosaic,
}
impl Method {
    pub const ALL: [Self; 5] = [
        Self::Black,
        Self::White,
        Self::Blur,
        Self::Pixelate,
        Self::Mosaic,
    ];
    pub fn concealment(self, strength: u32) -> Concealment {
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
            Self::Blur => "Blur (visual only)",
            Self::Pixelate => "Pixelate (visual only)",
            Self::Mosaic => "Mosaic (visual only)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Library,
    Viewer,
    Inspector,
    Filmstrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TechPage {
    #[default]
    Diagnostics,
    Performance,
    Validation,
    Forensics,
    Lab,
    Models,
    History,
    Formats,
}
impl TechPage {
    pub const ALL: [Self; 8] = [
        Self::Diagnostics,
        Self::Performance,
        Self::Validation,
        Self::Forensics,
        Self::Formats,
        Self::Lab,
        Self::Models,
        Self::History,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Diagnostics => "Diagnostics Center",
            Self::Performance => "Performance Center",
            Self::Validation => "Export Validation",
            Self::Forensics => "Forensic Inspection",
            Self::Lab => "Defense Lab",
            Self::Models => "Model Library",
            Self::History => "Processing History",
            Self::Formats => "Format Inspection",
        }
    }
}

/// Every adjustable value in the Color Studio. HSL uses the selected band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Exposure,
    Contrast,
    Brightness,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Temperature,
    Tint,
    Vibrance,
    Saturation,
    Clarity,
    Texture,
    Dehaze,
    Sharpness,
    Noise,
    Vignette,
    Grain,
    Gamma,
    BlackPoint,
    WhitePoint,
    HslHue,
    HslSat,
    HslLum,
    CurveShadows,
    CurveDarks,
    CurveLights,
    CurveHighlights,
    SplitShadowHue,
    SplitShadowAmount,
    SplitHighlightHue,
    SplitHighlightAmount,
    LutStrength,
    Opacity,
}

#[derive(Debug, Clone)]
pub struct Edited {
    pub handle: image::Handle,
    pub before: image::Handle,
    pub full: (u32, u32),
    pub proxy: (u32, u32),
    pub hist: Arc<Histogram>,
    pub hist_img: image::Handle,
    pub pixels: Arc<::image::RgbaImage>,
    pub ms: f64,
}

#[derive(Debug, Clone)]
pub struct ScanState {
    pub report: ScanReport,
    pub dismissed: Vec<bool>,
    pub dims: (u32, u32),
}

#[derive(Debug, Clone)]
pub enum ExportState {
    Done {
        path: PathBuf,
        passed: bool,
        report: Box<photo_pipeline::ExportReport>,
        ms: f64,
    },
    Blocked(Vec<String>),
}

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub raw: Option<String>,
    pub thumb: image::Handle,
    pub edits: EditStack,
    pub undo: Vec<EditStack>,
    pub redo: Vec<EditStack>,
    pub generation: u64,
    pub rendering: bool,
    pub dirty: bool,
    pub proxy: Option<Arc<::image::DynamicImage>>,
    pub edited: Option<Edited>,
    pub scan: Option<ScanState>,
    pub export: Option<ExportState>,
    pub rating: u8,
    pub flagged: bool,
    pub catalog_id: Option<i64>,
    pub status: String,
}

#[derive(Clone)]
pub struct Imported {
    path: PathBuf,
    width: u32,
    height: u32,
    bytes: u64,
    raw: Option<String>,
    thumb: image::Handle,
    proxy: Arc<::image::DynamicImage>,
}
impl std::fmt::Debug for Imported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Imported({}x{})", self.width, self.height)
    }
}

/// A non-photo media file (audio, video or subtitles).
pub struct MediaEntry {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub info: Option<photo_media::MediaInfo>,
    pub mismatch: bool,
    pub analysis: Option<photo_media::audio::Analysis>,
    pub wave: Option<image::Handle>,
    pub spectro: Option<image::Handle>,
    pub segments: Vec<photo_media::audio::Segment>,
    pub status: String,
    pub output: Option<PathBuf>,
    pub report: Option<String>,
    pub comparison: Option<photo_media::audio::Comparison>,
    pub out_lufs: Option<f64>,
    pub video_preview: Option<image::Handle>,
    pub video_time_s: f64,
    pub video_duration_s: f64,
    pub video_playing: bool,
    pub video_rect: String,
    pub video_from: String,
    pub video_to: String,
    pub video_blur: bool,
    pub video_track_warnings: Vec<String>,
}
impl MediaEntry {
    pub fn kind(&self) -> photo_media::MediaKind {
        self.info
            .as_ref()
            .map(|i| i.kind)
            .unwrap_or(photo_media::MediaKind::Unknown)
    }
}

#[derive(Clone)]
pub struct MediaLoad {
    info: Option<photo_media::MediaInfo>,
    mismatch: bool,
    bytes: u64,
    analysis: Option<photo_media::audio::Analysis>,
    wave: Option<image::Handle>,
    spectro: Option<image::Handle>,
    video_preview: Option<image::Handle>,
    video_duration_s: f64,
    error: Option<String>,
}
impl std::fmt::Debug for MediaLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MediaLoad")
    }
}
#[derive(Clone)]
pub struct Pcm(pub Arc<Vec<f32>>, pub u16, pub u32);
impl std::fmt::Debug for Pcm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pcm({} samples)", self.0.len())
    }
}
pub struct Player {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    sink: Option<rodio::Sink>,
}

#[derive(Debug, Clone, Default)]
pub struct Perf {
    pub cpu: Option<f32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub process_mem: Option<u64>,
    pub disk_free: Option<u64>,
    pub cores: usize,
    pub samples: Vec<(String, f64)>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Event(Event),
    Tick,
    Autosave,
    MenuToggle(Menu),
    MenuClose,
    Mode(Mode),
    Appearance(theme::Appearance),
    UiScale(f64),
    Fullscreen(bool),
    Group(&'static str),
    OpenDialog,
    OpenFolderDialog,
    Picked(Vec<PathBuf>),
    Imported(Result<Vec<Imported>, String>, Vec<String>),
    Select(usize),
    Remove(usize),
    ClearAll,
    Rating(usize, u8),
    Flag(usize),
    InputPath(String),
    ImportPath,
    OutputDir(String),
    BrowseOutput,
    OutputPicked(Option<PathBuf>),
    Tool(Tool),
    PointerMove(Point),
    PointerDown,
    PointerUp,
    View(ViewMode),
    Clipping,
    Thirds,
    Escape,
    Enter,
    ProxyLoaded(usize, Result<Arc<::image::DynamicImage>, String>),
    Rendered(usize, u64, Result<Edited, String>),
    Apply(Operation),
    Undo,
    Redo,
    Adjust(Field, f32),
    Commit,
    ResetLayer,
    AutoTone,
    Preset(String),
    Band(usize),
    Layer(Option<usize>),
    LayerToggle(usize),
    LayerMove(usize, bool),
    LayerDelete(usize),
    LayerBlend(BlendMode),
    NewLayer,
    Method(Method),
    Strength(u32),
    BrushRadius(f32),
    Straighten(f32),
    Lens(f32),
    ConcealSelection,
    ImportLut,
    LutLoaded(Result<Lut3d, String>),
    ExportLut,
    Profile(String),
    Scan,
    Scanned(usize, u64, Result<(ScanReport, (u32, u32)), String>),
    FocusFinding(usize),
    Dismiss(usize, bool),
    ConcealFindings,
    Format(ExportFormat),
    Quality(u8),
    HighAssurance(bool),
    AutoRedact(bool),
    EssentialAspect(Option<AspectRatio>),
    Export {
        accept_unresolved: bool,
        all: bool,
    },
    Exported(Vec<(usize, Result<ExportState, String>)>),
    Cancel,
    SaveProject,
    ProjectSaved(Result<PathBuf, String>),
    OpenProject,
    ProjectOpened(Result<photo_project::Project, String>),
    RestoreAutosave,
    DiscardAutosave,
    LibSearch(String),
    LibRating(u8),
    TagInput(String),
    ApplyTag,
    Page(TechPage),
    LogLevel(Level),
    LogComponent(Option<Component>),
    LogSearch(String),
    CodeLookup(String),
    CopyReport,
    PreviewReport,
    SaveReport,
    ReportSaved(Result<PathBuf, String>),
    ClearLogs,
    Retention(u32),
    Verbosity(Level),
    Lab,
    LabDone(Result<photo_lab::Report, String>),
    RunValidation(photo_lab::validation::ValidationMode),
    ValidationDone(Result<Box<photo_lab::validation::ValidationSuiteReport>, String>),
    ExportValidationReport(bool),
    ValidationReportSaved(Result<PathBuf, String>),
    Forensics,
    ForensicsDone(Result<Box<photo_privacy::forensics::ForensicReport>, String>),
    Cache(bool),
    ClearCache,
    MediaLoaded(PathBuf, Box<MediaLoad>),
    SelectMedia(usize),
    SegStart(String),
    SegEnd(String),
    AddSegment(photo_media::audio::RedactKind),
    RemoveSegment(usize),
    MediaExport,
    MediaExported(
        usize,
        Result<
            (
                PathBuf,
                String,
                Option<photo_media::audio::Comparison>,
                Option<f64>,
            ),
            String,
        >,
    ),
    Play(usize, bool),
    PlayReady(Result<Pcm, String>, f32),
    VideoScrub(f64),
    VideoFrameReady(usize, Result<(f64, u32, u32, Vec<u8>), String>),
    VideoTogglePlay,
    VideoTick,
    VideoRect(String),
    VideoFrom(String),
    VideoTo(String),
    VideoBlur(bool),
    VideoRedact,
    StopAudio,
    ChooseC2paCert,
    C2paCertChosen(Option<PathBuf>),
    ChooseC2paKey,
    C2paKeyChosen(Option<PathBuf>),
    ChooseVoiceModelFolder,
    VoiceModelFolderChosen(Option<PathBuf>),
    ProtectVoice,
    ChooseFaceModel,
    FaceModelChosen(Option<PathBuf>),
    ChooseClipModel,
    ClipModelChosen(Option<PathBuf>),
    ChooseClipManifest,
    ClipManifestChosen(Option<PathBuf>),
    BatchValidationDone(Vec<String>),
    ShowWizard(bool),
    WizardStep(u8),
    TutorialStep(u8),
    CheckForUpdates,
    UpdatesChecked(Result<UpdateInfo, String>),
    OpenReleasesPage,
    DownloadLatestInstaller,
    InstallerDownloaded(Result<String, String>),
    LoadCalibrationSample,
    VideoSetFromNow,
    VideoSetToNow,
    VideoStep(f64),
    VideoTrackAndRedact,
    VideoTrackDone(usize, Result<(PathBuf, String, Vec<String>), String>),
    SelectFace(photo_privacy::Rect),
    SelectAllFaces,
    PaneResized(pane_grid::ResizeEvent),
    PaneDragged(pane_grid::DragEvent),
    PaneMaximize(Pane),
    ResetLayout,
}

pub struct App {
    pub mode: Mode,
    pub appearance: theme::Appearance,
    pub ui_scale: f64,
    pub menu: Option<Menu>,
    pub fullscreen: bool,
    pub panes: pane_grid::State<Pane>,
    pub closed_groups: std::collections::HashSet<&'static str>,
    pub entries: Vec<Entry>,
    pub selected: Option<usize>,
    pub input_path: String,
    pub output_dir: String,
    pub status: String,
    pub busy: Option<String>,
    pub cancel: Arc<AtomicBool>,
    pub tool: Tool,
    pub view: ViewMode,
    pub clipping: bool,
    pub thirds: bool,
    pub pointer: Point,
    pub pressed: bool,
    pub drag_from: Option<Point>,
    pub points: Vec<(f32, f32)>,
    pub selection: Option<Selection>,
    pub gradient: Option<((f32, f32), (f32, f32))>,
    pub pixel_probe: Option<(u32, u32, [u8; 4])>,
    pub method: Method,
    pub strength: u32,
    pub brush_radius: f32,
    pub straighten: f32,
    pub lens: f32,
    pub band: usize,
    pub active_layer: Option<usize>,
    pub editing: bool,
    pub profile: String,
    pub focused_finding: Option<usize>,
    pub format: ExportFormat,
    pub quality: u8,
    pub high_assurance: bool,
    pub auto_redact: bool,
    pub essential_aspect: Option<AspectRatio>,
    pub project_path: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub autosave_offer: Option<photo_project::Project>,
    pub pending_restore: Option<photo_project::Project>,
    pub unclean_shutdown: bool,
    pub safe_mode: bool,
    pub _session: Option<photo_project::SessionGuard>,
    pub dirty_project: bool,
    pub catalog: Option<photo_catalog::Catalog>,
    pub cache_enabled: bool,
    pub lib_search: String,
    pub lib_rating: u8,
    pub lib_rows: Vec<photo_catalog::PhotoRow>,
    pub tag_input: String,
    pub page: TechPage,
    pub log_level: Level,
    pub log_component: Option<Component>,
    pub log_search: String,
    pub code_lookup: String,
    pub report_preview: Option<String>,
    pub lab: Option<photo_lab::Report>,
    pub validation_report: Option<Box<photo_lab::validation::ValidationSuiteReport>>,
    pub forensics: Option<Box<photo_privacy::forensics::ForensicReport>>,
    pub perf: Perf,
    pub sys: Option<sysinfo::System>,
    pub c2pa_cert_path: Option<PathBuf>,
    pub c2pa_key_path: Option<PathBuf>,
    pub media: Vec<MediaEntry>,
    pub selected_media: Option<usize>,
    pub focus_media: bool,
    pub seg_start: String,
    pub seg_end: String,
    pub player: Option<Player>,
    pub playing: Option<(usize, bool)>,
    pub voice_model_dir: Option<PathBuf>,
    pub face_model_path: Option<PathBuf>,
    pub clip_model_path: Option<PathBuf>,
    pub clip_manifest_path: Option<PathBuf>,
    pub batch_validation: Vec<String>,
    pub show_wizard: bool,
    pub wizard_step: u8,
    pub tutorial_step: u8,
    pub update_status: Option<Result<UpdateInfo, String>>,
    pub download_status: Option<String>,
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("APD_DATA_DIR") {
        return PathBuf::from(d);
    }
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.unwrap_or_else(std::env::temp_dir)
        .join("AI Photo Defense")
}

pub fn short(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
pub fn profile_by_name(n: &str) -> Profile {
    Profile::builtin(n).unwrap_or_else(|| Profile::builtin("Maximum Privacy").expect("builtin"))
}
fn handle(p: &::image::RgbaImage) -> image::Handle {
    image::Handle::from_rgba(p.width(), p.height(), p.clone().into_raw())
}
fn default_panes() -> pane_grid::State<Pane> {
    use pane_grid::{Axis, Configuration};
    pane_grid::State::with_configuration(Configuration::Split {
        axis: Axis::Vertical,
        ratio: 0.16,
        a: Box::new(Configuration::Pane(Pane::Library)),
        b: Box::new(Configuration::Split {
            axis: Axis::Vertical,
            ratio: 0.72,
            a: Box::new(Configuration::Split {
                axis: Axis::Horizontal,
                ratio: 0.82,
                a: Box::new(Configuration::Pane(Pane::Viewer)),
                b: Box::new(Configuration::Pane(Pane::Filmstrip)),
            }),
            b: Box::new(Configuration::Pane(Pane::Inspector)),
        }),
    })
}

/// Reads the value of a field from an adjustment set (HSL uses `band`).
pub fn field_value(a: &Adjustments, f: Field, band: usize) -> f32 {
    let curve = curve_params(&a.curve_master);
    match f {
        Field::Exposure => a.exposure,
        Field::Contrast => a.contrast,
        Field::Brightness => a.brightness,
        Field::Highlights => a.highlights,
        Field::Shadows => a.shadows,
        Field::Whites => a.whites,
        Field::Blacks => a.blacks,
        Field::Temperature => a.temperature,
        Field::Tint => a.tint,
        Field::Vibrance => a.vibrance,
        Field::Saturation => a.saturation,
        Field::Clarity => a.clarity,
        Field::Texture => a.texture,
        Field::Dehaze => a.dehaze,
        Field::Sharpness => a.sharpness,
        Field::Noise => a.noise_reduction,
        Field::Vignette => a.vignette,
        Field::Grain => a.grain,
        Field::Gamma => a.gamma,
        Field::BlackPoint => a.black_point,
        Field::WhitePoint => a.white_point,
        Field::HslHue => a.hsl_hue[band],
        Field::HslSat => a.hsl_saturation[band],
        Field::HslLum => a.hsl_luminance[band],
        Field::CurveShadows => curve[0],
        Field::CurveDarks => curve[1],
        Field::CurveLights => curve[2],
        Field::CurveHighlights => curve[3],
        Field::SplitShadowHue => a.split_shadow_hue,
        Field::SplitShadowAmount => a.split_shadow_strength,
        Field::SplitHighlightHue => a.split_highlight_hue,
        Field::SplitHighlightAmount => a.split_highlight_strength,
        Field::LutStrength => a.lut_strength,
        Field::Opacity => 1.,
    }
}
/// Parametric curve values are recovered from the stored control points.
pub fn curve_params(c: &Curve) -> [f32; 4] {
    if c.points.len() != 6 {
        return [0.; 4];
    }
    let k = 0.25;
    [
        (c.points[1].1 - 0.125) / (k * 0.5),
        (c.points[2].1 - 0.375) / k,
        (c.points[3].1 - 0.625) / k,
        (c.points[4].1 - 0.875) / (k * 0.5),
    ]
}
fn set_field(a: &mut Adjustments, f: Field, band: usize, v: f32) {
    let mut curve = curve_params(&a.curve_master);
    match f {
        Field::Exposure => a.exposure = v.clamp(-5., 5.),
        Field::Contrast => a.contrast = v,
        Field::Brightness => a.brightness = v,
        Field::Highlights => a.highlights = v,
        Field::Shadows => a.shadows = v,
        Field::Whites => a.whites = v,
        Field::Blacks => a.blacks = v,
        Field::Temperature => a.temperature = v,
        Field::Tint => a.tint = v,
        Field::Vibrance => a.vibrance = v,
        Field::Saturation => a.saturation = v,
        Field::Clarity => a.clarity = v,
        Field::Texture => a.texture = v,
        Field::Dehaze => a.dehaze = v,
        Field::Sharpness => a.sharpness = v.clamp(0., 1.),
        Field::Noise => a.noise_reduction = v.clamp(0., 1.),
        Field::Vignette => a.vignette = v,
        Field::Grain => a.grain = v.clamp(0., 1.),
        Field::Gamma => a.gamma = v.clamp(0.2, 5.),
        Field::BlackPoint => a.black_point = v.clamp(0., a.white_point - 0.05),
        Field::WhitePoint => a.white_point = v.clamp(a.black_point + 0.05, 1.),
        Field::HslHue => a.hsl_hue[band] = v,
        Field::HslSat => a.hsl_saturation[band] = v,
        Field::HslLum => a.hsl_luminance[band] = v,
        Field::CurveShadows | Field::CurveDarks | Field::CurveLights | Field::CurveHighlights => {
            let i = match f {
                Field::CurveShadows => 0,
                Field::CurveDarks => 1,
                Field::CurveLights => 2,
                _ => 3,
            };
            curve[i] = v;
            a.curve_master = if curve.iter().all(|x| x.abs() < 1e-4) {
                Curve::default()
            } else {
                Curve::parametric(curve[0], curve[1], curve[2], curve[3])
            };
        }
        Field::SplitShadowHue => a.split_shadow_hue = v,
        Field::SplitShadowAmount => a.split_shadow_strength = v.clamp(0., 1.),
        Field::SplitHighlightHue => a.split_highlight_hue = v,
        Field::SplitHighlightAmount => a.split_highlight_strength = v.clamp(0., 1.),
        Field::LutStrength => a.lut_strength = v.clamp(0., 1.),
        Field::Opacity => {}
    }
}

#[cfg(feature = "faces")]
fn scan_with_optional_faces(
    photo: &photo_core::Photo,
    profile: &photo_privacy::Profile,
    face_model: Option<&Path>,
) -> Result<ScanReport, String> {
    let face_regions = if let Some(mp) = face_model {
        let det = photo_privacy::faces::FaceDetector::load(mp).map_err(|e| format!("{e:#}"))?;
        let boxes = det
            .detect(&photo.pixels, 0.7, 0.35)
            .map_err(|e| format!("{e:#}"))?;
        Some(boxes)
    } else {
        None
    };
    Ok(photo_privacy::scan_with_faces(
        photo_privacy::ScanInput {
            bytes: &photo.bytes,
            pixels: &photo.pixels,
            ocr_text: None,
            extra_text: vec![],
        },
        profile,
        face_regions.as_deref(),
    ))
}

#[cfg(not(feature = "faces"))]
fn scan_with_optional_faces(
    photo: &photo_core::Photo,
    profile: &photo_privacy::Profile,
    _face_model: Option<&Path>,
) -> Result<ScanReport, String> {
    Ok(photo_pipeline::scan_photo(photo, profile))
}

#[cfg(feature = "c2pa")]
fn sign_exported_c2pa(
    saved: &Path,
    edits: &EditStack,
    cert_path: &Path,
    key_path: &Path,
) -> anyhow::Result<()> {
    let cert_pem = std::fs::read(cert_path)?;
    let key_pem = std::fs::read(key_path)?;
    let bytes = std::fs::read(saved)?;
    let mut actions = vec!["c2pa.metadata_removed"];
    if edits
        .active()
        .iter()
        .any(|op| matches!(op, Operation::Crop(_) | Operation::CropAspect(_)))
    {
        actions.push("c2pa.cropped");
    }
    if edits.active().iter().any(|op| op.is_secure_redaction()) {
        actions.push("c2pa.redacted");
    }
    let signed = photo_privacy::c2pa_cred::sign_bytes(&bytes, &cert_pem, &key_pem, &actions)?;
    std::fs::write(saved, signed)?;
    Ok(())
}

#[cfg(not(feature = "c2pa"))]
fn sign_exported_c2pa(
    _saved: &Path,
    _edits: &EditStack,
    _cert_path: &Path,
    _key_path: &Path,
) -> anyhow::Result<()> {
    anyhow::bail!("Built without the `c2pa` feature; rebuild with `--features c2pa`.")
}

/// Background media inspection: bounded read, content detection, structure, audio analysis.
fn load_media(path: &Path) -> MediaLoad {
    let mut l = MediaLoad {
        info: None,
        mismatch: false,
        bytes: 0,
        analysis: None,
        wave: None,
        spectro: None,
        video_preview: None,
        video_duration_s: 0.0,
        error: None,
    };
    let r = (|| -> anyhow::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        anyhow::ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Not a regular file"
        );
        l.bytes = meta.len();
        let mut head = vec![0u8; 512];
        let n = std::io::Read::read(&mut std::fs::File::open(path)?, &mut head)?;
        let det = photo_media::detect(&head[..n]);
        l.mismatch = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| photo_media::detect::extension_mismatch(&det, e));
        if meta.len() <= 1 << 30 {
            let bytes = std::fs::read(path)?;
            l.info = Some(photo_media::inspect(&bytes)?);
        } else {
            l.info = Some(photo_media::MediaInfo {
                kind: det.kind,
                container: det.container.into(),
                duration_s: None,
                tracks: vec![],
                findings: vec![],
                warnings: vec![
                    "Larger than 1 GiB: structure not inspected in the desktop app".into(),
                ],
            });
        }
        if det.kind == photo_media::MediaKind::Audio {
            let a = photo_media::audio::analyze(path)?;
            l.wave = Some(handle(&crate::widgets::waveform_image(
                &a.waveform,
                1200,
                120,
            )));
            l.spectro = Some(handle(&crate::widgets::spectrogram_image(
                &a.spectrogram,
                1200,
                128,
            )));
            l.analysis = Some(a);
        }
        #[cfg(feature = "video")]
        if det.kind == photo_media::MediaKind::Video
            && matches!(det.container, "MP4" | "MOV")
            && let Ok(summary) = photo_media::video::probe_video(path)
        {
            l.video_duration_s = summary.duration_s;
            if let Ok(frame) = photo_media::video::decode_frame_at(path, 0.0) {
                l.video_preview = Some(image::Handle::from_rgba(
                    frame.width,
                    frame.height,
                    frame.rgba,
                ));
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        l.error = Some(format!("{e:#}"));
    }
    l
}

fn unique_out(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    (1..1000)
        .map(|n| {
            dir.join(if n == 1 {
                format!("{stem}-private.{ext}")
            } else {
                format!("{stem}-private-{n}.{ext}")
            })
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{stem}-private-x.{ext}")))
}

#[cfg(feature = "voice-protect")]
fn protect_voice_media(
    path: &Path,
    out_dir: &Path,
    model_dir: &Path,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<photo_media::audio::Comparison>,
    Option<f64>,
)> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let out = unique_out(out_dir, &format!("{stem}-protected"), "wav");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let r = photo_media::ecapa::protect_wav(
        path,
        &out,
        model_dir,
        photo_media::protect::Settings::default(),
        &cancel,
    )?;
    let cmp = photo_media::audio::compare(path, &out).ok();
    let lufs = photo_media::audio::analyze(&out)
        .ok()
        .and_then(|a| a.integrated_lufs);
    Ok((out, serde_json::to_string_pretty(&r)?, cmp, lufs))
}

#[cfg(not(feature = "voice-protect"))]
fn protect_voice_media(
    _path: &Path,
    _out_dir: &Path,
    _model_dir: &Path,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<photo_media::audio::Comparison>,
    Option<f64>,
)> {
    anyhow::bail!(
        "Built without the `voice-protect` feature; rebuild with `--features voice-protect`."
    )
}

#[cfg(feature = "video")]
fn decode_video_preview(path: &Path, target_s: f64) -> Result<(f64, u32, u32, Vec<u8>), String> {
    let f = photo_media::video::decode_frame_at(path, target_s).map_err(|e| format!("{e:#}"))?;
    Ok((f.pts_s, f.width, f.height, f.rgba))
}

#[cfg(not(feature = "video"))]
fn decode_video_preview(_path: &Path, _target_s: f64) -> Result<(f64, u32, u32, Vec<u8>), String> {
    Err("Built without the `video` feature".into())
}

#[cfg(feature = "video")]
fn redact_video_media(
    path: &Path,
    out_dir: &Path,
    rect_str: &str,
    from_str: &str,
    to_str: &str,
    blur: bool,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<photo_media::audio::Comparison>,
    Option<f64>,
)> {
    let parts: Vec<u32> = rect_str
        .split(',')
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!("Rect must be x,y,w,h"))?;
    anyhow::ensure!(parts.len() == 4, "Rect must be x,y,w,h");
    let rect = photo_media::video::RedactRect {
        x: parts[0],
        y: parts[1],
        width: parts[2],
        height: parts[3],
        from_s: from_str
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid from_s"))?,
        to_s: to_str
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid to_s"))?,
        blur,
    };
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let out = unique_out(out_dir, &format!("{stem}-redacted"), "mp4");
    let r = photo_media::video::redact_mp4(path, &out, &[rect])?;
    Ok((out, serde_json::to_string_pretty(&r)?, None, None))
}

#[cfg(not(feature = "video"))]
fn redact_video_media(
    _path: &Path,
    _out_dir: &Path,
    _rect_str: &str,
    _from_str: &str,
    _to_str: &str,
    _blur: bool,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<photo_media::audio::Comparison>,
    Option<f64>,
)> {
    anyhow::bail!("Built without the `video` feature; rebuild with `--features video`.")
}

/// Exports a media file through the matching verified path. Never overwrites.
fn export_media(
    path: &Path,
    dir: &Path,
    kind: photo_media::MediaKind,
    container: &str,
    segs: &[photo_media::audio::Segment],
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<photo_media::audio::Comparison>,
    Option<f64>,
)> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("media");
    match kind {
        photo_media::MediaKind::Audio => {
            let out = unique_out(dir, stem, "wav");
            let r = photo_media::audio::export_wav(path, &out, segs)?;
            let cmp = photo_media::audio::compare(path, &out).ok();
            let lufs = photo_media::audio::analyze(&out)
                .ok()
                .and_then(|a| a.integrated_lufs);
            Ok((out, serde_json::to_string_pretty(&r)?, cmp, lufs))
        }
        photo_media::MediaKind::Video if matches!(container, "MP4" | "MOV" | "3GP" | "M4A") => {
            anyhow::ensure!(
                segs.is_empty(),
                "Audio segment redaction inside video files is not implemented (no video encoder); export the audio separately"
            );
            let bytes = std::fs::read(path)?;
            let (out_bytes, r) = photo_media::mp4::strip(&bytes)?;
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mp4")
                .to_ascii_lowercase();
            let saved = photo_core::save_unique(
                dir,
                &format!(
                    "{}-private",
                    stem.chars()
                        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '_'
                        })
                        .take(80)
                        .collect::<String>()
                ),
                &ext,
                &out_bytes,
            )?;
            Ok((saved, serde_json::to_string_pretty(&r)?, None, None))
        }
        photo_media::MediaKind::Subtitle => {
            let text = std::fs::read_to_string(path)?;
            let cues = photo_media::subtitles::parse(&text)?;
            let found = photo_media::subtitles::scan(&cues, &photo_privacy::Country::ALL);
            let flagged: Vec<usize> = found.iter().map(|f| f.cue).collect();
            let vtt = container == "WebVTT";
            let out = photo_core::save_unique(
                dir,
                &format!("{stem}-private"),
                if vtt { "vtt" } else { "srt" },
                photo_media::subtitles::write(&cues, &flagged, vtt).as_bytes(),
            )?;
            Ok((
                out,
                format!(
                    "{} cue(s) redacted; timestamps of findings:\n{}",
                    flagged.len(),
                    found
                        .iter()
                        .map(|f| format!(
                            "{:.1}s {:?}",
                            f.start_ms as f64 / 1000.,
                            f.finding.category
                        ))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                None,
                None,
            ))
        }
        _ => anyhow::bail!(
            "Export is not available for {container}: protection and re-encoding of this format are not implemented"
        ),
    }
}

/// Decodes up to two minutes for preview playback.
fn preview_pcm(path: &Path) -> anyhow::Result<Pcm> {
    let mut d = photo_media::audio::Decoder::open(path)?;
    let (ch, rate) = (d.info.channels, d.info.sample_rate);
    let limit = rate as usize * ch * 120;
    let mut v = Vec::new();
    while v.len() < limit {
        match d.next_block()? {
            Some(b) => v.extend(b),
            None => break,
        }
    }
    v.truncate(limit);
    Ok(Pcm(Arc::new(v), ch as u16, rate))
}

/// Output dimensions of an edit stack, derived from geometry operations.
pub fn full_dims(edits: &EditStack, w: u32, h: u32) -> (u32, u32) {
    let (mut w, mut h) = (w, h);
    for op in edits.ordered() {
        match op {
            Operation::Crop(r) => {
                if let Some(c) = r.clamp_to(w, h) {
                    (w, h) = (c.width, c.height)
                }
            }
            Operation::CropAspect(a) => {
                if let Ok(c) = a.centered_crop(w, h) {
                    (w, h) = (c.width, c.height)
                }
            }
            Operation::Resize { width, height } => (w, h) = (*width, *height),
            Operation::FitWithin {
                max_width,
                max_height,
            } => {
                if w > *max_width || h > *max_height {
                    let s = (f64::from(*max_width) / f64::from(w))
                        .min(f64::from(*max_height) / f64::from(h));
                    (w, h) = (
                        ((f64::from(w) * s).round() as u32).max(1),
                        ((f64::from(h) * s).round() as u32).max(1),
                    );
                }
            }
            Operation::Rotate90 | Operation::Rotate270 => (w, h) = (h, w),
            Operation::Straighten { .. } => {
                let k = (w.max(h) as f64 / 1024.).max(1.);
                let probe = ::image::DynamicImage::new_rgb8(
                    ((w as f64 / k) as u32).max(1),
                    ((h as f64 / k) as u32).max(1),
                );
                if let Ok(o) =
                    EditStack::from_operations(vec![op.clone()]).and_then(|s| s.render(&probe))
                {
                    (w, h) = (
                        (o.width() as f64 * k).round() as u32,
                        (o.height() as f64 * k).round() as u32,
                    );
                }
            }
            Operation::Perspective { width, height, .. }
            | Operation::Canvas { width, height, .. } => (w, h) = (*width, *height),
            _ => {}
        }
    }
    (w.max(1), h.max(1))
}

impl App {
    fn new(safe_mode: bool) -> Self {
        let data_dir = data_dir();
        let _ = std::fs::create_dir_all(&data_dir);
        photo_log::configure(photo_log::Config {
            directory: Some(data_dir.join("logs")),
            ..Default::default()
        });
        let session = photo_project::SessionGuard::acquire(&data_dir).ok();
        let unclean = session.as_ref().is_some_and(|s| s.previous_unclean);
        photo_log::install_panic_hook(session.as_ref().map(|s| s.crash_marker()));
        photo_project::clean_temp_files(&data_dir);
        photo_log::info(
            Component::App,
            Code::Startup,
            if safe_mode {
                "started in safe mode"
            } else {
                "started"
            },
        );
        let autosave_offer = if safe_mode {
            None
        } else {
            std::fs::read(photo_project::autosave_path(
                &data_dir.join("session.apdproj"),
            ))
            .ok()
            .and_then(|b| photo_project::Project::from_bytes(&b).ok())
            .filter(|p| !p.entries.is_empty())
        };
        let catalog = if safe_mode {
            None
        } else {
            photo_catalog::Catalog::open(&data_dir.join("catalog.db"))
                .inspect_err(|e| {
                    photo_log::error(Component::Catalog, Code::Database, &format!("{e:#}"))
                })
                .ok()
        };
        let sys = sysinfo::System::new().ok();
        let mut app = Self {
            mode: Mode::Professional,
            appearance: theme::Appearance::Graphite,
            ui_scale: 1.0,
            menu: None,
            fullscreen: false,
            panes: default_panes(),
            closed_groups: [
                "Detail",
                "Effects",
                "Levels",
                "Color Grading",
                "HSL",
                "Tone Curve",
                "LUT",
                "Geometry",
                "Presets",
                "Redaction",
            ]
            .into_iter()
            .collect(),
            entries: vec![],
            selected: None,
            input_path: String::new(),
            output_dir: String::new(),
            status: "Ready".into(),
            busy: None,
            cancel: Arc::new(AtomicBool::new(false)),
            tool: Tool::Inspect,
            view: ViewMode::Edited,
            clipping: false,
            thirds: false,
            pointer: Point::ORIGIN,
            pressed: false,
            drag_from: None,
            points: vec![],
            selection: None,
            gradient: None,
            pixel_probe: None,
            method: Method::Black,
            strength: 16,
            brush_radius: 20.,
            straighten: 0.,
            lens: 0.,
            band: 0,
            active_layer: None,
            c2pa_cert_path: None,
            c2pa_key_path: None,
            editing: false,
            profile: "Social Media".into(),
            focused_finding: None,
            format: ExportFormat::Png,
            quality: 92,
            high_assurance: true,
            auto_redact: false,
            essential_aspect: None,
            project_path: None,
            data_dir,
            autosave_offer,
            pending_restore: None,
            unclean_shutdown: unclean,
            safe_mode,
            _session: session,
            dirty_project: false,
            catalog,
            cache_enabled: false,
            lib_search: String::new(),
            lib_rating: 0,
            lib_rows: vec![],
            tag_input: String::new(),
            page: TechPage::Diagnostics,
            log_level: Level::Info,
            log_component: None,
            log_search: String::new(),
            code_lookup: String::new(),
            report_preview: None,
            lab: None,
            validation_report: None,
            forensics: None,
            perf: Perf {
                cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                ..Default::default()
            },
            sys,
            media: vec![],
            selected_media: None,
            focus_media: false,
            seg_start: String::new(),
            seg_end: String::new(),
            player: None,
            playing: None,
            voice_model_dir: None,
            face_model_path: None,
            clip_model_path: None,
            clip_manifest_path: None,
            batch_validation: vec![],
            show_wizard: true,
            wizard_step: 0,
            tutorial_step: 0,
            update_status: None,
            download_status: None,
        };
        app.refresh_library();
        app
    }

    pub fn theme_accent(&self) -> [u8; 3] {
        let c = crate::theme::theme(self.appearance).palette().primary;
        [(c.r * 255.) as u8, (c.g * 255.) as u8, (c.b * 255.) as u8]
    }
    pub fn note(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }
    pub fn current(&self) -> Option<&Entry> {
        self.selected.and_then(|i| self.entries.get(i))
    }
    fn current_mut(&mut self) -> Option<&mut Entry> {
        self.selected.and_then(|i| self.entries.get_mut(i))
    }
    pub fn group_open(&self, g: &'static str) -> bool {
        !self.closed_groups.contains(g)
    }
    /// The adjustment layer the Color Studio controls edit.
    pub fn layer(&self) -> Option<(usize, &Operation)> {
        let e = self.current()?;
        let k = self.active_layer.or_else(|| {
            e.edits.active().iter().position(
                |o| matches!(o, Operation::Adjust { name, mask: None, .. } if name == "Basic"),
            )
        })?;
        e.edits
            .active()
            .get(k)
            .filter(|o| matches!(o, Operation::Adjust { .. }))
            .map(|o| (k, o))
    }
    pub fn layer_adjustments(&self) -> Adjustments {
        match self.layer() {
            Some((_, Operation::Adjust { adjustments, .. })) => (**adjustments).clone(),
            _ => Adjustments::default(),
        }
    }
    fn snapshot(&mut self) {
        if let Some(e) = self.current_mut() {
            e.undo.push(e.edits.clone());
            if e.undo.len() > 200 {
                e.undo.remove(0);
            }
            e.redo.clear();
        }
        self.dirty_project = true;
    }
    fn ensure_layer(&mut self) -> Option<usize> {
        let i = self.selected?;
        if let Some((k, _)) = self.layer() {
            self.active_layer = Some(k);
            return Some(k);
        }
        let op = Operation::Adjust {
            name: "Basic".into(),
            adjustments: Box::default(),
            mask: None,
            invert: false,
            opacity: 1.,
            blend: BlendMode::Normal,
            visible: true,
        };
        self.entries[i].edits.push(op).ok()?;
        let k = self.entries[i].edits.active().len() - 1;
        self.active_layer = Some(k);
        Some(k)
    }
    fn modify_layer(
        &mut self,
        f: impl FnOnce(&mut Adjustments, &mut f32, &mut BlendMode),
    ) -> Task<Message> {
        if self.selected.is_none() {
            return Task::none();
        }
        if !self.editing {
            self.snapshot();
            self.editing = true;
        }
        let Some(k) = self.ensure_layer() else {
            return Task::none();
        };
        let i = self.selected.expect("checked");
        if let Some(Operation::Adjust {
            name,
            adjustments,
            mask,
            invert,
            mut opacity,
            mut blend,
            visible,
        }) = self.entries[i].edits.active().get(k).cloned()
        {
            let mut a = *adjustments;
            f(&mut a, &mut opacity, &mut blend);
            if let Err(e) = self.entries[i].edits.replace(
                k,
                Operation::Adjust {
                    name,
                    adjustments: Box::new(a),
                    mask,
                    invert,
                    opacity,
                    blend,
                    visible,
                },
            ) {
                self.note(format!("{e:#}"));
            }
        }
        self.after_edit(i)
    }
    fn after_edit(&mut self, i: usize) -> Task<Message> {
        if let Some(e) = self.entries.get_mut(i) {
            e.export = None;
        }
        self.dirty_project = true;
        self.request_render(i)
    }
    /// Coalesced proxy render: at most one in flight per photo; the latest state wins.
    pub fn request_render(&mut self, i: usize) -> Task<Message> {
        let clip = self.clipping;
        let Some(e) = self.entries.get_mut(i) else {
            return Task::none();
        };
        e.generation += 1;
        if e.rendering {
            e.dirty = true;
            return Task::none();
        }
        let Some(proxy) = e.proxy.clone() else {
            return self.load_proxy(i);
        };
        e.rendering = true;
        e.dirty = false;
        let generation = e.generation;
        let (fw, fh) = (e.width, e.height);
        let edits = e.edits.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || -> Result<Edited, String> {
                    let t = Instant::now();
                    let s = proxy.width() as f32 / fw.max(1) as f32;
                    let out = edits
                        .scaled(s)
                        .render(&proxy)
                        .map_err(|e| format!("{e:#}"))?
                        .to_rgba8();
                    let hist = photo_edit::color::histogram(&out);
                    let shown = if clip {
                        photo_edit::color::clipping_overlay(&out)
                    } else {
                        out.clone()
                    };
                    Ok(Edited {
                        handle: handle(&shown),
                        before: handle(&proxy.to_rgba8()),
                        full: full_dims(&edits, fw, fh),
                        proxy: (out.width(), out.height()),
                        hist_img: handle(&crate::widgets::histogram_image(&hist, 256, 90, true)),
                        hist: Arc::new(hist),
                        pixels: Arc::new(out),
                        ms: t.elapsed().as_secs_f64() * 1000.,
                    })
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |r| Message::Rendered(i, generation, r),
        )
    }
    fn load_proxy(&mut self, i: usize) -> Task<Message> {
        let path = self.entries[i].path.clone();
        // Keep at most four decoded proxies in memory (sized for 8 GB systems).
        let cached: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(k, e)| e.proxy.is_some() && *k != i && Some(*k) != self.selected)
            .map(|(k, _)| k)
            .collect();
        for k in cached.iter().take(cached.len().saturating_sub(3)) {
            self.entries[*k].proxy = None;
        }
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let p = photo_core::load(&path).map_err(|e| format!("{e:#}"))?;
                    Ok(Arc::new(p.pixels.thumbnail(PROXY_EDGE, PROXY_EDGE)))
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            move |r| Message::ProxyLoaded(i, r),
        )
    }
    fn project(&self) -> photo_project::Project {
        photo_project::Project {
            name: self
                .project_path
                .as_deref()
                .map(short)
                .unwrap_or_else(|| "Session".into()),
            entries: self
                .entries
                .iter()
                .map(|e| photo_project::ProjectEntry {
                    path: e.path.clone(),
                    edits: e.edits.clone(),
                    rating: e.rating,
                    tags: vec![],
                    flagged: e.flagged,
                })
                .collect(),
            settings: photo_project::Settings {
                profile: self.profile.clone(),
                format: self.format.extension().into(),
                quality: self.quality,
                mode: self.mode.to_string(),
                voice_model_dir: self
                    .voice_model_dir
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                face_model_path: self
                    .face_model_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                c2pa_cert_path: self
                    .c2pa_cert_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                c2pa_key_path: self
                    .c2pa_key_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                ui_scale: self.ui_scale,
            },
            ..Default::default()
        }
    }
    fn refresh_library(&mut self) {
        if let Some(c) = &self.catalog {
            match c.query(&photo_catalog::Filter {
                text: self.lib_search.clone(),
                min_rating: self.lib_rating,
                ..Default::default()
            }) {
                Ok(r) => self.lib_rows = r,
                Err(e) => photo_log::error(Component::Catalog, Code::Database, &format!("{e:#}")),
            }
        }
    }
    fn selection_rect(&self) -> Option<Rect> {
        match &self.selection {
            Some(Selection::Rect(r)) | Some(Selection::Ellipse(r)) => Some(*r),
            _ => None,
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Tick => self.refresh_perf(),
            Message::Autosave => {
                if self.dirty_project && !self.safe_mode && !self.entries.is_empty() {
                    match photo_project::autosave(
                        &self.data_dir.join("session.apdproj"),
                        &self.project(),
                    ) {
                        Ok(()) => self.dirty_project = false,
                        Err(e) => photo_log::error(
                            Component::Project,
                            Code::ProjectSaveFailed,
                            &format!("autosave {e:#}"),
                        ),
                    }
                }
            }
            Message::MenuToggle(m) => self.menu = if self.menu == Some(m) { None } else { Some(m) },
            Message::MenuClose => self.menu = None,
            Message::Mode(m) => {
                self.mode = m;
                self.show_wizard = false;
                self.menu = None;
                if m == Mode::Technical {
                    self.refresh_perf();
                }
            }
            Message::Appearance(a) => {
                self.appearance = a;
                self.menu = None;
            }
            Message::UiScale(s) => {
                self.ui_scale = s.clamp(0.85, 2.0);
                self.menu = None;
            }
            Message::Fullscreen(f) => {
                self.fullscreen = f;
                self.menu = None;
                let mode = if f {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                };
                return window::get_latest().and_then(move |id| window::change_mode(id, mode));
            }
            Message::Group(g) => {
                if !self.closed_groups.remove(g) {
                    self.closed_groups.insert(g);
                }
            }
            Message::InputPath(p) => self.input_path = p,
            Message::OutputDir(p) => self.output_dir = p,
            Message::ImportPath if !self.input_path.trim().is_empty() => {
                let path = PathBuf::from(std::mem::take(&mut self.input_path).trim());
                return self.update(Message::Picked(vec![path]));
            }
            Message::OpenDialog => {
                self.menu = None;
                let mut exts = vec!["jpg", "jpeg", "png", "webp", "tif", "tiff"];
                exts.extend(photo_core::raw::EXTENSIONS);
                exts.extend(["bmp", "gif"]);
                let media = photo_pipeline::batch::MEDIA_EXTENSIONS;
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Photographs", &exts)
                            .add_filter("Audio, video and subtitles", &media)
                            .pick_files()
                            .await
                            .map(|v| v.into_iter().map(|f| f.path().to_path_buf()).collect())
                            .unwrap_or_default()
                    },
                    Message::Picked,
                );
            }
            Message::OpenFolderDialog => {
                self.menu = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|f| vec![f.path().to_path_buf()])
                            .unwrap_or_default()
                    },
                    Message::Picked,
                );
            }
            Message::Picked(paths) if !paths.is_empty() => {
                if self.busy.is_some() {
                    self.note("Busy: wait for the current task to finish.");
                    return Task::none();
                }
                let opts = photo_pipeline::batch::CollectOptions {
                    recursive: true,
                    max_files: QUEUE_LIMIT,
                    include_media: true,
                    ..Default::default()
                };
                let (files, skipped) = match photo_pipeline::batch::collect(&paths, &opts) {
                    Ok(c) => (
                        c.files,
                        c.skipped
                            .into_iter()
                            .map(|(p, w)| format!("Skipped {}: {w}", short(&p)))
                            .collect::<Vec<_>>(),
                    ),
                    Err(e) => {
                        self.note(format!("{e:#}"));
                        return Task::none();
                    }
                };
                let files: Vec<PathBuf> = files
                    .into_iter()
                    .filter(|f| {
                        !self.entries.iter().any(|e| &e.path == f)
                            && !self.media.iter().any(|m| &m.path == f)
                    })
                    .collect();
                // Route by content, not extension: photographs go to the photo collection.
                let (files, media): (Vec<PathBuf>, Vec<PathBuf>) =
                    files.into_iter().partition(|f| {
                        let mut head = [0u8; 512];
                        let n = std::fs::File::open(f)
                            .and_then(|mut h| std::io::Read::read(&mut h, &mut head))
                            .unwrap_or(0);
                        let d = photo_media::detect(&head[..n]);
                        matches!(
                            d.kind,
                            photo_media::MediaKind::Image | photo_media::MediaKind::RawPhoto
                        ) || d.container == "GIF"
                            || d.kind == photo_media::MediaKind::Unknown
                    });
                let media_tasks: Vec<Task<Message>> = media
                    .into_iter()
                    .map(|path| {
                        self.media.push(MediaEntry {
                            name: short(&path),
                            path: path.clone(),
                            bytes: 0,
                            info: None,
                            mismatch: false,
                            analysis: None,
                            wave: None,
                            spectro: None,
                            segments: vec![],
                            status: "Inspecting".into(),
                            output: None,
                            report: None,
                            comparison: None,
                            out_lufs: None,
                            video_preview: None,
                            video_time_s: 0.0,
                            video_duration_s: 0.0,
                            video_playing: false,
                            video_rect: "16,16,64,64".into(),
                            video_from: "0.0".into(),
                            video_to: "1.0".into(),
                            video_blur: false,
                            video_track_warnings: vec![],
                        });
                        Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    (path.clone(), Box::new(load_media(&path)))
                                })
                                .await
                                .expect("join")
                            },
                            |(p, l)| Message::MediaLoaded(p, l),
                        )
                    })
                    .collect();
                if files.is_empty() {
                    if self.selected_media.is_none() && !self.media.is_empty() {
                        self.selected_media = Some(0);
                        self.focus_media = true;
                    }
                    return Task::batch(media_tasks);
                }
                let media_batch = Task::batch(media_tasks);
                if self.entries.len() + files.len() > QUEUE_LIMIT {
                    self.note(format!("The collection is limited to {QUEUE_LIMIT} photographs; use the CLI batch command for larger folders."));
                    return Task::none();
                }
                self.busy = Some(format!("Importing {} photograph(s)", files.len()));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut ok = vec![];
                            let mut errors = skipped;
                            for path in files {
                                let t = Instant::now();
                                match photo_core::load(&path) {
                                    Ok(p) => {
                                        photo_log::event(
                                            Level::Debug,
                                            Component::Decode,
                                            Code::Other,
                                            "decoded",
                                        )
                                        .duration(t.elapsed())
                                        .emit();
                                        if p.raw.is_some() {
                                            photo_log::info(
                                                Component::Raw,
                                                Code::RawPreview,
                                                "RAW opened via embedded preview",
                                            );
                                        }
                                        ok.push(Imported {
                                            width: p.pixels.width(),
                                            height: p.pixels.height(),
                                            bytes: p.bytes.len() as u64,
                                            raw: p.raw.map(|k| format!("{k:?}")),
                                            thumb: handle(&p.pixels.thumbnail(220, 160).to_rgba8()),
                                            proxy: Arc::new(
                                                p.pixels.thumbnail(PROXY_EDGE, PROXY_EDGE),
                                            ),
                                            path,
                                        });
                                    }
                                    Err(e) => {
                                        photo_log::warn(
                                            Component::Decode,
                                            Code::DecodeFailed,
                                            &format!("{e:#}"),
                                        );
                                        errors.push(format!("{}: {e:#}", short(&path)));
                                    }
                                }
                            }
                            (Ok(ok), errors)
                        })
                        .await
                        .unwrap_or_else(|e| (Err(e.to_string()), vec![]))
                    },
                    |(r, errs)| Message::Imported(r, errs),
                )
                .chain(media_batch);
            }
            Message::Imported(r, errors) => {
                self.busy = None;
                match r {
                    Ok(list) => {
                        let n = list.len();
                        for im in list {
                            let catalog_id =
                                self.catalog.as_ref().and_then(|c| c.add(&im.path).ok());
                            self.entries.push(Entry {
                                name: short(&im.path),
                                path: im.path,
                                width: im.width,
                                height: im.height,
                                bytes: im.bytes,
                                raw: im.raw,
                                thumb: im.thumb,
                                edits: EditStack::new(),
                                undo: vec![],
                                redo: vec![],
                                generation: 0,
                                rendering: false,
                                dirty: false,
                                proxy: Some(im.proxy),
                                edited: None,
                                scan: None,
                                export: None,
                                rating: 0,
                                flagged: false,
                                catalog_id,
                                status: "Ready".into(),
                            });
                        }
                        if let Some(p) = self.pending_restore.take() {
                            for pe in p.entries {
                                if let Some(e) = self.entries.iter_mut().find(|e| e.path == pe.path)
                                {
                                    e.edits = pe.edits;
                                    e.rating = pe.rating;
                                    e.flagged = pe.flagged;
                                    e.edited = None;
                                }
                            }
                            self.note("Project restored. Edits are reversible until export.");
                        } else {
                            self.note(format!(
                                "Imported {n} photograph(s){}.",
                                if errors.is_empty() {
                                    String::new()
                                } else {
                                    format!("; {} could not be read: {}", errors.len(), errors[0])
                                }
                            ));
                        }
                        self.refresh_library();
                        self.dirty_project = true;
                        let mut tasks = vec![];
                        if self.selected.is_none() && !self.entries.is_empty() {
                            tasks.push(self.update(Message::Select(0)));
                        } else if let Some(i) = self.selected {
                            tasks.push(self.request_render(i));
                        }
                        if self.mode == Mode::Essential {
                            tasks.push(self.update(Message::Scan));
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::Select(i) if i < self.entries.len() => {
                self.selected = Some(i);
                self.focus_media = false;
                self.active_layer = None;
                self.focused_finding = None;
                self.selection = None;
                self.points.clear();
                self.gradient = None;
                self.forensics = None;
                if self.entries[i].edited.is_none() {
                    return self.request_render(i);
                }
            }
            Message::Remove(i) if i < self.entries.len() && self.busy.is_none() => {
                self.entries.remove(i);
                self.selected = if self.entries.is_empty() {
                    None
                } else {
                    Some(i.min(self.entries.len() - 1))
                };
                self.dirty_project = true;
            }
            Message::ClearAll if self.busy.is_none() => {
                self.entries.clear();
                self.selected = None;
                self.dirty_project = true;
                self.menu = None;
                self.note("Collection cleared. Exported files and originals were kept.");
            }
            Message::Rating(i, r) => {
                if let Some(e) = self.entries.get_mut(i) {
                    e.rating = r.min(5);
                    if let (Some(c), Some(id)) = (&self.catalog, e.catalog_id) {
                        let _ = c.set_rating(id, e.rating);
                    }
                    self.dirty_project = true;
                    self.refresh_library();
                }
            }
            Message::Flag(i) => {
                if let Some(e) = self.entries.get_mut(i) {
                    e.flagged = !e.flagged;
                    if let (Some(c), Some(id)) = (&self.catalog, e.catalog_id) {
                        let _ = c.set_flag(id, e.flagged);
                    }
                }
            }
            Message::BrowseOutput => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::OutputPicked,
                );
            }
            Message::OutputPicked(Some(p)) => self.output_dir = p.to_string_lossy().into_owned(),
            Message::Tool(t) => {
                self.tool = t;
                self.points.clear();
                self.drag_from = None;
                if !matches!(t, Tool::Linear | Tool::Radial) {
                    self.gradient = None;
                }
            }
            Message::View(v) => self.view = v,
            Message::Clipping => {
                self.clipping = !self.clipping;
                if let Some(i) = self.selected {
                    return self.request_render(i);
                }
            }
            Message::Thirds => self.thirds = !self.thirds,
            Message::PointerMove(p) => {
                self.pointer = p;
                if let Some(ed) = self.current().and_then(|e| e.edited.as_ref()) {
                    let s = ed.proxy.0 as f32 / ed.full.0.max(1) as f32;
                    let (px, py) = ((p.x * s) as u32, (p.y * s) as u32);
                    if px < ed.pixels.width() && py < ed.pixels.height() {
                        self.pixel_probe =
                            Some((p.x as u32, p.y as u32, ed.pixels.get_pixel(px, py).0));
                    }
                }
                if self.pressed {
                    match self.tool {
                        Tool::Lasso | Tool::Brush => {
                            if self
                                .points
                                .last()
                                .is_none_or(|l| (l.0 - p.x).abs() + (l.1 - p.y).abs() > 2.)
                                && self.points.len() < 5000
                            {
                                self.points.push((p.x, p.y));
                            }
                        }
                        Tool::Rect | Tool::Ellipse | Tool::Crop | Tool::Radial => {
                            if let Some(a) = self.drag_from {
                                let r = Rect::new(
                                    a.x.min(p.x) as u32,
                                    a.y.min(p.y) as u32,
                                    ((a.x - p.x).abs() as u32).max(1),
                                    ((a.y - p.y).abs() as u32).max(1),
                                );
                                self.selection =
                                    Some(if matches!(self.tool, Tool::Ellipse | Tool::Radial) {
                                        Selection::Ellipse(r)
                                    } else {
                                        Selection::Rect(r)
                                    });
                            }
                        }
                        Tool::Linear => {
                            if let Some(a) = self.drag_from {
                                self.gradient = Some(((a.x, a.y), (p.x, p.y)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::PointerDown => {
                self.pressed = true;
                self.drag_from = Some(self.pointer);
                match self.tool {
                    Tool::Polygon => {
                        if self.points.len() < 1000 {
                            self.points.push((self.pointer.x, self.pointer.y))
                        }
                    }
                    Tool::Lasso | Tool::Brush => {
                        self.points = vec![(self.pointer.x, self.pointer.y)]
                    }
                    _ => {}
                }
            }
            Message::PointerUp => {
                self.pressed = false;
                match self.tool {
                    Tool::Lasso if self.points.len() >= 3 => {
                        self.selection = Some(Selection::Polygon(std::mem::take(&mut self.points)))
                    }
                    Tool::Brush if !self.points.is_empty() => {
                        self.selection = Some(Selection::Brush {
                            points: std::mem::take(&mut self.points),
                            radius: self.brush_radius,
                        })
                    }
                    _ => {}
                }
            }
            Message::Escape => {
                self.menu = None;
                self.selection = None;
                self.points.clear();
                self.gradient = None;
                self.focused_finding = None;
                if self.fullscreen {
                    return self.update(Message::Fullscreen(false));
                }
            }
            Message::Enter => {
                if self.tool == Tool::Polygon && self.points.len() >= 3 {
                    self.selection = Some(Selection::Polygon(std::mem::take(&mut self.points)));
                } else if self.tool == Tool::Crop
                    && let Some(r) = self.selection_rect()
                {
                    self.selection = None;
                    return self.update(Message::Apply(Operation::Crop(r)));
                } else if matches!(self.tool, Tool::Linear | Tool::Radial) {
                    return self.update(Message::NewLayer);
                } else if self.selection.is_some() {
                    return self.update(Message::ConcealSelection);
                }
            }
            Message::ProxyLoaded(i, r) => match r {
                Ok(p) => {
                    if let Some(e) = self.entries.get_mut(i) {
                        e.proxy = Some(p);
                    }
                    return self.request_render(i);
                }
                Err(e) => self.note(e),
            },
            Message::Rendered(i, generation, r) => {
                let mut again = false;
                if let Some(e) = self.entries.get_mut(i) {
                    e.rendering = false;
                    match r {
                        Ok(ed) => {
                            self.perf
                                .samples
                                .push(("Preview render (proxy)".into(), ed.ms));
                            e.edited = Some(ed);
                        }
                        Err(err) => {
                            photo_log::error(Component::Edit, Code::EditFailed, &err);
                            self.status = err;
                        }
                    }
                    again = e.dirty || e.generation != generation;
                }
                if self.perf.samples.len() > 400 {
                    self.perf.samples.drain(..200);
                }
                if again {
                    if let Some(e) = self.entries.get_mut(i) {
                        e.generation = e.generation.saturating_sub(1);
                    }
                    return self.request_render(i);
                }
            }
            Message::Apply(op) => {
                let Some(i) = self.selected else {
                    return Task::none();
                };
                self.snapshot();
                self.editing = false;
                let label = op.label();
                if let Err(e) = self.entries[i].edits.push(op) {
                    self.note(format!("{e:#}"));
                    return Task::none();
                }
                self.menu = None;
                self.note(format!("Applied: {label}"));
                return self.after_edit(i);
            }
            Message::Undo => {
                self.menu = None;
                if let Some(i) = self.selected
                    && let Some(prev) = self.entries[i].undo.pop()
                {
                    let cur = std::mem::replace(&mut self.entries[i].edits, prev);
                    self.entries[i].redo.push(cur);
                    self.active_layer = None;
                    self.editing = false;
                    self.note("Undo");
                    return self.after_edit(i);
                }
            }
            Message::Redo => {
                self.menu = None;
                if let Some(i) = self.selected
                    && let Some(next) = self.entries[i].redo.pop()
                {
                    let cur = std::mem::replace(&mut self.entries[i].edits, next);
                    self.entries[i].undo.push(cur);
                    self.active_layer = None;
                    self.editing = false;
                    self.note("Redo");
                    return self.after_edit(i);
                }
            }
            Message::Adjust(f, v) => {
                if !v.is_finite() {
                    return Task::none();
                }
                if f == Field::Opacity {
                    return self.modify_layer(|_, o, _| *o = v.clamp(0., 1.));
                }
                let band = self.band;
                return self.modify_layer(move |a, _, _| set_field(a, f, band, v));
            }
            Message::LayerBlend(b) => {
                self.editing = false;
                return self.modify_layer(move |_, _, bl| *bl = b);
            }
            Message::Commit => self.editing = false,
            Message::ResetLayer => {
                self.editing = false;
                return self.modify_layer(|a, o, b| {
                    *a = Adjustments::default();
                    *o = 1.;
                    *b = BlendMode::Normal;
                });
            }
            Message::AutoTone => {
                self.menu = None;
                let Some(ed) = self.current().and_then(|e| e.edited.as_ref()) else {
                    return Task::none();
                };
                let auto = photo_edit::color::auto_adjust(&ed.pixels);
                self.editing = false;
                self.note(format!(
                    "Auto tone: exposure {:+.2} EV, temperature {:+.2}, tint {:+.2}",
                    auto.exposure, auto.temperature, auto.tint
                ));
                return self.modify_layer(move |a, _, _| {
                    a.exposure = (a.exposure + auto.exposure).clamp(-5., 5.);
                    a.temperature = (a.temperature + auto.temperature).clamp(-1., 1.);
                    a.tint = (a.tint + auto.tint).clamp(-1., 1.);
                });
            }
            Message::Preset(name) => {
                if let Some((_, p)) = photo_edit::color::presets()
                    .into_iter()
                    .find(|(n, _)| *n == name)
                {
                    self.editing = false;
                    return self.modify_layer(move |a, _, _| {
                        let lut = a.lut.take();
                        *a = p;
                        a.lut = lut;
                    });
                }
            }
            Message::Band(b) => self.band = b.min(7),
            Message::Layer(k) => self.active_layer = k,
            Message::LayerToggle(k) => {
                if let Some(i) = self.selected {
                    self.snapshot();
                    if self.entries[i].edits.toggle_visible(k) {
                        return self.after_edit(i);
                    }
                }
            }
            Message::LayerMove(k, up) => {
                if let Some(i) = self.selected {
                    self.snapshot();
                    if self.entries[i].edits.move_layer(k, up) {
                        self.active_layer = None;
                        return self.after_edit(i);
                    }
                }
            }
            Message::LayerDelete(k) => {
                if let Some(i) = self.selected {
                    self.snapshot();
                    if self.entries[i].edits.remove(k) {
                        self.active_layer = None;
                        return self.after_edit(i);
                    }
                }
            }
            Message::NewLayer => {
                let mask = match (&self.selection, self.gradient, self.tool) {
                    (_, Some((a, b)), Tool::Linear) => Some(LayerMask::Linear {
                        x0: a.0,
                        y0: a.1,
                        x1: b.0,
                        y1: b.1,
                    }),
                    (Some(Selection::Ellipse(r)), _, Tool::Radial) => Some(LayerMask::Radial {
                        cx: r.x as f32 + r.width as f32 / 2.,
                        cy: r.y as f32 + r.height as f32 / 2.,
                        rx: (r.width as f32 / 2.).max(1.),
                        ry: (r.height as f32 / 2.).max(1.),
                        feather: 0.6,
                    }),
                    (Some(s), _, _) => Some(LayerMask::Selection(s.clone())),
                    _ => None,
                };
                let Some(i) = self.selected else {
                    return Task::none();
                };
                self.snapshot();
                self.editing = false;
                let name = if mask.is_some() {
                    "Local adjustment"
                } else {
                    "Adjustment"
                };
                let op = Operation::Adjust {
                    name: name.into(),
                    adjustments: Box::default(),
                    mask,
                    invert: false,
                    opacity: 1.,
                    blend: BlendMode::Normal,
                    visible: true,
                };
                if self.entries[i].edits.push(op).is_ok() {
                    self.active_layer = Some(self.entries[i].edits.active().len() - 1);
                    self.selection = None;
                    self.gradient = None;
                    self.note("Adjustment layer added; the Color Studio now edits this layer.");
                    return self.after_edit(i);
                }
            }
            Message::Method(m) => {
                self.method = m;
                self.strength = match m {
                    Method::Blur => 12,
                    Method::Pixelate | Method::Mosaic => 16,
                    _ => self.strength,
                };
            }
            Message::Strength(s) => self.strength = s,
            Message::BrushRadius(r) => self.brush_radius = r.clamp(1., 400.),
            Message::Straighten(a) => self.straighten = a,
            Message::Lens(k) => self.lens = k,
            Message::ConcealSelection => {
                if let Some(sel) = self.selection.take() {
                    self.points.clear();
                    return self.update(Message::Apply(Operation::Conceal {
                        selection: sel,
                        method: self.method.concealment(self.strength),
                        invert: false,
                    }));
                }
                self.note("Make a selection first (Rect, Ellipse, Polygon, Lasso or Brush tool).");
            }
            Message::ImportLut => {
                return Task::perform(
                    async {
                        let f = rfd::AsyncFileDialog::new()
                            .add_filter("3D LUT", &["cube"])
                            .pick_file()
                            .await
                            .ok_or_else(|| "cancelled".to_string())?;
                        let bytes = f.read().await;
                        if bytes.len() > photo_edit::color::MAX_CUBE_BYTES {
                            return Err("LUT file too large".into());
                        }
                        Lut3d::parse_cube(&String::from_utf8_lossy(&bytes))
                            .map_err(|e| format!("{e:#}"))
                    },
                    Message::LutLoaded,
                );
            }
            Message::LutLoaded(r) => match r {
                Ok(l) => {
                    self.editing = false;
                    self.note(format!("Loaded {0}x{0}x{0} LUT", l.size));
                    return self.modify_layer(move |a, _, _| a.lut = Some(l));
                }
                Err(e) if e == "cancelled" => {}
                Err(e) => {
                    photo_log::warn(
                        Component::Color,
                        Code::SecurityEvent,
                        &format!("LUT rejected: {e}"),
                    );
                    self.note(format!("LUT rejected: {e}"));
                }
            },
            Message::ExportLut => {
                let a = self.layer_adjustments();
                let dir = self.output_dir.trim().to_string();
                return Task::perform(
                    async move {
                        let lut = a.bake_lut(33).map_err(|e| format!("{e:#}"))?;
                        let d = if dir.is_empty() {
                            rfd::AsyncFileDialog::new()
                                .pick_folder()
                                .await
                                .map(|f| f.path().to_path_buf())
                                .ok_or("cancelled")?
                        } else {
                            PathBuf::from(dir)
                        };
                        photo_core::save_unique(
                            &d,
                            "adjustments",
                            "cube",
                            lut.to_cube("AI Photo Defense").as_bytes(),
                        )
                        .map_err(|e| format!("{e:#}"))
                    },
                    Message::ReportSaved,
                );
            }
            Message::Profile(p) => self.profile = p,
            Message::Scan => {
                self.menu = None;
                let targets: Vec<usize> = if self.mode == Mode::Essential {
                    (0..self.entries.len())
                        .filter(|k| self.entries[*k].scan.is_none())
                        .collect()
                } else {
                    self.selected.into_iter().collect()
                };
                if targets.is_empty() {
                    return Task::none();
                }
                let profile = profile_by_name(&self.profile);
                self.busy = Some("Scanning".into());
                for &k in &targets {
                    self.entries[k].status = "Scanning".into();
                }
                let jobs: Vec<(usize, PathBuf, EditStack, u64)> = targets
                    .iter()
                    .map(|&k| {
                        (
                            k,
                            self.entries[k].path.clone(),
                            self.entries[k].edits.clone(),
                            self.entries[k].generation,
                        )
                    })
                    .collect();
                return Task::batch(jobs.into_iter().map(|(k, path, edits, generation)| {
                    let profile = profile.clone();
                    let face_model = self.face_model_path.clone();
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(
                                move || -> Result<(ScanReport, (u32, u32)), String> {
                                    let t = Instant::now();
                                    let p =
                                        photo_core::load(&path).map_err(|e| format!("{e:#}"))?;
                                    let pixels =
                                        edits.render(&p.pixels).map_err(|e| format!("{e:#}"))?;
                                    let dims = (pixels.width(), pixels.height());
                                    let photo = p.with_pixels(pixels);
                                    let r = scan_with_optional_faces(
                                        &photo,
                                        &profile,
                                        face_model.as_deref(),
                                    )?;
                                    photo_log::event(
                                        Level::Info,
                                        Component::Privacy,
                                        Code::ScanCompleted,
                                        &format!("{} findings", r.findings.len()),
                                    )
                                    .duration(t.elapsed())
                                    .emit();
                                    Ok((r, dims))
                                },
                            )
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        move |r| Message::Scanned(k, generation, r),
                    )
                }));
            }
            Message::Scanned(i, _generation, r) => {
                self.busy = None;
                match r {
                    Ok((report, dims)) => {
                        let n = report.findings.len();
                        if let Some(e) = self.entries.get_mut(i) {
                            e.status = if n == 0 {
                                "No findings".into()
                            } else {
                                format!("{n} finding(s)")
                            };
                            e.scan = Some(ScanState {
                                dismissed: vec![false; n],
                                report,
                                dims,
                            });
                        }
                        self.note(format!("Scan finished: {n} finding(s). Detectors without installed models are listed as not examined."));
                    }
                    Err(e) => {
                        photo_log::error(Component::Privacy, Code::ScanFailed, &e);
                        if let Some(en) = self.entries.get_mut(i) {
                            en.status = "Scan failed".into();
                        }
                        self.note(e);
                    }
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
                    self.selection = Some(Selection::Rect(Rect::new(r.x, r.y, r.width, r.height)));
                }
            }
            Message::Dismiss(k, d) => {
                if let Some(e) = self.current_mut()
                    && let Some(s) = e.scan.as_mut()
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
                if s.dims
                    != full_dims(
                        &self.entries[i].edits,
                        self.entries[i].width,
                        self.entries[i].height,
                    )
                {
                    self.note("Geometry changed since the scan. Scan again before concealing.");
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
                    None => self.note(
                        "No located findings to conceal. Metadata is always removed on export.",
                    ),
                }
            }
            Message::Format(f) => self.format = f,
            Message::Quality(q) => self.quality = q,
            Message::HighAssurance(h) => self.high_assurance = h,
            Message::AutoRedact(a) => self.auto_redact = a,
            Message::EssentialAspect(a) => self.essential_aspect = a,
            Message::Export {
                accept_unresolved,
                all,
            } => {
                self.menu = None;
                return self.start_export(accept_unresolved, all);
            }
            Message::Exported(results) => {
                self.busy = None;
                let (mut good, mut blocked, mut failed) = (0, 0, 0);
                for (i, r) in results {
                    match r {
                        Ok(s) => {
                            let Some(e) = self.entries.get_mut(i) else {
                                continue;
                            };
                            match &s {
                                ExportState::Done { ms, passed, .. } => {
                                    good += 1;
                                    e.status = if *passed {
                                        "Exported, verified".into()
                                    } else {
                                        "Exported, findings accepted".into()
                                    };
                                    self.perf
                                        .samples
                                        .push(("Export (full resolution)".into(), *ms));
                                }
                                ExportState::Blocked(_) => {
                                    blocked += 1;
                                    e.status = "Blocked: review".into();
                                }
                            }
                            e.export = Some(s);
                        }
                        Err(err) => {
                            failed += 1;
                            photo_log::error(Component::Export, Code::ExportFailed, &err);
                            if let Some(e) = self.entries.get_mut(i) {
                                e.status = "Failed".into();
                            }
                        }
                    }
                }
                self.note(format!(
                    "Export: {good} saved, {blocked} blocked by verification, {failed} failed{}.",
                    if self.cancel.load(Ordering::Relaxed) {
                        " (cancelled)"
                    } else {
                        ""
                    }
                ));
            }
            Message::Cancel => {
                self.cancel.store(true, Ordering::Relaxed);
                self.note("Cancelling after the current photograph.");
            }
            Message::SaveProject => {
                self.menu = None;
                let p = self.project();
                let existing = self.project_path.clone();
                let dir = self.output_dir.trim().to_string();
                return Task::perform(
                    async move {
                        let path = match existing {
                            Some(p) => p,
                            None => {
                                let mut d = rfd::AsyncFileDialog::new()
                                    .add_filter(
                                        "AI Photo Defense project",
                                        &[photo_project::EXTENSION],
                                    )
                                    .set_file_name("project.apdproj");
                                if !dir.is_empty() {
                                    d = d.set_directory(&dir);
                                }
                                d.save_file()
                                    .await
                                    .map(|f| f.path().to_path_buf())
                                    .ok_or("cancelled")?
                            }
                        };
                        photo_project::save(&path, &p, &photo_project::SaveOptions::default())
                            .map_err(|e| format!("{e:#}"))?;
                        Ok(path)
                    },
                    Message::ProjectSaved,
                );
            }
            Message::ProjectSaved(r) => match r {
                Ok(p) => {
                    self.note(format!("Project saved: {}", short(&p)));
                    self.project_path = Some(p);
                    self.dirty_project = false;
                }
                Err(e) if e == "cancelled" => {}
                Err(e) => self.note(e),
            },
            Message::OpenProject => {
                self.menu = None;
                return Task::perform(
                    async {
                        let f = rfd::AsyncFileDialog::new()
                            .add_filter("AI Photo Defense project", &[photo_project::EXTENSION])
                            .pick_file()
                            .await
                            .ok_or("cancelled")?;
                        photo_project::open(f.path())
                            .map(|l| l.project)
                            .map_err(|e| format!("{e:#}"))
                    },
                    Message::ProjectOpened,
                );
            }
            Message::ProjectOpened(r) => match r {
                Ok(p) => return self.restore(p),
                Err(e) if e == "cancelled" => {}
                Err(e) => self.note(e),
            },
            Message::RestoreAutosave => {
                if let Some(p) = self.autosave_offer.take() {
                    photo_log::event(
                        Level::Info,
                        Component::Recovery,
                        Code::ProjectRecovered,
                        "autosave restored",
                    )
                    .recovered(true)
                    .emit();
                    return self.restore(p);
                }
            }
            Message::DiscardAutosave => {
                self.autosave_offer = None;
                let _ = std::fs::remove_file(photo_project::autosave_path(
                    &self.data_dir.join("session.apdproj"),
                ));
                self.note("Autosave discarded.");
            }
            Message::LibSearch(s) => {
                self.lib_search = s;
                self.refresh_library();
            }
            Message::LibRating(r) => {
                self.lib_rating = r;
                self.refresh_library();
            }
            Message::TagInput(t) => self.tag_input = t,
            Message::ApplyTag => {
                if let (Some(c), Some(id)) =
                    (&self.catalog, self.current().and_then(|e| e.catalog_id))
                {
                    match c.tag(id, &self.tag_input) {
                        Ok(()) => self.tag_input.clear(),
                        Err(e) => self.status = format!("{e:#}"),
                    }
                }
                self.refresh_library();
            }
            Message::Page(p) => {
                self.page = p;
                if p == TechPage::Performance {
                    self.refresh_perf();
                }
                if p == TechPage::Forensics && self.forensics.is_none() {
                    return self.update(Message::Forensics);
                }
            }
            Message::LogLevel(l) => self.log_level = l,
            Message::LogComponent(c) => self.log_component = c,
            Message::LogSearch(s) => self.log_search = s,
            Message::CodeLookup(s) => self.code_lookup = s,
            Message::CopyReport => {
                self.note("Sanitized diagnostic report copied to the clipboard.");
                return iced::clipboard::write(self.diagnostic_report());
            }
            Message::PreviewReport => {
                self.report_preview = if self.report_preview.is_some() {
                    None
                } else {
                    Some(self.diagnostic_report())
                }
            }
            Message::SaveReport => {
                let text = self.diagnostic_report();
                let dir = self.output_dir.trim().to_string();
                return Task::perform(
                    async move {
                        let d = if dir.is_empty() {
                            rfd::AsyncFileDialog::new()
                                .pick_folder()
                                .await
                                .map(|f| f.path().to_path_buf())
                                .ok_or("cancelled")?
                        } else {
                            PathBuf::from(dir)
                        };
                        photo_core::save_unique(&d, "diagnostic-report", "txt", text.as_bytes())
                            .map_err(|e| format!("{e:#}"))
                    },
                    Message::ReportSaved,
                );
            }
            Message::ReportSaved(r) => match r {
                Ok(p) => self.note(format!("Saved {}", short(&p))),
                Err(e) if e == "cancelled" => {}
                Err(e) => self.note(e),
            },
            Message::ClearLogs => {
                let n = photo_log::clear();
                self.note(format!(
                    "Deleted {n} log file(s) and cleared the in-memory log."
                ));
            }
            Message::Retention(d) => {
                let mut c = photo_log::config();
                c.retention_days = d.clamp(1, 365);
                photo_log::configure(c);
            }
            Message::Verbosity(l) => {
                let mut c = photo_log::config();
                c.min_level = l;
                photo_log::configure(c);
            }
            Message::Lab => {
                let Some(e) = self.current() else {
                    return Task::none();
                };
                let Some(ExportState::Done { path, .. }) = &e.export else {
                    self.note(
                        "Export the selected photograph without geometry edits, then run the lab.",
                    );
                    return Task::none();
                };
                let (a, b) = (e.path.clone(), path.clone());
                self.busy = Some("Measuring".into());
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let a = photo_core::load(&a).map_err(|e| e.to_string())?;
                            let b = photo_core::load(&b).map_err(|e| e.to_string())?;
                            photo_lab::evaluate(&a.pixels, &b.pixels).map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::LabDone,
                );
            }
            Message::LabDone(r) => {
                self.busy = None;
                match r {
                    Ok(r) => self.lab = Some(r),
                    Err(e) => self.note(e),
                }
            }
            Message::RunValidation(mode) => {
                if mode == photo_lab::validation::ValidationMode::BatchValidation {
                    if self.entries.is_empty() {
                        self.note("Import photographs first for Batch Validation.");
                        return Task::none();
                    }
                    let items: Vec<(String, PathBuf, PathBuf)> = self
                        .entries
                        .iter()
                        .map(|e| {
                            let cand = match &e.export {
                                Some(ExportState::Done { path, .. }) => path.clone(),
                                _ => e.path.clone(),
                            };
                            (e.name.clone(), e.path.clone(), cand)
                        })
                        .collect();
                    let face_model = self.face_model_path.clone();
                    let clip_model = self.clip_model_path.clone();
                    let clip_manifest = self.clip_manifest_path.clone();
                    self.busy = Some("Running Batch Validation".into());
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                items
                                    .into_iter()
                                    .map(|(name, orig, cand)| {
                                        let res = (|| -> anyhow::Result<String> {
                                            let a = photo_core::load(&orig)?;
                                            let b = photo_core::load(&cand)?;
                                            let cfg = photo_lab::validation::ValidationConfig {
                                                mode: Some(photo_lab::validation::ValidationMode::QuickProtectionTest),
                                                face_model_path: face_model.clone(),
                                                clip_model_path: clip_model.clone(),
                                                clip_manifest_path: clip_manifest.clone(),
                                                original_bytes: Some(a.bytes.len() as u64),
                                                candidate_bytes: Some(b.bytes.len() as u64),
                                            };
                                            let rep = photo_lab::validation::run_validation(&a.pixels, &b.pixels, &cfg)?;
                                            let summary: Vec<String> = rep
                                                .tests
                                                .iter()
                                                .map(|t| format!("{}: {}", t.category.label(), t.classification.label()))
                                                .collect();
                                            Ok(format!(
                                                "{name} ({}x{}, SSIM {:.4}) — {}",
                                                rep.quality.resolution[0],
                                                rep.quality.resolution[1],
                                                rep.quality.base.ssim_luma_8x8,
                                                summary.join(" | ")
                                            ))
                                        })();
                                        match res {
                                            Ok(line) => line,
                                            Err(e) => format!("{name} — Error: {e}"),
                                        }
                                    })
                                    .collect()
                            })
                            .await
                            .unwrap_or_default()
                        },
                        Message::BatchValidationDone,
                    );
                }
                let Some(e) = self.current() else {
                    self.note("Select a photograph first.");
                    return Task::none();
                };
                let (orig_path, cand_path) = match &e.export {
                    Some(ExportState::Done { path, .. }) => (e.path.clone(), path.clone()),
                    _ => (e.path.clone(), e.path.clone()),
                };
                let face_model = self.face_model_path.clone();
                let clip_model = self.clip_model_path.clone();
                let clip_manifest = self.clip_manifest_path.clone();
                self.busy = Some(format!("Running {}", mode.label()));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let a = photo_core::load(&orig_path).map_err(|e| e.to_string())?;
                            let b = photo_core::load(&cand_path).map_err(|e| e.to_string())?;
                            let cfg = photo_lab::validation::ValidationConfig {
                                mode: Some(mode),
                                face_model_path: face_model,
                                clip_model_path: clip_model,
                                clip_manifest_path: clip_manifest,
                                original_bytes: Some(a.bytes.len() as u64),
                                candidate_bytes: Some(b.bytes.len() as u64),
                            };
                            photo_lab::validation::run_validation(&a.pixels, &b.pixels, &cfg)
                                .map(Box::new)
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::ValidationDone,
                );
            }
            Message::BatchValidationDone(rows) => {
                self.busy = None;
                self.note(format!(
                    "Batch validation finished ({} item(s))",
                    rows.len()
                ));
                self.batch_validation = rows;
            }
            Message::ShowWizard(show) => {
                self.show_wizard = show;
                self.menu = None;
                if show {
                    self.wizard_step = 0;
                }
            }
            Message::WizardStep(step) => {
                self.wizard_step = step.min(3);
            }
            Message::TutorialStep(step) => {
                self.tutorial_step = step.min(4);
            }
            Message::CheckForUpdates => {
                self.busy = Some("Checking GitHub releases...".into());
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(check_github_release)
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::UpdatesChecked,
                );
            }
            Message::UpdatesChecked(res) => {
                self.busy = None;
                match &res {
                    Ok(info) => {
                        if info.update_available {
                            self.note(format!(
                                "Update available: {} (current: {})",
                                info.latest_version, info.current_version
                            ));
                        } else {
                            self.note(format!("Up to date ({})", info.current_version));
                        }
                    }
                    Err(e) => self.note(format!("Update check: {e}")),
                }
                self.update_status = Some(res);
            }
            Message::OpenReleasesPage => {
                open_releases_in_browser();
            }
            Message::DownloadLatestInstaller => {
                let Some(Ok(info)) = &self.update_status else {
                    self.note("Check for updates first.");
                    return Task::none();
                };
                let (Some(name), Some(url)) = (info.asset_name.clone(), info.asset_url.clone())
                else {
                    open_releases_in_browser();
                    return Task::none();
                };
                self.busy = Some(format!("Downloading {name}..."));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || download_release_asset(&name, &url))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::InstallerDownloaded,
                );
            }
            Message::InstallerDownloaded(res) => {
                self.busy = None;
                match &res {
                    Ok(msg) => {
                        self.note(msg.clone());
                        self.download_status = Some(msg.clone());
                    }
                    Err(e) => {
                        self.note(format!("Download failed: {e}"));
                        self.download_status = Some(format!("Download error: {e}"));
                    }
                }
            }
            Message::LoadCalibrationSample => {
                let candidates = [
                    PathBuf::from("tests/fixtures/calibration.png"),
                    PathBuf::from("../../tests/fixtures/calibration.png"),
                ];
                if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
                    self.show_wizard = false;
                    return self.update(Message::Picked(vec![p]));
                } else {
                    self.note("Sample fixture not found in working directory; use Import Photos.");
                    return self.update(Message::OpenDialog);
                }
            }
            Message::VideoSetFromNow => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_from = format!("{:.2}", m.video_time_s);
                }
            }
            Message::VideoSetToNow => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_to = format!("{:.2}", m.video_time_s);
                }
            }
            Message::VideoStep(dt) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get(i)
                {
                    let next = (m.video_time_s + dt).clamp(0.0, m.video_duration_s.max(0.0));
                    return self.update(Message::VideoScrub(next));
                }
            }
            Message::VideoTrackAndRedact => {
                let Some(i) = self.selected_media else {
                    return Task::none();
                };
                let dir = PathBuf::from(self.output_dir.trim());
                if self.output_dir.trim().is_empty() || !dir.is_dir() {
                    self.note("Choose an existing output folder first.");
                    return self.update(Message::BrowseOutput);
                }
                let m = &mut self.media[i];
                let path = m.path.clone();
                let rect_str = m.video_rect.clone();
                let time_s = m.video_time_s;
                let blur = m.video_blur;
                m.status = "Tracking & redacting video...".into();
                m.video_track_warnings.clear();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            track_video_media(&path, &dir, &rect_str, time_s, blur)
                                .map_err(|e| format!("{e:#}"))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::VideoTrackDone(i, r),
                );
            }
            Message::VideoTrackDone(i, res) => {
                if let Some(m) = self.media.get_mut(i) {
                    match res {
                        Ok((out, msg, warnings)) => {
                            m.status = msg.clone();
                            m.output = Some(out);
                            m.video_track_warnings = warnings;
                            self.note(msg);
                        }
                        Err(e) => {
                            m.status = format!("Track failed: {e}");
                            self.note(e);
                        }
                    }
                }
            }
            Message::ValidationDone(r) => {
                self.busy = None;
                match r {
                    Ok(rep) => {
                        self.note(format!("Validation complete: {}", rep.mode.label()));
                        self.validation_report = Some(rep);
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::ExportValidationReport(markdown) => {
                let Some(rep) = &self.validation_report else {
                    return Task::none();
                };
                let content = if markdown {
                    rep.to_markdown().into_bytes()
                } else {
                    match rep.to_json_pretty() {
                        Ok(j) => j.into_bytes(),
                        Err(e) => {
                            self.note(e.to_string());
                            return Task::none();
                        }
                    }
                };
                let default_name = if markdown {
                    "validation-report.md"
                } else {
                    "validation-report.json"
                };
                return Task::perform(
                    async move {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_file_name(default_name)
                            .save_file()
                            .await
                        else {
                            return Err("Cancelled".into());
                        };
                        let path = file.path().to_path_buf();
                        photo_core::save_new(&path, &content).map_err(|e| e.to_string())?;
                        Ok(path)
                    },
                    Message::ValidationReportSaved,
                );
            }
            Message::ValidationReportSaved(r) => match r {
                Ok(p) => self.note(format!("Saved validation report to {}", p.display())),
                Err(e) if e != "Cancelled" => self.note(e),
                Err(_) => {}
            },
            Message::Forensics => {
                let Some(e) = self.current() else {
                    return Task::none();
                };
                let path = e.path.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let p = photo_core::load(&path).map_err(|e| format!("{e:#}"))?;
                            Ok(Box::new(photo_privacy::forensics::inspect(&p.bytes)))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::ForensicsDone,
                );
            }
            Message::ForensicsDone(r) => match r {
                Ok(f) => self.forensics = Some(f),
                Err(e) => self.note(e),
            },
            Message::Cache(on) => self.cache_enabled = on,
            Message::ClearCache => {
                match photo_catalog::ThumbCache::enabled(self.data_dir.join("thumbnails"))
                    .and_then(|c| c.clear())
                {
                    Ok(n) => self.note(format!("Overwrote and deleted {n} cached thumbnail(s).")),
                    Err(e) => self.note(format!("{e:#}")),
                }
            }
            Message::MediaLoaded(path, l) => {
                if let Some(m) = self.media.iter_mut().find(|m| m.path == path) {
                    m.bytes = l.bytes;
                    m.mismatch = l.mismatch;
                    m.status = match (&l.error, &l.info) {
                        (Some(e), _) => format!("Error: {e}"),
                        (None, Some(i)) if i.findings.is_empty() => "Inspected".into(),
                        (None, Some(i)) => format!("{} privacy finding(s)", i.findings.len()),
                        _ => "Inspected".into(),
                    };
                    if let Some(e) = &l.error {
                        photo_log::warn(Component::Decode, Code::DecodeFailed, e);
                    }
                    if l.error.is_none() {
                        photo_log::event(
                            Level::Info,
                            Component::Decode,
                            Code::Other,
                            "media inspected",
                        )
                        .emit();
                    }
                    m.info = l.info;
                    m.analysis = l.analysis;
                    m.wave = l.wave;
                    m.spectro = l.spectro;
                    m.video_preview = l.video_preview;
                    m.video_duration_s = l.video_duration_s;
                }
                if self.selected_media.is_none() && !self.media.is_empty() {
                    self.selected_media = Some(0);
                }
            }
            Message::SelectMedia(i) if i < self.media.len() => {
                self.selected_media = Some(i);
                self.focus_media = true;
            }
            Message::SegStart(v) => {
                self.seg_start = v
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.')
                    .take(12)
                    .collect()
            }
            Message::SegEnd(v) => {
                self.seg_end = v
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.')
                    .take(12)
                    .collect()
            }
            Message::AddSegment(kind) => {
                let (a, b) = (self.seg_start.parse::<f64>(), self.seg_end.parse::<f64>());
                match (a, b, self.selected_media) {
                    (Ok(a), Ok(b), Some(i)) if b > a && a >= 0. => {
                        self.media[i].segments.push(photo_media::audio::Segment {
                            start_s: a,
                            end_s: b,
                            kind,
                        });
                        self.media[i].output = None;
                        self.seg_start.clear();
                        self.seg_end.clear();
                    }
                    _ => self.note("Enter a start and end time in seconds (end after start)."),
                }
            }
            Message::RemoveSegment(k) => {
                if let Some(i) = self.selected_media
                    && k < self.media[i].segments.len()
                {
                    self.media[i].segments.remove(k);
                }
            }
            Message::MediaExport => {
                let Some(i) = self.selected_media else {
                    return Task::none();
                };
                let dir = PathBuf::from(self.output_dir.trim());
                if self.output_dir.trim().is_empty() || !dir.is_dir() {
                    self.note("Choose an existing output folder first.");
                    return self.update(Message::BrowseOutput);
                }
                let m = &self.media[i];
                let (path, segs, kind, container) = (
                    m.path.clone(),
                    m.segments.clone(),
                    m.kind(),
                    m.info
                        .as_ref()
                        .map(|i| i.container.clone())
                        .unwrap_or_default(),
                );
                self.media[i].status = "Exporting".into();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            export_media(&path, &dir, kind, &container, &segs)
                                .map_err(|e| format!("{e:#}"))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::MediaExported(i, r),
                );
            }
            Message::MediaExported(i, r) => {
                if let Some(m) = self.media.get_mut(i) {
                    match r {
                        Ok((p, report, cmp, lufs)) => {
                            m.status = "Exported, verified".into();
                            m.output = Some(p);
                            m.report = Some(report);
                            m.comparison = cmp;
                            m.out_lufs = lufs;
                        }
                        Err(e) => {
                            photo_log::error(Component::Export, Code::ExportFailed, &e);
                            m.status = format!("Export failed: {e}");
                        }
                    }
                }
            }
            Message::Play(i, processed) => {
                let Some(m) = self.media.get(i) else {
                    return Task::none();
                };
                let path = if processed {
                    m.output.clone()
                } else {
                    Some(m.path.clone())
                };
                let Some(path) = path else {
                    return Task::none();
                };
                // Loudness-matched comparison: both versions are played at -23 LUFS.
                let lufs = if processed {
                    m.out_lufs
                } else {
                    m.analysis.as_ref().and_then(|a| a.integrated_lufs)
                };
                let gain = lufs
                    .map(|l| 10f64.powf((-23. - l) / 20.).clamp(0.05, 4.) as f32)
                    .unwrap_or(1.);
                self.playing = Some((i, processed));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            preview_pcm(&path).map_err(|e| format!("{e:#}"))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::PlayReady(r, gain),
                );
            }
            Message::PlayReady(r, gain) => {
                match r {
                    Ok(pcm) => {
                        if self.player.is_none() {
                            match rodio::OutputStream::try_default() {
                                Ok((stream, handle)) => {
                                    self.player = Some(Player {
                                        _stream: stream,
                                        handle,
                                        sink: None,
                                    })
                                }
                                Err(e) => {
                                    self.note(format!("No audio output device: {e}"));
                                    return Task::none();
                                }
                            }
                        }
                        if let Some(p) = self.player.as_mut() {
                            if let Some(old) = p.sink.take() {
                                old.stop();
                            }
                            match rodio::Sink::try_new(&p.handle) {
                                Ok(sink) => {
                                    sink.set_volume(gain);
                                    sink.append(rodio::buffer::SamplesBuffer::new(
                                        pcm.1,
                                        pcm.2,
                                        (*pcm.0).clone(),
                                    ));
                                    p.sink = Some(sink);
                                    self.note(format!("Playing at {:+.1} dB loudness-matched gain (first 2 minutes).", 20. * gain.log10()));
                                }
                                Err(e) => self.note(format!("Playback failed: {e}")),
                            }
                        }
                    }
                    Err(e) => self.note(e),
                }
            }
            Message::StopAudio => {
                if let Some(p) = self.player.as_mut()
                    && let Some(s) = p.sink.take()
                {
                    s.stop();
                }
                self.playing = None;
            }
            Message::VideoScrub(t) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_time_s = t.clamp(0.0, m.video_duration_s.max(0.0));
                    let path = m.path.clone();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || decode_video_preview(&path, t))
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        move |r| Message::VideoFrameReady(i, r),
                    );
                }
            }
            Message::VideoFrameReady(i, r) => {
                if let Some(m) = self.media.get_mut(i) {
                    match r {
                        Ok((pts, w, h, rgba)) => {
                            m.video_time_s = pts;
                            m.video_preview = Some(image::Handle::from_rgba(w, h, rgba));
                        }
                        Err(e) => self.note(format!("Video preview: {e}")),
                    }
                }
            }
            Message::VideoTogglePlay => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_playing = !m.video_playing;
                    if m.video_playing && m.video_time_s >= m.video_duration_s {
                        m.video_time_s = 0.0;
                    }
                }
            }
            Message::VideoTick => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                    && m.video_playing
                {
                    let next = m.video_time_s + 1.0 / 30.0;
                    if next >= m.video_duration_s {
                        m.video_playing = false;
                        m.video_time_s = m.video_duration_s;
                    } else {
                        m.video_time_s = next;
                        let path = m.path.clone();
                        return Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    decode_video_preview(&path, next)
                                })
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                            },
                            move |r| Message::VideoFrameReady(i, r),
                        );
                    }
                }
            }
            Message::VideoRect(s) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_rect = s;
                }
            }
            Message::VideoFrom(s) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_from = s;
                }
            }
            Message::VideoTo(s) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_to = s;
                }
            }
            Message::VideoBlur(b) => {
                if let Some(i) = self.selected_media
                    && let Some(m) = self.media.get_mut(i)
                {
                    m.video_blur = b;
                }
            }
            Message::VideoRedact => {
                let Some(i) = self.selected_media else {
                    return Task::none();
                };
                let dir = PathBuf::from(self.output_dir.trim());
                if self.output_dir.trim().is_empty() || !dir.is_dir() {
                    self.note("Choose an existing output folder first.");
                    return self.update(Message::BrowseOutput);
                }
                let m = &mut self.media[i];
                let path = m.path.clone();
                let rect_str = m.video_rect.clone();
                let from_str = m.video_from.clone();
                let to_str = m.video_to.clone();
                let blur = m.video_blur;
                m.status = "Redacting video".into();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            redact_video_media(&path, &dir, &rect_str, &from_str, &to_str, blur)
                                .map_err(|e| format!("{e:#}"))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::MediaExported(i, r),
                );
            }
            Message::ChooseVoiceModelFolder => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::VoiceModelFolderChosen,
                );
            }
            Message::VoiceModelFolderChosen(Some(dir)) => {
                self.voice_model_dir = Some(dir);
                self.dirty_project = true;
            }
            Message::VoiceModelFolderChosen(None) => {}
            Message::ProtectVoice => {
                let Some(i) = self.selected_media else {
                    return Task::none();
                };
                let Some(model_dir) = self.voice_model_dir.clone() else {
                    self.note("Choose a voice model folder in Technical > Model Library first.");
                    return Task::none();
                };
                let dir = PathBuf::from(self.output_dir.trim());
                if self.output_dir.trim().is_empty() || !dir.is_dir() {
                    self.note("Choose an existing output folder first.");
                    return self.update(Message::BrowseOutput);
                }
                let path = self.media[i].path.clone();
                self.media[i].status = "Protecting voice".into();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            protect_voice_media(&path, &dir, &model_dir)
                                .map_err(|e| format!("{e:#}"))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::MediaExported(i, r),
                );
            }
            Message::ChooseFaceModel => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("ONNX model", &["onnx"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::FaceModelChosen,
                );
            }
            Message::FaceModelChosen(Some(p)) => {
                self.face_model_path = Some(p);
                self.dirty_project = true;
            }
            Message::FaceModelChosen(None) => {}
            Message::ChooseClipModel => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("SafeTensors model", &["safetensors"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::ClipModelChosen,
                );
            }
            Message::ClipModelChosen(Some(p)) => {
                self.clip_model_path = Some(p);
            }
            Message::ClipModelChosen(None) => {}
            Message::ChooseClipManifest => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Manifest JSON", &["json"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::ClipManifestChosen,
                );
            }
            Message::ClipManifestChosen(Some(p)) => {
                self.clip_manifest_path = Some(p);
            }
            Message::ClipManifestChosen(None) => {}
            Message::SelectFace(r) => {
                self.tool = Tool::Rect;
                self.selection = Some(Selection::Rect(photo_edit::Rect {
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                }));
            }
            Message::SelectAllFaces => {
                let Some(i) = self.selected else {
                    return Task::none();
                };
                let faces: Vec<photo_privacy::Finding> = {
                    let Some(s) = self.entries[i].scan.as_mut() else {
                        return Task::none();
                    };
                    for (f, d) in s.report.findings.iter().zip(&mut s.dismissed) {
                        if f.category == photo_privacy::Category::Face && f.region.is_some() {
                            *d = false;
                        }
                    }
                    s.report
                        .findings
                        .iter()
                        .filter(|f| {
                            f.category == photo_privacy::Category::Face && f.region.is_some()
                        })
                        .cloned()
                        .collect()
                };
                if faces.len() == 1
                    && let Some(r) = faces[0].region
                {
                    self.tool = Tool::Rect;
                    self.selection = Some(Selection::Rect(photo_edit::Rect {
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                    }));
                } else if !faces.is_empty() {
                    let pref = match self.method {
                        Method::White => photo_privacy::profiles::RedactionPreference::OpaqueWhite,
                        Method::Blur => photo_privacy::profiles::RedactionPreference::Blur,
                        Method::Pixelate | Method::Mosaic => {
                            photo_privacy::profiles::RedactionPreference::Pixelate
                        }
                        Method::Black => photo_privacy::profiles::RedactionPreference::OpaqueBlack,
                    };
                    if let Some(op) = photo_pipeline::conceal_findings(&faces, pref, 4) {
                        self.note(format!("Applied redaction to {} face(s).", faces.len()));
                        return self.update(Message::Apply(op));
                    }
                }
            }
            Message::ChooseC2paCert => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("PEM certificate", &["pem", "crt", "cer"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::C2paCertChosen,
                );
            }
            Message::C2paCertChosen(Some(p)) => {
                self.c2pa_cert_path = Some(p);
                self.dirty_project = true;
            }
            Message::C2paCertChosen(None) => {}
            Message::ChooseC2paKey => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("PEM private key", &["pem", "key"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::C2paKeyChosen,
                );
            }
            Message::C2paKeyChosen(Some(p)) => {
                self.c2pa_key_path = Some(p);
                self.dirty_project = true;
            }
            Message::C2paKeyChosen(None) => {}
            Message::PaneResized(e) => self.panes.resize(e.split, e.ratio),
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target)
            }
            Message::PaneDragged(_) => {}
            Message::PaneMaximize(kind) => {
                self.menu = None;
                if self.panes.maximized().is_some() {
                    self.panes.restore();
                } else {
                    let found = self
                        .panes
                        .iter()
                        .find(|(_, k)| **k == kind)
                        .map(|(p, _)| *p);
                    if let Some(p) = found {
                        self.panes.maximize(p);
                    }
                }
            }
            Message::ResetLayout => {
                self.panes = default_panes();
                self.menu = None;
            }
            Message::Event(Event::Window(window::Event::FileDropped(p))) => {
                return self.update(Message::Picked(vec![p]));
            }
            Message::Event(Event::Window(window::Event::CloseRequested)) => {
                let _ = self.update(Message::Autosave);
                photo_log::info(Component::App, Code::Shutdown, "shutdown");
                return window::get_latest().and_then(window::close);
            }
            Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
                key, modifiers, ..
            })) => return self.shortcut(key, modifiers),
            _ => {}
        }
        Task::none()
    }

    fn restore(&mut self, p: photo_project::Project) -> Task<Message> {
        let paths: Vec<PathBuf> = p
            .entries
            .iter()
            .map(|e| e.path.clone())
            .filter(|p| p.is_file())
            .collect();
        let missing = p.entries.len() - paths.len();
        self.entries.clear();
        self.selected = None;
        self.profile = p.settings.profile.clone();
        self.quality = p.settings.quality;
        self.c2pa_cert_path = p.settings.c2pa_cert_path.as_ref().map(PathBuf::from);
        self.c2pa_key_path = p.settings.c2pa_key_path.as_ref().map(PathBuf::from);
        self.voice_model_dir = p.settings.voice_model_dir.as_ref().map(PathBuf::from);
        self.face_model_path = p.settings.face_model_path.as_ref().map(PathBuf::from);
        self.pending_restore = Some(p);
        if missing > 0 {
            photo_log::warn(
                Component::Project,
                Code::FileAccess,
                &format!("{missing} referenced photographs missing"),
            );
        }
        let t = self.update(Message::Picked(paths));
        if missing > 0 {
            self.note(format!(
                "{missing} photograph(s) referenced by the project are missing."
            ));
        }
        t
    }

    fn start_export(&mut self, accept: bool, all: bool) -> Task<Message> {
        if self.busy.is_some() || self.entries.is_empty() {
            return Task::none();
        }
        let dir = PathBuf::from(self.output_dir.trim());
        if self.output_dir.trim().is_empty() || !dir.is_dir() {
            self.note("Choose an existing output folder first.");
            return self.update(Message::BrowseOutput);
        }
        let targets: Vec<usize> = if all || self.mode == Mode::Essential {
            (0..self.entries.len()).collect()
        } else {
            self.selected.into_iter().collect()
        };
        let (f, q) = (self.format, self.quality);
        let profile = profile_by_name(&self.profile);
        let (high, auto) = (self.high_assurance, self.auto_redact);
        let c2pa_creds = self.c2pa_cert_path.clone().zip(self.c2pa_key_path.clone());
        let aspect = if self.mode == Mode::Essential {
            self.essential_aspect
        } else {
            None
        };
        let jobs: Vec<(usize, PathBuf, EditStack, Option<ScanState>)> = targets
            .iter()
            .map(|&i| {
                (
                    i,
                    self.entries[i].path.clone(),
                    self.entries[i].edits.clone(),
                    self.entries[i].scan.clone(),
                )
            })
            .collect();
        for &i in &targets {
            self.entries[i].status = "Queued".into();
        }
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        self.busy = Some(format!("Exporting {} photograph(s)", jobs.len()));
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let mut out = vec![];
                    for (i, path, mut edits, scan) in jobs {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let job = photo_log::new_job();
                        let t = Instant::now();
                        let r = (|| -> anyhow::Result<ExportState> {
                            let p = photo_core::load(&path)?;
                            if let Some(a) = aspect {
                                edits.push(Operation::CropAspect(a))?;
                            }
                            if auto {
                                let staged = p.with_pixels(edits.render(&p.pixels)?);
                                let found = photo_pipeline::scan_photo(&staged, &profile).findings;
                                if let Some(op) =
                                    photo_pipeline::conceal_findings(&found, profile.redaction, 4)
                                {
                                    edits.push(op)?;
                                }
                            }
                            let risks = if high {
                                photo_pipeline::pre_export_review(
                                    &edits,
                                    scan.as_ref().map(|s| (&s.report, s.dismissed.as_slice())),
                                    auto,
                                )
                            } else {
                                vec![]
                            };
                            let prepared = photo_pipeline::prepare(&p, &edits, f, q, &profile)?;
                            let v = &prepared.report.verification;
                            if (!v.passed || !risks.is_empty()) && !accept {
                                let mut w = risks;
                                w.extend(v.warnings.clone());
                                w.extend(
                                    v.metadata_findings
                                        .iter()
                                        .chain(&v.remaining_findings)
                                        .map(|f| format!("{:?}: {}", f.category, f.description)),
                                );
                                photo_log::event(
                                    Level::Warning,
                                    Component::Export,
                                    Code::ExportBlocked,
                                    &format!("{} unresolved items", w.len()),
                                )
                                .job(job)
                                .emit();
                                return Ok(ExportState::Blocked(w));
                            }
                            let saved = photo_pipeline::publish(&prepared, &path, &dir, f, accept)?;
                            if let Some((cert_p, key_p)) = &c2pa_creds {
                                sign_exported_c2pa(&saved.image, &edits, cert_p, key_p)?;
                            }
                            photo_log::event(
                                Level::Info,
                                Component::Export,
                                Code::ExportVerified,
                                if saved.verification_passed {
                                    "verified"
                                } else {
                                    "accepted with findings"
                                },
                            )
                            .job(job)
                            .duration(t.elapsed())
                            .emit();
                            Ok(ExportState::Done {
                                path: saved.image,
                                passed: saved.verification_passed,
                                report: Box::new(prepared.report),
                                ms: t.elapsed().as_secs_f64() * 1000.,
                            })
                        })();
                        out.push((i, r.map_err(|e| format!("{}: {e:#}", short(&path)))));
                    }
                    out
                })
                .await
                .unwrap_or_else(|e| vec![(0, Err(e.to_string()))])
            },
            Message::Exported,
        )
    }

    fn shortcut(&mut self, key: keyboard::Key, m: keyboard::Modifiers) -> Task<Message> {
        use keyboard::key::Named;
        if m.command() {
            return match key.as_ref() {
                keyboard::Key::Character("o") if m.shift() => {
                    self.update(Message::OpenFolderDialog)
                }
                keyboard::Key::Character("o") => self.update(Message::OpenDialog),
                keyboard::Key::Character("s") => self.update(Message::SaveProject),
                keyboard::Key::Character("e") => self.update(Message::Export {
                    accept_unresolved: false,
                    all: m.shift(),
                }),
                keyboard::Key::Character("z") if m.shift() => self.update(Message::Redo),
                keyboard::Key::Character("z") => self.update(Message::Undo),
                keyboard::Key::Character("y") => self.update(Message::Redo),
                keyboard::Key::Character("p") => self.update(Message::Scan),
                keyboard::Key::Character("1") => self.update(Message::Mode(Mode::Essential)),
                keyboard::Key::Character("2") => self.update(Message::Mode(Mode::Professional)),
                keyboard::Key::Character("3") => self.update(Message::Mode(Mode::Technical)),
                _ => Task::none(),
            };
        }
        match key.as_ref() {
            keyboard::Key::Named(Named::Escape) => return self.update(Message::Escape),
            keyboard::Key::Named(Named::Enter) => return self.update(Message::Enter),
            keyboard::Key::Named(Named::F11) => {
                return self.update(Message::Fullscreen(!self.fullscreen));
            }
            keyboard::Key::Named(Named::Tab) => {
                return if m.shift() {
                    iced::widget::focus_previous()
                } else {
                    iced::widget::focus_next()
                };
            }
            keyboard::Key::Named(Named::ArrowRight) if self.mode != Mode::Technical => {
                if let Some(i) = self.selected
                    && i + 1 < self.entries.len()
                {
                    return self.update(Message::Select(i + 1));
                }
            }
            keyboard::Key::Named(Named::ArrowLeft) if self.mode != Mode::Technical => {
                if let Some(i) = self.selected
                    && i > 0
                {
                    return self.update(Message::Select(i - 1));
                }
            }
            keyboard::Key::Character(c) if self.mode == Mode::Professional && !m.alt() => {
                let c = c.to_ascii_lowercase();
                if let Some(t) = Tool::ALL.iter().find(|t| t.key().eq_ignore_ascii_case(&c)) {
                    return self.update(Message::Tool(*t));
                }
                match c.as_str() {
                    "\\" => {
                        return self.update(Message::View(if self.view == ViewMode::Before {
                            ViewMode::Edited
                        } else {
                            ViewMode::Before
                        }));
                    }
                    "y" => {
                        return self.update(Message::View(if self.view == ViewMode::Split {
                            ViewMode::Edited
                        } else {
                            ViewMode::Split
                        }));
                    }
                    "j" => return self.update(Message::Clipping),
                    "o" => return self.update(Message::Thirds),
                    "f" => return self.update(Message::Fullscreen(!self.fullscreen)),
                    d if d.len() == 1
                        && d.as_bytes()[0].is_ascii_digit()
                        && d.as_bytes()[0] <= b'5' =>
                    {
                        if let Some(i) = self.selected {
                            return self.update(Message::Rating(i, d.parse().unwrap_or(0)));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn refresh_perf(&mut self) {
        if self.mode != Mode::Technical {
            return;
        }
        let Some(sys) = self.sys.as_mut() else { return };
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        if let Ok(pid) = sysinfo::get_current_pid() {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            self.perf.process_mem = sys.process(pid).map(|p| p.memory());
        }
        let cpu = sys.global_cpu_usage();
        self.perf.cpu = (cpu > 0.).then_some(cpu);
        self.perf.mem_used = Some(sys.used_memory());
        self.perf.mem_total = Some(sys.total_memory());
        if self.page == TechPage::Performance {
            let out = PathBuf::from(self.output_dir.trim());
            let target = if out.is_dir() {
                std::fs::canonicalize(out).unwrap_or_default()
            } else {
                std::fs::canonicalize(&self.data_dir).unwrap_or_default()
            };
            // Windows canonical paths carry the extended-length prefix; mount points do not.
            let target = PathBuf::from(
                target
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .to_string(),
            );
            let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
                return;
            };
            self.perf.disk_free = disks
                .iter()
                .filter(|d| target.starts_with(d.mount_point()))
                .max_by_key(|d| d.mount_point().as_os_str().len())
                .map(|d| d.available_space());
        }
    }

    pub fn diagnostic_report(&self) -> String {
        let p = &self.perf;
        photo_log::report(&[
            ("mode", self.mode.to_string()),
            ("photos in collection", self.entries.len().to_string()),
            ("cpu cores", p.cores.to_string()),
            (
                "system memory MiB",
                p.mem_total
                    .map(|m| (m / 1_048_576).to_string())
                    .unwrap_or("unknown".into()),
            ),
            ("gpu", "not used (CPU renderer)".into()),
            ("models installed", "none".into()),
            (
                "unclean previous shutdown",
                self.unclean_shutdown.to_string(),
            ),
            ("dependencies", DEPENDENCIES.into()),
        ])
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            iced::event::listen_with(|e, status, _| match e {
                Event::Keyboard(keyboard::Event::KeyPressed { .. })
                    if status == iced::event::Status::Captured =>
                {
                    None
                }
                Event::Keyboard(_)
                | Event::Window(window::Event::FileDropped(_))
                | Event::Window(window::Event::CloseRequested) => Some(Message::Event(e)),
                _ => None,
            }),
            iced::time::every(Duration::from_secs(60)).map(|_| Message::Autosave),
        ];
        if self.mode == Mode::Technical {
            subs.push(iced::time::every(Duration::from_secs(2)).map(|_| Message::Tick));
        }
        if let Some(i) = self.selected_media
            && self.media.get(i).is_some_and(|m| m.video_playing)
        {
            subs.push(iced::time::every(Duration::from_millis(33)).map(|_| Message::VideoTick));
        }
        Subscription::batch(subs)
    }
}

fn load_app_icon() -> Option<window::Icon> {
    let bytes = include_bytes!("../../../assets/icons/icon-64.png");
    let (img, _) = photo_core::decode_bounded(bytes).ok()?;
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    window::icon::from_rgba(rgba.into_raw(), w, h).ok()
}

pub fn run() -> iced::Result {
    let mut args = std::env::args_os().skip(1);
    let mut paths = vec![];
    let mut mode = None;
    let mut output = String::new();
    let mut safe = false;
    let mut appearance = theme::Appearance::Graphite;
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--mode") => mode = args.next().and_then(|m| m.into_string().ok()),
            Some("--output") => {
                output = args
                    .next()
                    .and_then(|m| m.into_string().ok())
                    .unwrap_or_default()
            }
            Some("--safe-mode") => safe = true,
            Some("--light") => appearance = theme::Appearance::Paper,
            _ => paths.push(PathBuf::from(a)),
        }
    }
    iced::application("AI Photo Defense", App::update, views::view)
        .subscription(App::subscription)
        .theme(|a: &App| theme::theme(a.appearance))
        .scale_factor(|a: &App| a.ui_scale)
        .default_font(iced::Font::with_name(if cfg!(target_os = "windows") {
            "Segoe UI"
        } else if cfg!(target_os = "macos") {
            "Helvetica Neue"
        } else {
            "DejaVu Sans"
        }))
        .window(window::Settings {
            size: iced::Size::new(1280., 760.),
            min_size: Some(iced::Size::new(900., 560.)),
            icon: load_app_icon(),
            exit_on_close_request: false,
            ..Default::default()
        })
        .run_with(move || {
            let mut app = App::new(safe);
            app.appearance = appearance;
            app.output_dir = output;
            app.mode = match mode.as_deref() {
                Some("home") => Mode::Home,
                Some("essential") => Mode::Essential,
                Some("technical") => Mode::Technical,
                Some("tutorial") => Mode::Tutorial,
                _ => Mode::Professional,
            };
            if !paths.is_empty() || mode.is_some() {
                app.show_wizard = false;
            }
            let import = app.update(Message::Picked(paths));
            (
                app,
                Task::batch([
                    import,
                    window::get_latest().and_then(|id| window::maximize(id, true)),
                    window::get_latest().and_then(window::gain_focus),
                ]),
            )
        })
}

fn check_github_release() -> Result<UpdateInfo, String> {
    let url = "https://api.github.com/repos/test3457890/Ai-Photo-Defense-App/releases/latest";
    let ua = format!("AI-Photo-Defense/{VERSION}");
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "8", "-A", &ua, url])
        .output()
        .map_err(|e| format!("Could not run system curl to check GitHub: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("404") {
            // No published release tag yet — query latest commit on main so user sees real GitHub status.
            let c_url =
                "https://api.github.com/repos/test3457890/Ai-Photo-Defense-App/commits/main";
            if let Ok(c_out) = std::process::Command::new("curl")
                .args(["-fsSL", "--max-time", "8", "-A", &ua, c_url])
                .output()
                && c_out.status.success()
                && let Ok(c_val) = serde_json::from_slice::<serde_json::Value>(&c_out.stdout)
            {
                let sha: String = c_val
                    .get("sha")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .chars()
                    .take(7)
                    .collect();
                let msg: String = c_val
                    .pointer("/commit/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect();
                return Ok(UpdateInfo {
                    current_version: VERSION.to_string(),
                    latest_version: format!("main@{sha}"),
                    update_available: false,
                    release_url: "https://github.com/test3457890/Ai-Photo-Defense-App/actions"
                        .to_string(),
                    notes: format!(
                        "No tagged GitHub Release published yet. Latest commit on main: {sha} — {msg}"
                    ),
                    asset_name: None,
                    asset_url: None,
                });
            }
            return Err(format!(
                "No published GitHub release yet (running development build {VERSION})."
            ));
        }
        return Err(format!("GitHub check unavailable ({})", stderr.trim()));
    }
    let val: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("Invalid JSON response: {e}"))?;
    let tag = val
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let release_url = val
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com/test3457890/Ai-Photo-Defense-App/releases")
        .to_string();
    let notes: String = val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();

    let wanted_ext = if cfg!(target_os = "windows") {
        ".msi"
    } else if cfg!(target_os = "macos") {
        ".dmg"
    } else {
        ".AppImage"
    };
    let mut asset_name = None;
    let mut asset_url = None;
    if let Some(assets) = val.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let dl = a
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name.ends_with(wanted_ext) && dl.starts_with("https://") {
                asset_name = Some(name.to_string());
                asset_url = Some(dl.to_string());
                break;
            }
        }
    }

    let latest_clean = tag.trim_start_matches('v');
    let update_available = latest_clean != VERSION && latest_clean != "unknown";
    Ok(UpdateInfo {
        current_version: VERSION.to_string(),
        latest_version: tag,
        update_available,
        release_url,
        notes,
        asset_name,
        asset_url,
    })
}

fn download_release_asset(name: &str, url: &str) -> Result<String, String> {
    if !url.starts_with("https://github.com/")
        && !url.starts_with("https://objects.githubusercontent.com/")
    {
        return Err("Refusing non-GitHub download URL".into());
    }
    let safe_name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if safe_name.is_empty() {
        return Err("Invalid asset filename".into());
    }
    let dl_dir = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| PathBuf::from(h).join("Downloads"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let dest = dl_dir.join(&safe_name);
    let ua = format!("AI-Photo-Defense/{VERSION}");
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "120", "-A", &ua, "-o"])
        .arg(&dest)
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to invoke curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let bytes = std::fs::read(&dest).map_err(|e| format!("Read downloaded file: {e}"))?;
    let fp = photo_core::fingerprint(&bytes);
    Ok(format!(
        "Downloaded {} ({} bytes) · SHA-256: {}",
        dest.display(),
        bytes.len(),
        fp.sha256
    ))
}

#[cfg(feature = "video")]
fn track_video_media(
    path: &Path,
    out_dir: &Path,
    rect_str: &str,
    time_s: f64,
    blur: bool,
) -> anyhow::Result<(PathBuf, String, Vec<String>)> {
    let parts: Vec<u32> = rect_str
        .split(',')
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!("Rect must be x,y,w,h"))?;
    anyhow::ensure!(parts.len() == 4, "Rect must be x,y,w,h");
    let kf = photo_media::video_track::Keyframe {
        time_s,
        x: parts[0],
        y: parts[1],
        width: parts[2],
        height: parts[3],
    };
    let (tracked, rects) = photo_media::video_track::track_object_in_mp4(path, &[kf], blur)?;
    let summary = photo_media::video_track::summarize_track(&tracked);
    let stem = photo_core::safe_stem(path)?;
    let mut out = out_dir.join(format!("{stem}-tracked.mp4"));
    let mut n = 2u32;
    while out.exists() {
        out = out_dir.join(format!("{stem}-tracked-{n}.mp4"));
        n += 1;
    }
    let r = photo_media::video::redact_mp4(path, &out, &rects)?;
    let verify = photo_media::video_track::verify_redacted_mp4(&out, &rects)?;
    let msg = format!(
        "Tracked & redacted {} frames ({} verified, {} failed, metadata clean={})",
        r.frame_count, verify.redacted_frames_checked, verify.failed_frames, verify.metadata_clean
    );
    Ok((out, msg, summary.warnings))
}

#[cfg(not(feature = "video"))]
fn track_video_media(
    _path: &Path,
    _out_dir: &Path,
    _rect_str: &str,
    _time_s: f64,
    _blur: bool,
) -> anyhow::Result<(PathBuf, String, Vec<String>)> {
    anyhow::bail!("Built without the `video` feature.")
}

fn open_releases_in_browser() {
    let url = "https://github.com/test3457890/Ai-Photo-Defense-App/releases";
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
