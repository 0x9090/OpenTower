#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(feature = "bundled-resources")]
use macroquad::miniquad::conf::Icon;
use macroquad::miniquad::window;
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use simtower_core::{
    Clock, ElevatorDirection, ElevatorMode, FLOOR_CONSTRUCTION_COST, Facility, FacilityKind,
    GridPosition, IncomeEvent, MAX_ELEVATOR_QUEUE_PER_FLOOR, MAX_FLOOR, MIN_FLOOR, Person,
    PersonActivity, PersonMood, PersonRole, PlacementError, SimulationSpeed, Tower,
    TowerStatusProgress, TrafficSimulation, elevator_car_cost, elevator_landing_is_valid,
    facility_is_open_at, is_weekend, vertical_walkway_story_span, vertical_walkway_story_span_at,
};
use simtower_formats::DibImage;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

mod resources;
use resources::*;

#[cfg(target_os = "windows")]
fn hidden_windows_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

// The original placement code converts x coordinates in 8-pixel slices and
// y coordinates in 36-pixel floors. Keeping that ratio is essential to the
// wide, low-detail perspective of the original tower view.
const CELL_WIDTH: f32 = 8.0;
const FLOOR_HEIGHT: f32 = 36.0;
const ROOM_HEIGHT: f32 = 24.0;
const PERSON_SPRITE_HEIGHT: f32 = 24.0;
const TOPBAR_HEIGHT: f32 = 64.0;
const MODE_MENU_LABELS: [&str; 4] = ["Edit", "Evaluation", "Rent", "Hotel"];
const RATING_STAR_WIDTH: f32 = 22.0;
const RATING_STAR_HEIGHT: f32 = 18.0;
const RATING_STAR_GAP: f32 = 2.0;
const RATING_ROW_WIDTH: f32 = 128.0;
const UI_TEXTURE_SCALE: f32 = 4.0;
const SAVE_FORMAT_VERSION: u32 = 1;
const SAVE_EXTENSION: &str = "smtower";
const DEFAULT_SAVE_NAME: &str = "Tower";
const UX_SCALE_SETTINGS_FILE: &str = "OpenTower.ux-scale";
const INTRO_MAXIS_SECONDS: f32 = 1.5;
const INTRO_TITLE_SECONDS: f32 = 1.5;
const INTRO_TOTAL_SECONDS: f32 = INTRO_MAXIS_SECONDS + INTRO_TITLE_SECONDS;
const TITLE_SCREEN_WIDTH: f32 = 640.0;
const TITLE_SCREEN_HEIGHT: f32 = 480.0;
static ACTIVE_UX_SCALE: AtomicU8 = AtomicU8::new(UxScale::One as u8);
static UI_CAMERA_ACTIVE: AtomicBool = AtomicBool::new(false);
const SPEED_CHOICES: [SimulationSpeed; 6] = [
    SimulationSpeed::Normal,
    SimulationSpeed::Fast,
    SimulationSpeed::Triple,
    SimulationSpeed::Quintuple,
    SimulationSpeed::Tenfold,
    SimulationSpeed::Paused,
];
const SKY_TILE_WIDTH: f32 = 200.0;
const SKY_HORIZON: f32 = 264.0;
const VERTICAL_WHEEL_FLOORS_PER_STEP: f32 = 0.5;
const HORIZONTAL_WHEEL_COLUMNS_PER_STEP: f32 = 2.5;
const LOBBY_HEIGHT_HELP: &str =
    "first lobby: Ctrl/Option = 2 stories; Ctrl/Option+Shift = 3; build upper floors first";
const EMERGENCY_STAIR_HALF_WIDTH: f32 = 24.0;
const BOMB_RANSOM_COST: i64 = 300_000;
const BOMB_SEARCH_SECONDS_PER_FLOOR: f32 = 2.5;
const BOMB_FUSE_SECONDS: f32 = 60.0;
// Each original Security Office supplies six search units. Deploy them one per
// floor so offices at different heights cover their neighboring floors in
// parallel instead of stacking several guards into the same search strip.
const BOMB_GUARDS_PER_SECURITY_OFFICE: usize = 6;
const PALETTE_X: f32 = 12.0;
const PALETTE_Y: f32 = 78.0;
const PALETTE_TITLE_HEIGHT: f32 = 20.0;
const PALETTE_BUTTON_SIZE: f32 = 38.0;
const PALETTE_GAP: f32 = 2.0;
const PALETTE_COLUMNS: usize = 3;
const SUBMENU_WIDTH: f32 = 164.0;
const SUBMENU_HEADER_HEIGHT: f32 = 19.0;
const SUBMENU_ROW_HEIGHT: f32 = 28.0;
const SUBMENU_ICON_SIZE: f32 = 26.0;
const SUBMENU_ICON_PADDING: f32 = 1.0;
const OVERVIEW_PANEL_WIDTH: f32 = 238.0;
const OVERVIEW_PANEL_HEIGHT: f32 = 390.0;

const HOTEL_SINGLE_VARIANT_IDS: [u16; 2] = [1192, 1194];
const HOTEL_TWIN_VARIANT_IDS: [u16; 4] = [1256, 1258, 1260, 1262];
const HOTEL_SUITE_VARIANT_IDS: [u16; 2] = [1320, 1322];
const OFFICE_VARIANT_IDS: [u16; 6] = [1448, 1448, 1449, 1449, 1450, 1450];
const CONDO_VARIANT_IDS: [u16; 3] = [1576, 1581, 1586];
const RESTAURANT_VARIANT_IDS: [u16; 5] = [1384, 1386, 1388, 1390, 1392];
const FAST_FOOD_VARIANT_IDS: [u16; 5] = [1768, 1770, 1772, 1774, 1776];
const SHOP_VARIANT_IDS: [u16; 11] = [
    1640, 1641, 1642, 1643, 1644, 1645, 1646, 1647, 1648, 1649, 1650,
];

// Facility bitmap groups follow the original executable's `1000 + type * 64`
// convention. Lobby interiors are the executable's custom raw resource 2536;
// bitmap 1001 is only the pair of exterior entrance awnings. The
// 1384/1448/1576 sound families are ambience, not build effects.
const CONSTRUCTION_SOUND_ID: u16 = 7000;
const LOBBY_SEGMENT_SOUND_ID: u16 = 7001;
const NO_MONEY_SOUND_ID: u16 = 7002;
const DEMOLITION_SOUND_ID: u16 = 7003;
const PAYMENT_SOUND_ID: u16 = 10013;
const FIRE_ALERT_SOUND_ID: u16 = 10006;
const FIRE_RESPONSE_SOUND_ID: u16 = 10009;
const ELEVATOR_MOVE_SOUND_ID: u16 = 6000;
const ELEVATOR_OPEN_SOUND_IDS: [u16; 2] = [6001, 6002];
const ELEVATOR_BELL_DIVISOR: u8 = 5;
const MAX_PAYMENT_SOUND_QUEUE: u8 = 30;
const PAYMENT_SOUND_INTERVAL: f32 = 0.5;
const FIRE_RESCUE_COST: i64 = 500_000;
const FIRE_QUICK_RESPONSE_SECONDS: f32 = 22.0;
const FIRE_HELICOPTER_DURATION_SECONDS: f32 = 12.0;
const FIRE_RESPONSE_DECISION_SECONDS: f32 = 15.0;
const QUEUE_CONCERNED_PINK: [u8; 3] = [255, 92, 152];
const CROWD_SOUND_ID: u16 = 8000;
const RESTAURANT_AMBIENCE_IDS: &[u16] = &[1384, 1385];
const OFFICE_AMBIENCE_IDS: &[u16] = &[1448];
const CONDO_AMBIENCE_IDS: &[u16] = &[1576, 1577];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FacilityResourceIds {
    bitmap: u16,
    source_width: u16,
    source_height: u16,
    ambience: &'static [u16],
}

const fn facility_resource_ids(kind: FacilityKind) -> FacilityResourceIds {
    match kind {
        FacilityKind::Lobby => FacilityResourceIds {
            bitmap: 2536,
            source_width: 992,
            source_height: 36,
            ambience: &[],
        },
        FacilityKind::Office => FacilityResourceIds {
            bitmap: 1448,
            source_width: 72,
            source_height: 24,
            ambience: OFFICE_AMBIENCE_IDS,
        },
        FacilityKind::Condo => FacilityResourceIds {
            bitmap: 1576,
            source_width: 128,
            source_height: 24,
            ambience: CONDO_AMBIENCE_IDS,
        },
        FacilityKind::HotelSingle => FacilityResourceIds {
            bitmap: 1192,
            source_width: 32,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::HotelTwin => FacilityResourceIds {
            bitmap: 1256,
            source_width: 48,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::HotelSuite => FacilityResourceIds {
            bitmap: 1320,
            source_width: 80,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Restaurant => FacilityResourceIds {
            bitmap: 1384,
            source_width: 192,
            source_height: 24,
            ambience: RESTAURANT_AMBIENCE_IDS,
        },
        FacilityKind::FastFood => FacilityResourceIds {
            bitmap: 1768,
            source_width: 128,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Shop => FacilityResourceIds {
            bitmap: 1640,
            source_width: 96,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Cinema => FacilityResourceIds {
            bitmap: 2152,
            source_width: 192,
            source_height: 60,
            ambience: &[],
        },
        FacilityKind::PartyHall => FacilityResourceIds {
            bitmap: 2856,
            source_width: 192,
            source_height: 60,
            ambience: &[],
        },
        FacilityKind::Metro => FacilityResourceIds {
            bitmap: 2984,
            source_width: 240,
            source_height: 96,
            ambience: &[],
        },
        FacilityKind::Parking => FacilityResourceIds {
            bitmap: 1704,
            source_width: 32,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Medical => FacilityResourceIds {
            bitmap: 1832,
            source_width: 208,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Security => FacilityResourceIds {
            bitmap: 1896,
            source_width: 128,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Recycling => FacilityResourceIds {
            bitmap: 2280,
            source_width: 200,
            source_height: 60,
            ambience: &[],
        },
        FacilityKind::Stairs => FacilityResourceIds {
            bitmap: 2408,
            source_width: 64,
            source_height: 60,
            ambience: &[],
        },
        FacilityKind::Escalator => FacilityResourceIds {
            bitmap: 2728,
            source_width: 64,
            source_height: 72,
            ambience: &[],
        },
        FacilityKind::Elevator => FacilityResourceIds {
            bitmap: 1064,
            source_width: 32,
            source_height: 36,
            ambience: &[],
        },
        FacilityKind::ServiceElevator => FacilityResourceIds {
            bitmap: 1066,
            source_width: 32,
            source_height: 36,
            ambience: &[],
        },
        FacilityKind::ExpressElevator => FacilityResourceIds {
            bitmap: 1067,
            source_width: 48,
            source_height: 36,
            ambience: &[],
        },
        FacilityKind::Housekeeping => FacilityResourceIds {
            bitmap: 1960,
            source_width: 120,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Ramp => FacilityResourceIds {
            bitmap: 3816,
            source_width: 128,
            source_height: 24,
            ambience: &[],
        },
        FacilityKind::Cathedral => FacilityResourceIds {
            bitmap: 3304,
            source_width: 224,
            source_height: 180,
            ambience: &[],
        },
    }
}

const CLASSIC_FACE: Color = Color::new(0.75, 0.75, 0.75, 1.0);
const PROMOTION_FACE: Color = Color::new(0.8, 0.8, 0.8, 1.0);
const FINANCE_TABLE_FACE: Color = Color::new(0.9, 0.9, 0.9, 1.0);
const CLASSIC_LIGHT: Color = Color::new(0.96, 0.96, 0.96, 1.0);
const CLASSIC_SHADOW: Color = Color::new(0.32, 0.32, 0.32, 1.0);
const CLASSIC_DARK: Color = Color::new(0.12, 0.12, 0.12, 1.0);
const TITLE_BLUE: Color = Color::new(0.06, 0.16, 0.48, 1.0);
const CONSTRUCTION_SECONDS: f32 = 2.8;
const LOBBY_BODY_WIDTH: f32 = 256.0;
const LOBBY_CHUNK_WIDTH: f32 = 328.0;
const LOBBY_FACADE_OFFSET: f32 = 272.0;
const LOBBY_FACADE_WIDTH: f32 = 56.0;
const LOBBY_AWNING_HALF_WIDTH: f32 = 56.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeasonalEventKind {
    Santa,
    Witch,
}

#[derive(Clone, Copy, Debug)]
struct SeasonalEventState {
    kind: SeasonalEventKind,
    x: f32,
}

#[derive(Clone, Copy, Debug)]
struct TreasurePopup {
    position: GridPosition,
    amount: i64,
    remaining: f32,
}

#[derive(Clone, Copy, Debug)]
struct FireEventState {
    facility_id: u64,
    elapsed: f32,
    decision_elapsed: f32,
    duration: f32,
    response: FireResponse,
    saves_unit: bool,
    security_start_x: Option<f32>,
    resume_speed: SimulationSpeed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FireResponse {
    AwaitingDecision,
    Security,
    Helicopter,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct VipVisitState {
    suite_id: u64,
    person_id: u64,
    started_minute: u64,
    worst_mood: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VipDialogKind {
    Arrival,
    Approved,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct BombEventState {
    facility_id: u64,
    bomb_floor: i16,
    security_floor: i16,
    elapsed: f32,
    #[serde(default)]
    decision_elapsed: f32,
    response: BombResponse,
    resume_speed: SimulationSpeed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum BombResponse {
    AwaitingDecision,
    Searching,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct FinanceLedger {
    tenant_income: i64,
    maintenance: i64,
    #[serde(default)]
    construction_costs: i64,
    #[serde(default)]
    other_income: i64,
    #[serde(default)]
    income_by_kind: HashMap<FacilityKind, i64>,
    #[serde(default)]
    maintenance_by_kind: HashMap<FacilityKind, i64>,
    #[serde(default)]
    period_start_balance: i64,
    #[serde(default)]
    last_observed_cash: i64,
    #[serde(default)]
    last_report: Option<QuarterlyReport>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct QuarterlyReport {
    year: u32,
    quarter: u8,
    total_income: i64,
    total_maintenance: i64,
    construction_costs: i64,
    other_income: i64,
    starting_balance: i64,
    ending_balance: i64,
    income_by_kind: HashMap<FacilityKind, i64>,
    maintenance_by_kind: HashMap<FacilityKind, i64>,
    population_by_kind: HashMap<FacilityKind, u32>,
}

#[derive(Deserialize, Serialize)]
struct SaveGame {
    format_version: u32,
    tower: Tower,
    traffic: TrafficSimulation,
    camera_x: f32,
    camera_floor: f32,
    last_running_speed: SimulationSpeed,
    next_fire_minute: u64,
    finance_ledger: FinanceLedger,
    #[serde(default)]
    options: SaveOptions,
    #[serde(default, rename = "automatic_finance_reports", skip_serializing)]
    legacy_automatic_finance_reports: Option<bool>,
    #[serde(default)]
    tenant_variants: HashMap<u64, usize>,
    #[serde(default)]
    tenant_variant_bags: HashMap<FacilityKind, TenantVariantBag>,
    #[serde(default)]
    tenant_names: HashMap<u64, String>,
    #[serde(default)]
    person_names: HashMap<u64, String>,
    #[serde(default)]
    tenant_finances: HashMap<u64, TenantFinanceState>,
    #[serde(default)]
    hotel_guest_stays: HashSet<u64>,
    #[serde(default)]
    vip_visit: Option<VipVisitState>,
    #[serde(default)]
    last_vip_attempt_day: u32,
    #[serde(default)]
    bomb_event: Option<BombEventState>,
}

impl SaveGame {
    fn resolved_options(&self) -> SaveOptions {
        let mut options = self.options;
        if let Some(automatic_finance_reports) = self.legacy_automatic_finance_reports {
            options.automatic_finance_reports = automatic_finance_reports;
        }
        options
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct TenantFinanceState {
    current_day: u32,
    current_profit: i64,
    previous_profit: i64,
}

struct TenantPanelState {
    facility_id: u64,
    renaming: bool,
    rename_draft: String,
    original_name: String,
}

struct PersonPanelState {
    person_id: u64,
    renaming: bool,
    rename_draft: String,
    original_name: String,
}

struct ParkingState {
    connected: HashSet<u64>,
    occupied: HashSet<u64>,
    last_absolute_minute: u64,
    layout_dirty: bool,
}

impl Default for ParkingState {
    fn default() -> Self {
        Self {
            connected: HashSet::new(),
            occupied: HashSet::new(),
            last_absolute_minute: u64::MAX,
            layout_dirty: true,
        }
    }
}

struct SaveNameDialog {
    name: String,
    error: Option<String>,
    overwrite_path: Option<PathBuf>,
    replace_default_on_input: bool,
    key_repeat: Option<TextKeyRepeat>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TextEditAction {
    Backspace,
    Insert(char),
}

#[derive(Clone, Copy, Debug)]
struct TextKeyRepeat {
    key: KeyCode,
    action: TextEditAction,
    elapsed: f32,
    next_repeat: f32,
}

impl TextKeyRepeat {
    fn advance(&mut self, dt: f32) -> usize {
        self.elapsed += dt.max(0.0);
        let mut repeats = 0;
        while self.elapsed >= self.next_repeat {
            repeats += 1;
            self.next_repeat += TEXT_REPEAT_INTERVAL_SECONDS;
        }
        repeats
    }
}

const TEXT_REPEAT_DELAY_SECONDS: f32 = 0.42;
const TEXT_REPEAT_INTERVAL_SECONDS: f32 = 0.045;

impl SaveNameDialog {
    fn with_default_name(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            error: None,
            overwrite_path: None,
            replace_default_on_input: true,
            key_repeat: None,
        }
    }

    fn backspace(&mut self) {
        if self.replace_default_on_input {
            self.name.clear();
            self.replace_default_on_input = false;
        } else {
            self.name.pop();
        }
        self.error = None;
    }

    fn push_character(&mut self, character: char) {
        if character.is_control() || self.name.chars().count() >= 64 {
            return;
        }
        if self.replace_default_on_input {
            self.name.clear();
            self.replace_default_on_input = false;
        }
        self.name.push(character);
        self.error = None;
    }

    fn start_key_repeat(&mut self, key: KeyCode, action: TextEditAction) {
        self.key_repeat = Some(TextKeyRepeat {
            key,
            action,
            elapsed: 0.0,
            next_repeat: TEXT_REPEAT_DELAY_SECONDS,
        });
    }

    fn is_repeating_key(&self, key: KeyCode) -> bool {
        self.key_repeat.is_some_and(|repeat| repeat.key == key)
    }

    fn update_key_repeat(&mut self, dt: f32) {
        let Some(mut repeat) = self.key_repeat.take() else {
            return;
        };
        if !is_key_down(repeat.key) {
            return;
        }
        for _ in 0..repeat.advance(dt) {
            match repeat.action {
                TextEditAction::Backspace => self.backspace(),
                TextEditAction::Insert(character) => self.push_character(character),
            }
        }
        self.key_repeat = Some(repeat);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
enum UxScale {
    One = 4,
    OneAndQuarter = 5,
    OneAndHalf = 6,
    Two = 8,
}

const UX_SCALE_CHOICES: [UxScale; 4] = [
    UxScale::One,
    UxScale::OneAndQuarter,
    UxScale::OneAndHalf,
    UxScale::Two,
];

impl UxScale {
    const fn factor(self) -> f32 {
        self as u8 as f32 / 4.0
    }

    const fn label(self) -> &'static str {
        match self {
            Self::One => "1x",
            Self::OneAndQuarter => "1.25x",
            Self::OneAndHalf => "1.5x",
            Self::Two => "2x",
        }
    }

    const fn preference_value(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::OneAndQuarter => "1.25",
            Self::OneAndHalf => "1.5",
            Self::Two => "2",
        }
    }

    const fn from_quarters(value: u8) -> Self {
        match value {
            5 => Self::OneAndQuarter,
            6 => Self::OneAndHalf,
            8 => Self::Two,
            _ => Self::One,
        }
    }

    fn from_preference(value: &str) -> Option<Self> {
        match value.trim() {
            "1" | "1x" => Some(Self::One),
            "1.25" | "1.25x" => Some(Self::OneAndQuarter),
            "1.5" | "1.5x" => Some(Self::OneAndHalf),
            "2" | "2x" | "3" | "3x" => Some(Self::Two),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
struct SaveOptions {
    muted: bool,
    ux_scale: UxScale,
    automatic_finance_reports: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            muted: false,
            ux_scale: UxScale::One,
            automatic_finance_reports: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameMenuAction {
    New,
    Save,
    Load,
    AutomaticReports,
    UxScale,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WelcomeAction {
    NewTower,
    LoadSavedTower,
    ExitProgram,
}

const WELCOME_ACTIONS: [(WelcomeAction, &str); 3] = [
    (WelcomeAction::NewTower, "New Tower"),
    (WelcomeAction::LoadSavedTower, "Load Saved Tower"),
    (WelcomeAction::ExitProgram, "Quit"),
];

const GAME_MENU_ACTIONS: [(GameMenuAction, &str); 6] = [
    (GameMenuAction::New, "New"),
    (GameMenuAction::Save, "Save"),
    (GameMenuAction::Load, "Load"),
    (GameMenuAction::AutomaticReports, "Automatic reports"),
    (GameMenuAction::UxScale, "UX scale"),
    (GameMenuAction::Quit, "Quit"),
];

impl FinanceLedger {
    fn new(starting_balance: i64) -> Self {
        Self {
            period_start_balance: starting_balance,
            last_observed_cash: starting_balance,
            ..Self::default()
        }
    }

    fn ensure_initialized(&mut self, cash: i64) {
        if self.period_start_balance == 0 && self.last_observed_cash == 0 {
            self.period_start_balance = cash;
            self.last_observed_cash = cash;
        }
    }

    fn reconcile_external_cash(&mut self, cash: i64) {
        let change = cash.saturating_sub(self.last_observed_cash);
        if change < 0 {
            self.construction_costs = self
                .construction_costs
                .saturating_add(change.saturating_abs());
        } else if change > 0 {
            self.other_income = self.other_income.saturating_add(change);
        }
        self.last_observed_cash = cash;
    }

    fn record(&mut self, events: &[simtower_core::IncomeEvent]) {
        for event in events {
            if event.amount >= 0 {
                self.tenant_income = self.tenant_income.saturating_add(event.amount);
                let total = self.income_by_kind.entry(event.kind).or_default();
                *total = total.saturating_add(event.amount);
            } else {
                self.maintenance = self
                    .maintenance
                    .saturating_add(event.amount.saturating_abs());
                let total = self.maintenance_by_kind.entry(event.kind).or_default();
                *total = total.saturating_add(event.amount.saturating_abs());
            }
        }
    }

    fn record_emergency_charge(&mut self, amount: i64, current_cash: i64) {
        self.maintenance = self.maintenance.saturating_add(amount);
        let security = self
            .maintenance_by_kind
            .entry(FacilityKind::Security)
            .or_default();
        *security = security.saturating_add(amount);
        self.last_observed_cash = current_cash;
    }

    fn close_quarter(
        &mut self,
        year: u32,
        quarter: u8,
        ending_balance: i64,
        population_by_kind: HashMap<FacilityKind, u32>,
    ) {
        self.last_report = Some(QuarterlyReport {
            year,
            quarter,
            total_income: self.tenant_income,
            total_maintenance: self.maintenance,
            construction_costs: self.construction_costs,
            other_income: self.other_income,
            starting_balance: self.period_start_balance,
            ending_balance,
            income_by_kind: std::mem::take(&mut self.income_by_kind),
            maintenance_by_kind: std::mem::take(&mut self.maintenance_by_kind),
            population_by_kind,
        });
        self.tenant_income = 0;
        self.maintenance = 0;
        self.construction_costs = 0;
        self.other_income = 0;
        self.period_start_balance = ending_balance;
        self.last_observed_cash = ending_balance;
    }
}

fn record_tenant_finances(
    finances: &mut HashMap<u64, TenantFinanceState>,
    day: u32,
    events: &[IncomeEvent],
) {
    for state in finances.values_mut() {
        if state.current_day == 0 {
            state.current_day = day;
        } else if state.current_day != day {
            state.previous_profit = if state.current_day.saturating_add(1) == day {
                state.current_profit
            } else {
                0
            };
            state.current_profit = 0;
            state.current_day = day;
        }
    }
    for event in events.iter().filter(|event| event.amount > 0) {
        let state = finances.entry(event.facility_id).or_default();
        if state.current_day == 0 {
            state.current_day = day;
        } else if state.current_day != day {
            state.previous_profit = if state.current_day.saturating_add(1) == day {
                state.current_profit
            } else {
                0
            };
            state.current_profit = 0;
            state.current_day = day;
        }
        state.current_profit = state.current_profit.saturating_add(event.amount);
    }
}

fn window_conf() -> Conf {
    #[cfg(feature = "bundled-resources")]
    let icon = Some(original_window_icon().expect("the original SimTower icon should decode"));
    #[cfg(not(feature = "bundled-resources"))]
    let icon = None;
    Conf {
        window_title: "OpenTower - SimTower compatibility project".to_owned(),
        window_width: 1600,
        window_height: 1000,
        high_dpi: true,
        icon,
        ..Default::default()
    }
}

#[cfg(feature = "bundled-resources")]
fn original_window_icon() -> Result<Icon, String> {
    // Keep validating the bundled icon so a damaged resource package still
    // fails early, but render the desktop icon from resolution-independent
    // geometry. Enlarging the original 32-pixel icon made the 64-pixel dock
    // and taskbar versions visibly blocky on high-DPI displays.
    DibImage::decode_bmp(SIMTOWER_ICON_BMP.bytes()?).map_err(|error| error.to_string())?;
    Ok(Icon {
        small: render_opentower_icon::<{ 16 * 16 * 4 }>(16),
        medium: render_opentower_icon::<{ 32 * 32 * 4 }>(32),
        big: render_opentower_icon::<{ 64 * 64 * 4 }>(64),
    })
}

#[cfg(feature = "bundled-resources")]
fn render_opentower_icon<const LEN: usize>(side: usize) -> [u8; LEN] {
    assert_eq!(LEN, side * side * 4);
    let mut output = [0_u8; LEN];
    const SAMPLES: usize = 4;
    for y in 0..side {
        for x in 0..side {
            let target = (y * side + x) * 4;
            let mut accumulated = [0_u16; 4];
            for sample_y in 0..SAMPLES {
                for sample_x in 0..SAMPLES {
                    let px = (x as f32 + (sample_x as f32 + 0.5) / SAMPLES as f32) / side as f32;
                    let py = (y as f32 + (sample_y as f32 + 0.5) / SAMPLES as f32) / side as f32;
                    let color = opentower_icon_sample(px, py);
                    for channel in 0..4 {
                        accumulated[channel] += u16::from(color[channel]);
                    }
                }
            }
            for channel in 0..4 {
                output[target + channel] =
                    (accumulated[channel] / (SAMPLES * SAMPLES) as u16) as u8;
            }
        }
    }
    output
}

#[cfg(feature = "bundled-resources")]
fn opentower_icon_sample(x: f32, y: f32) -> [u8; 4] {
    let inside = |left: f32, top: f32, right: f32, bottom: f32| {
        x >= left && x <= right && y >= top && y <= bottom
    };
    let black = [12, 12, 16, 255];
    let yellow = [255, 238, 0, 255];
    let red = [238, 34, 24, 255];
    let white = [248, 248, 244, 255];

    // Black frame and ground preserve the original SimTower icon's visual
    // weight even at the 16-pixel system-icon size.
    if !(0.035..=0.965).contains(&x) || !(0.035..=0.88).contains(&y) {
        return black;
    }

    let mut color = white;
    // Distant yellow skyline.
    if y > 0.39
        || inside(0.06, 0.31, 0.13, 0.88)
        || inside(0.17, 0.36, 0.24, 0.88)
        || inside(0.28, 0.28, 0.35, 0.88)
        || inside(0.72, 0.32, 0.79, 0.88)
        || inside(0.84, 0.27, 0.91, 0.88)
    {
        color = yellow;
    }
    // Near red skyline.
    if y > 0.62
        || inside(0.08, 0.55, 0.17, 0.88)
        || inside(0.22, 0.58, 0.31, 0.88)
        || inside(0.68, 0.57, 0.78, 0.88)
        || inside(0.82, 0.52, 0.93, 0.88)
    {
        color = red;
    }

    // Central OpenTower silhouette with a narrow antenna crown.
    if inside(0.46, 0.10, 0.54, 0.23)
        || inside(0.40, 0.22, 0.60, 0.88)
        || inside(0.35, 0.34, 0.65, 0.88)
    {
        color = black;
        let window_column = if x < 0.47 {
            0
        } else if x > 0.53 {
            2
        } else {
            1
        };
        let window_row = ((y - 0.39) / 0.105).floor() as i32;
        let window_center_x = [0.425, 0.5, 0.575][window_column];
        let window_center_y = 0.42 + window_row as f32 * 0.105;
        if (x - window_center_x).abs() < 0.024
            && (y - window_center_y).abs() < 0.022
            && (0..5).contains(&window_row)
        {
            color = yellow;
        }
    }

    // A few square lights keep the two background layers legible at 32/64px.
    for (window_x, window_y) in [
        (0.12, 0.48),
        (0.23, 0.44),
        (0.31, 0.53),
        (0.72, 0.46),
        (0.86, 0.42),
    ] {
        if (x - window_x).abs() < 0.018 && (y - window_y).abs() < 0.018 {
            color = white;
        }
    }
    for (window_x, window_y) in [(0.13, 0.70), (0.26, 0.75), (0.73, 0.69), (0.87, 0.73)] {
        if (x - window_x).abs() < 0.019 && (y - window_y).abs() < 0.019 {
            color = yellow;
        }
    }
    color
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildMenu {
    Structure,
    Tenants,
    FoodRetail,
    Transport,
    Services,
    Entertainment,
}

impl BuildMenu {
    const fn title(self) -> &'static str {
        match self {
            Self::Structure => "Structure",
            Self::Tenants => "Tenants",
            Self::FoodRetail => "Food & retail",
            Self::Transport => "Transport",
            Self::Services => "Services",
            Self::Entertainment => "Entertainment",
        }
    }

    fn entries(self) -> Vec<MenuEntry> {
        match self {
            Self::Structure => vec![
                MenuEntry::floor("Floor", 1, 0),
                MenuEntry::available("Lobby", FacilityKind::Lobby, 0, 0),
                MenuEntry::available("Stairs", FacilityKind::Stairs, 2, 0),
            ],
            Self::Tenants => vec![
                MenuEntry::available("Office", FacilityKind::Office, 7, 0),
                MenuEntry::available("Condominium", FacilityKind::Condo, 0, 1),
                MenuEntry::available("Hotel single", FacilityKind::HotelSingle, 1, 1),
                MenuEntry::available("Hotel twin", FacilityKind::HotelTwin, 2, 1),
                MenuEntry::available("Hotel suite", FacilityKind::HotelSuite, 2, 1),
            ],
            Self::FoodRetail => vec![
                MenuEntry::available("Restaurant", FacilityKind::Restaurant, 4, 1),
                MenuEntry::available("Fast food", FacilityKind::FastFood, 3, 1),
                MenuEntry::available("Shop", FacilityKind::Shop, 5, 1),
            ],
            Self::Transport => vec![
                MenuEntry::available("Elevator", FacilityKind::Elevator, 4, 0),
                MenuEntry::available("Service elevator", FacilityKind::ServiceElevator, 5, 0),
                MenuEntry::available("Express elevator", FacilityKind::ExpressElevator, 6, 0),
                MenuEntry::available("Escalator", FacilityKind::Escalator, 3, 0),
                MenuEntry::available("Parking", FacilityKind::Parking, 1, 2),
                MenuEntry::available("Parking ramp", FacilityKind::Ramp, 2, 2),
                MenuEntry::available("Metro station", FacilityKind::Metro, 3, 2),
            ],
            Self::Services => vec![
                MenuEntry::available("Security", FacilityKind::Security, 1, 3),
                MenuEntry::available("Medical center", FacilityKind::Medical, 6, 2),
                MenuEntry::available("Housekeeping", FacilityKind::Housekeeping, 3, 1),
                MenuEntry::available("Recycling center", FacilityKind::Recycling, 5, 2),
            ],
            Self::Entertainment => vec![
                MenuEntry::available("Movie theater", FacilityKind::Cinema, 6, 1),
                MenuEntry::available("Party hall", FacilityKind::PartyHall, 7, 1),
                MenuEntry::available("Cathedral", FacilityKind::Cathedral, 7, 2),
            ],
        }
    }

    const fn unlock_stars(self) -> u8 {
        match self {
            Self::Structure | Self::Tenants | Self::FoodRetail | Self::Transport => 1,
            Self::Services => 2,
            Self::Entertainment => 3,
        }
    }
}

#[derive(Clone, Copy)]
struct MenuEntry {
    label: &'static str,
    kind: Option<FacilityKind>,
    builds_floor: bool,
    disabled_reason: Option<&'static str>,
    atlas_column: u8,
    atlas_row: u8,
}

impl MenuEntry {
    const fn available(
        label: &'static str,
        kind: FacilityKind,
        atlas_column: u8,
        atlas_row: u8,
    ) -> Self {
        Self {
            label,
            kind: Some(kind),
            builds_floor: false,
            disabled_reason: None,
            atlas_column,
            atlas_row,
        }
    }

    const fn floor(label: &'static str, atlas_column: u8, atlas_row: u8) -> Self {
        Self {
            label,
            kind: None,
            builds_floor: true,
            disabled_reason: None,
            atlas_column,
            atlas_row,
        }
    }

    const fn required_stars(self) -> u8 {
        match self.kind {
            Some(kind) => kind.unlock_stars(),
            None => 1,
        }
    }

    const fn is_available(self, rating: u8) -> bool {
        (self.kind.is_some() || self.builds_floor) && rating >= self.required_stars()
    }
}

#[derive(Clone, Copy)]
enum PaletteAction {
    Speed,
    Inspect,
    Demolish,
    Menu(BuildMenu),
}

#[derive(Clone, Copy)]
enum ToolIcon {
    Pause,
    Pointer(u8),
    Build(u8, u8),
}

#[derive(Clone, Copy)]
struct PaletteButton {
    label: &'static str,
    action: PaletteAction,
    icon: ToolIcon,
}

const PALETTE_BUTTONS: [PaletteButton; 9] = [
    PaletteButton {
        label: "Speed",
        action: PaletteAction::Speed,
        icon: ToolIcon::Pause,
    },
    PaletteButton {
        label: "Inspect",
        action: PaletteAction::Inspect,
        icon: ToolIcon::Pointer(2),
    },
    PaletteButton {
        label: "Bulldoze",
        action: PaletteAction::Demolish,
        icon: ToolIcon::Pointer(0),
    },
    PaletteButton {
        label: "Structure",
        action: PaletteAction::Menu(BuildMenu::Structure),
        icon: ToolIcon::Build(0, 0),
    },
    PaletteButton {
        label: "Tenants",
        action: PaletteAction::Menu(BuildMenu::Tenants),
        icon: ToolIcon::Build(7, 0),
    },
    PaletteButton {
        label: "Food & retail",
        action: PaletteAction::Menu(BuildMenu::FoodRetail),
        icon: ToolIcon::Build(4, 1),
    },
    PaletteButton {
        label: "Transport",
        action: PaletteAction::Menu(BuildMenu::Transport),
        icon: ToolIcon::Build(4, 0),
    },
    PaletteButton {
        label: "Services",
        action: PaletteAction::Menu(BuildMenu::Services),
        icon: ToolIcon::Build(1, 3),
    },
    PaletteButton {
        label: "Entertainment",
        action: PaletteAction::Menu(BuildMenu::Entertainment),
        icon: ToolIcon::Build(6, 1),
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolMode {
    Floor,
    Build(FacilityKind),
    Inspect,
    Demolish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DemolitionTarget {
    Facility(u64),
    Floor(GridPosition),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewMode {
    Edit,
    Evaluation,
    Pricing,
    Hotel,
}

impl ViewMode {
    const fn menu_index(self) -> usize {
        match self {
            Self::Edit => 0,
            Self::Evaluation => 1,
            Self::Pricing => 2,
            Self::Hotel => 3,
        }
    }

    const fn label(self) -> &'static str {
        MODE_MENU_LABELS[self.menu_index()]
    }

    const fn from_menu_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Edit),
            1 => Some(Self::Evaluation),
            2 => Some(Self::Pricing),
            3 => Some(Self::Hotel),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FacilityCrowd {
    Closed,
    Empty,
    Light,
    Heavy,
}

struct TenantEvaluationContext {
    mood_penalties: HashMap<u64, i32>,
    noisy_sources: Vec<(i32, i16)>,
    recycling_capacity: usize,
}

struct ConstructionAnimation {
    facility_id: u64,
    remaining: f32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct TenantVariantBag {
    remaining: Vec<usize>,
    last: Option<usize>,
}

impl TenantVariantBag {
    fn next(&mut self, variant_count: usize) -> usize {
        debug_assert!(variant_count > 0);
        if self.remaining.is_empty() {
            self.remaining.extend(0..variant_count);
            for index in (1..self.remaining.len()).rev() {
                let other = macroquad::rand::gen_range(0, (index + 1) as i32) as usize;
                self.remaining.swap(index, other);
            }
            if self.remaining.len() > 1 && self.remaining.last() == self.last.as_ref() {
                self.remaining.swap(0, variant_count - 1);
            }
        }
        let choice = self.remaining.pop().expect("refilled tenant variant bag");
        self.last = Some(choice);
        choice
    }
}

fn tenant_variant_count(kind: FacilityKind) -> Option<usize> {
    match kind {
        FacilityKind::Office => Some(OFFICE_VARIANT_IDS.len()),
        FacilityKind::Condo => Some(CONDO_VARIANT_IDS.len()),
        FacilityKind::HotelSingle => Some(HOTEL_SINGLE_VARIANT_IDS.len()),
        FacilityKind::HotelTwin => Some(HOTEL_TWIN_VARIANT_IDS.len()),
        FacilityKind::HotelSuite => Some(HOTEL_SUITE_VARIANT_IDS.len()),
        FacilityKind::Restaurant => Some(RESTAURANT_VARIANT_IDS.len()),
        FacilityKind::FastFood => Some(FAST_FOOD_VARIANT_IDS.len()),
        FacilityKind::Shop => Some(SHOP_VARIANT_IDS.len()),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct ElevatorDrag {
    kind: FacilityKind,
    x: u16,
    last_floor: i16,
    direction: i8,
}

#[derive(Clone, Copy)]
struct ElevatorPanelState {
    shaft_id: u64,
    weekend: bool,
    period: usize,
    simulate: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TrafficAudioSnapshot {
    people: usize,
    moving_cars: usize,
    passengers: usize,
    escalator_riders: usize,
}

impl TrafficAudioSnapshot {
    fn capture(traffic: &TrafficSimulation, escalator_riders: usize) -> Self {
        Self {
            people: traffic.people().len(),
            moving_cars: traffic
                .elevators()
                .iter()
                .flat_map(|shaft| &shaft.cars)
                .filter(|car| car.direction != ElevatorDirection::Idle)
                .count(),
            passengers: traffic
                .elevators()
                .iter()
                .flat_map(|shaft| &shaft.cars)
                .map(|car| car.passengers.len())
                .sum(),
            escalator_riders,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrafficSoundEvent {
    ElevatorMove,
    ElevatorOpen,
    Crowd,
    EscalatorMove,
}

fn traffic_sound_events(
    previous: TrafficAudioSnapshot,
    current: TrafficAudioSnapshot,
) -> Vec<TrafficSoundEvent> {
    let mut events = Vec::with_capacity(3);
    if current.moving_cars > previous.moving_cars {
        events.push(TrafficSoundEvent::ElevatorMove);
    }
    if current.moving_cars < previous.moving_cars || current.passengers != previous.passengers {
        events.push(TrafficSoundEvent::ElevatorOpen);
    }
    if current.people > previous.people {
        events.push(TrafficSoundEvent::Crowd);
    }
    if current.escalator_riders > previous.escalator_riders {
        events.push(TrafficSoundEvent::EscalatorMove);
    }
    events
}

fn escalator_rider_count(tower: &Tower, traffic: &TrafficSimulation) -> usize {
    traffic
        .people()
        .iter()
        .filter(|person| person.activity == PersonActivity::UsingStairs)
        .filter(|person| {
            tower.facilities().iter().any(|facility| {
                if facility.kind != FacilityKind::Escalator {
                    return false;
                }
                let base = simtower_core::floor_ordinal(facility.position.floor);
                let center_x =
                    f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5;
                (person.x - center_x).abs() < 0.1
                    && person.floor_position >= f32::from(base) - 0.01
                    && person.floor_position <= f32::from(base + 1) + 0.01
            })
        })
        .count()
}

fn escalator_frame_for_position(
    base_floor: i16,
    origin_floor: i16,
    floor_position: f32,
    flights: u16,
) -> usize {
    let base = f32::from(simtower_core::floor_ordinal(base_floor));
    let progress_upward = ((floor_position - base) / f32::from(flights.max(1))).clamp(0.0, 1.0);
    let descending =
        simtower_core::floor_ordinal(origin_floor) > simtower_core::floor_ordinal(base_floor);
    let travel_progress = if descending {
        1.0 - progress_upward
    } else {
        progress_upward
    };
    (travel_progress * 7.0).round().clamp(0.0, 7.0) as usize
}

#[derive(Default)]
struct SimulationSoundState {
    previous: TrafficAudioSnapshot,
    initialized: bool,
    elevator_move_cooldown: f32,
    elevator_open_cooldown: f32,
    crowd_cooldown: f32,
    escalator_cooldown: f32,
    elevator_open_variant: usize,
    elevator_bell_phase: u8,
    payment_pending: u8,
    payment_cooldown: f32,
}

fn should_play_elevator_bell(phase: &mut u8) -> bool {
    let play = *phase == 0;
    *phase = (*phase + 1) % ELEVATOR_BELL_DIVISOR;
    play
}

impl SimulationSoundState {
    fn queue_payments(&mut self, events: &[IncomeEvent]) {
        if events.iter().any(|event| event.amount > 0) {
            self.payment_pending = self
                .payment_pending
                .saturating_add(1)
                .min(MAX_PAYMENT_SOUND_QUEUE);
        }
    }

    fn update(
        &mut self,
        dt: f32,
        traffic: &TrafficSimulation,
        sounds: &NativeSounds,
        simulation_paused: bool,
        escalator_riders: usize,
    ) {
        self.elevator_move_cooldown = (self.elevator_move_cooldown - dt).max(0.0);
        self.elevator_open_cooldown = (self.elevator_open_cooldown - dt).max(0.0);
        self.crowd_cooldown = (self.crowd_cooldown - dt).max(0.0);
        self.escalator_cooldown = (self.escalator_cooldown - dt).max(0.0);
        if !simulation_paused {
            self.payment_cooldown = (self.payment_cooldown - dt).max(0.0);
            if self.payment_pending > 0 && self.payment_cooldown <= 0.0 {
                sounds.play_payment();
                self.payment_pending -= 1;
                self.payment_cooldown = PAYMENT_SOUND_INTERVAL;
            }
        }

        let current = TrafficAudioSnapshot::capture(traffic, escalator_riders);
        if !self.initialized {
            self.previous = current;
            self.initialized = true;
            return;
        }

        for event in traffic_sound_events(self.previous, current) {
            match event {
                TrafficSoundEvent::ElevatorMove if self.elevator_move_cooldown <= 0.0 => {
                    sounds.play_elevator_move();
                    self.elevator_move_cooldown = 0.75;
                }
                TrafficSoundEvent::ElevatorOpen => {
                    if should_play_elevator_bell(&mut self.elevator_bell_phase)
                        && self.elevator_open_cooldown <= 0.0
                    {
                        sounds.play_elevator_open(self.elevator_open_variant);
                        self.elevator_open_variant = (self.elevator_open_variant + 1) % 2;
                        self.elevator_open_cooldown = 0.5;
                    }
                }
                TrafficSoundEvent::Crowd if self.crowd_cooldown <= 0.0 => {
                    sounds.play_crowd();
                    self.crowd_cooldown = 5.0;
                }
                TrafficSoundEvent::EscalatorMove if self.escalator_cooldown <= 0.0 => {
                    sounds.play_escalator_move();
                    self.escalator_cooldown = 1.5;
                }
                _ => {}
            }
        }
        self.previous = current;
    }
}

struct App {
    tower: Tower,
    traffic: TrafficSimulation,
    cached_population_by_kind: HashMap<FacilityKind, u32>,
    cached_population: u32,
    cached_star_rating: u8,
    assets: OriginalAssets,
    tool: ToolMode,
    open_menu: Option<BuildMenu>,
    camera_x: f32,
    camera_floor: f32,
    status: String,
    construction: Vec<ConstructionAnimation>,
    paint_drag_last: Option<GridPosition>,
    lobby_drag_stories: u8,
    elevator_drag: Option<ElevatorDrag>,
    elevator_panel: Option<ElevatorPanelState>,
    simulation_sounds: SimulationSoundState,
    parking_state: ParkingState,
    visual_people_x: HashMap<u64, f32>,
    tenant_variants: HashMap<u64, usize>,
    tenant_variant_bags: HashMap<FacilityKind, TenantVariantBag>,
    tenant_names: HashMap<u64, String>,
    person_names: HashMap<u64, String>,
    tenant_finances: HashMap<u64, TenantFinanceState>,
    hotel_guest_stays: HashSet<u64>,
    tenant_panel: Option<TenantPanelState>,
    tenant_panel_resume_speed: Option<SimulationSpeed>,
    person_panel: Option<PersonPanelState>,
    person_panel_resume_speed: Option<SimulationSpeed>,
    people_animation_seconds: f64,
    view_mode: ViewMode,
    speed_menu_open: bool,
    last_running_speed: SimulationSpeed,
    seasonal_event: Option<SeasonalEventState>,
    treasure_popup: Option<TreasurePopup>,
    fire_event: Option<FireEventState>,
    next_fire_minute: u64,
    vip_visit: Option<VipVisitState>,
    last_vip_attempt_day: u32,
    vip_dialog: Option<VipDialogKind>,
    vip_dialog_resume_speed: Option<SimulationSpeed>,
    bomb_event: Option<BombEventState>,
    last_star_rating: u8,
    promotion_dialog_open: bool,
    promotion_resume_speed: Option<SimulationSpeed>,
    promotion_rating: u8,
    finance_ledger: FinanceLedger,
    finance_panel_open: bool,
    finance_resume_speed: Option<SimulationSpeed>,
    automatic_finance_reports: bool,
    ux_scale: UxScale,
    intro_remaining: f32,
    welcome_dialog_open: bool,
    welcome_selection: usize,
    game_menu_open: bool,
    ux_scale_menu_open: bool,
    mode_menu_open: bool,
    save_name_dialog: Option<SaveNameDialog>,
    current_save_name: String,
}

impl App {
    fn new(assets: OriginalAssets) -> Self {
        let tower = fresh_tower();
        let finance_ledger = FinanceLedger::new(tower.cash());
        let cached_star_rating = tower.star_rating_for_population(0);
        let ux_scale = load_ux_scale_preference();
        set_active_ux_scale(ux_scale);
        assets.sounds.play_opening_theme();
        Self {
            tower,
            traffic: TrafficSimulation::new(),
            cached_population_by_kind: HashMap::new(),
            cached_population: 0,
            cached_star_rating,
            assets,
            tool: ToolMode::Floor,
            open_menu: Some(BuildMenu::Structure),
            camera_x: 0.0,
            camera_floor: 1.0,
            status: "Floor tool: click and drag to create empty buildable floor space.".to_owned(),
            construction: Vec::new(),
            paint_drag_last: None,
            lobby_drag_stories: 1,
            elevator_drag: None,
            elevator_panel: None,
            simulation_sounds: SimulationSoundState::default(),
            parking_state: ParkingState::default(),
            visual_people_x: HashMap::new(),
            tenant_variants: HashMap::new(),
            tenant_variant_bags: HashMap::new(),
            tenant_names: HashMap::new(),
            person_names: HashMap::new(),
            tenant_finances: HashMap::new(),
            hotel_guest_stays: HashSet::new(),
            tenant_panel: None,
            tenant_panel_resume_speed: None,
            person_panel: None,
            person_panel_resume_speed: None,
            people_animation_seconds: 0.0,
            view_mode: ViewMode::Edit,
            speed_menu_open: false,
            last_running_speed: SimulationSpeed::Normal,
            seasonal_event: seasonal_event_for_system_date()
                .map(|kind| SeasonalEventState { kind, x: -180.0 }),
            treasure_popup: None,
            fire_event: None,
            next_fire_minute: 11 * 60,
            vip_visit: None,
            last_vip_attempt_day: 0,
            vip_dialog: None,
            vip_dialog_resume_speed: None,
            bomb_event: None,
            last_star_rating: 1,
            promotion_dialog_open: false,
            promotion_resume_speed: None,
            promotion_rating: 1,
            finance_ledger,
            finance_panel_open: false,
            finance_resume_speed: None,
            automatic_finance_reports: true,
            ux_scale,
            intro_remaining: INTRO_TOTAL_SECONDS,
            welcome_dialog_open: false,
            welcome_selection: 0,
            game_menu_open: false,
            ux_scale_menu_open: false,
            mode_menu_open: false,
            save_name_dialog: None,
            current_save_name: DEFAULT_SAVE_NAME.to_owned(),
        }
    }

    fn start_new_tower(&mut self) {
        self.tower = fresh_tower();
        self.traffic = TrafficSimulation::new();
        self.cached_population_by_kind.clear();
        self.cached_population = 0;
        self.cached_star_rating = self.tower.star_rating_for_population(0);
        self.tool = ToolMode::Floor;
        self.open_menu = Some(BuildMenu::Structure);
        self.camera_x = 0.0;
        self.camera_floor = 1.0;
        self.status = "New tower started — use the Floor tool to create buildable space".to_owned();
        self.construction.clear();
        self.paint_drag_last = None;
        self.lobby_drag_stories = 1;
        self.elevator_drag = None;
        self.elevator_panel = None;
        self.simulation_sounds = SimulationSoundState::default();
        self.parking_state = ParkingState::default();
        self.visual_people_x.clear();
        self.tenant_variants.clear();
        self.tenant_variant_bags.clear();
        self.tenant_names.clear();
        self.person_names.clear();
        self.tenant_finances.clear();
        self.hotel_guest_stays.clear();
        self.tenant_panel = None;
        self.tenant_panel_resume_speed = None;
        self.person_panel = None;
        self.person_panel_resume_speed = None;
        self.people_animation_seconds = 0.0;
        self.view_mode = ViewMode::Edit;
        self.speed_menu_open = false;
        self.last_running_speed = SimulationSpeed::Normal;
        self.seasonal_event =
            seasonal_event_for_system_date().map(|kind| SeasonalEventState { kind, x: -180.0 });
        self.treasure_popup = None;
        self.fire_event = None;
        self.next_fire_minute = 11 * 60;
        self.vip_visit = None;
        self.last_vip_attempt_day = 0;
        self.vip_dialog = None;
        self.vip_dialog_resume_speed = None;
        self.bomb_event = None;
        self.last_star_rating = 1;
        self.promotion_dialog_open = false;
        self.promotion_resume_speed = None;
        self.promotion_rating = 1;
        self.finance_ledger = FinanceLedger::new(self.tower.cash());
        self.finance_panel_open = false;
        self.finance_resume_speed = None;
        self.intro_remaining = 0.0;
        self.welcome_dialog_open = false;
        self.welcome_selection = 0;
        self.assets.sounds.stop_opening_theme();
        self.game_menu_open = false;
        self.ux_scale_menu_open = false;
        self.mode_menu_open = false;
        self.save_name_dialog = None;
        self.current_save_name = DEFAULT_SAVE_NAME.to_owned();
    }

    fn begin_save_name_dialog(&mut self) {
        discard_queued_text_input();
        self.save_name_dialog = Some(SaveNameDialog::with_default_name(&self.current_save_name));
        self.status = "Enter a save-game name".to_owned();
    }

    fn close_finance_panel(&mut self) {
        self.finance_panel_open = false;
        if let Some(speed) = self.finance_resume_speed.take() {
            self.tower.clock.speed = speed;
        }
    }

    fn open_tenant_panel(&mut self, facility_id: u64) {
        let Some((kind, variant)) = self
            .tower
            .facilities()
            .iter()
            .find(|facility| facility.id == facility_id && facility.kind.has_tenant_occupancy())
            .map(|facility| {
                (
                    facility.kind,
                    self.tenant_variants.get(&facility_id).copied(),
                )
            })
        else {
            return;
        };
        let name = self
            .tenant_names
            .get(&facility_id)
            .cloned()
            .unwrap_or_else(|| default_tenant_name(kind, variant).to_owned());
        if self.tower.clock.speed != SimulationSpeed::Paused {
            self.tenant_panel_resume_speed = Some(self.tower.clock.speed);
            self.tower.clock.speed = SimulationSpeed::Paused;
        } else {
            self.tenant_panel_resume_speed = None;
        }
        self.open_menu = None;
        self.speed_menu_open = false;
        self.game_menu_open = false;
        self.ux_scale_menu_open = false;
        self.mode_menu_open = false;
        self.tenant_panel = Some(TenantPanelState {
            facility_id,
            renaming: false,
            rename_draft: name.clone(),
            original_name: name,
        });
        self.status = format!("Inspecting {}", kind.spec().name);
    }

    fn close_tenant_panel(&mut self) {
        self.tenant_panel = None;
        if let Some(speed) = self.tenant_panel_resume_speed.take() {
            self.tower.clock.speed = speed;
        }
        self.status = "Selection cursor".to_owned();
    }

    fn commit_tenant_rename(&mut self) {
        let Some(panel) = &mut self.tenant_panel else {
            return;
        };
        let name = panel.rename_draft.trim().to_owned();
        if !name.is_empty() {
            self.tenant_names.insert(panel.facility_id, name.clone());
            panel.rename_draft.clone_from(&name);
            panel.original_name = name;
        }
        panel.renaming = false;
    }

    fn update_tenant_panel(&mut self) {
        let Some(panel) = &mut self.tenant_panel else {
            return;
        };
        if !self
            .tower
            .facilities()
            .iter()
            .any(|facility| facility.id == panel.facility_id)
        {
            self.close_tenant_panel();
            return;
        }

        if panel.renaming {
            if is_key_pressed(KeyCode::Escape) {
                panel.renaming = false;
                panel.rename_draft.clone_from(&panel.original_name);
                return;
            }
            if is_key_pressed(KeyCode::Backspace) {
                panel.rename_draft.pop();
            }
            while let Some(character) = get_char_pressed() {
                if !character.is_control() && panel.rename_draft.chars().count() < 15 {
                    panel.rename_draft.push(character);
                }
            }
            if is_key_pressed(KeyCode::Enter) {
                self.commit_tenant_rename();
                return;
            }
        } else if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Enter) {
            self.close_tenant_panel();
            return;
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            let point = ui_mouse_position();
            if tenant_info_ok_rect().contains(point) {
                if self
                    .tenant_panel
                    .as_ref()
                    .is_some_and(|panel| panel.renaming)
                {
                    self.commit_tenant_rename();
                }
                self.close_tenant_panel();
            } else if tenant_info_rename_rect().contains(point) {
                let facility_id = self
                    .tenant_panel
                    .as_ref()
                    .map(|panel| panel.facility_id)
                    .unwrap_or_default();
                let is_cinema = self.tower.facilities().iter().any(|facility| {
                    facility.id == facility_id && facility.kind == FacilityKind::Cinema
                });
                if is_cinema {
                    if self.traffic.change_cinema_program(facility_id) {
                        self.status = "The cinema is now showing a new movie".to_owned();
                    }
                } else if self
                    .tenant_panel
                    .as_ref()
                    .is_some_and(|panel| panel.renaming)
                {
                    self.commit_tenant_rename();
                } else if let Some(panel) = &mut self.tenant_panel {
                    panel.renaming = true;
                }
            }
        }
    }

    fn open_person_panel(&mut self, person_id: u64) {
        let Some(person) = self
            .traffic
            .people()
            .iter()
            .find(|person| person.id == person_id)
        else {
            return;
        };
        let name = self
            .person_names
            .get(&person_id)
            .cloned()
            .unwrap_or_else(|| person_role(person, &self.tower).to_owned());
        if self.tower.clock.speed != SimulationSpeed::Paused {
            self.person_panel_resume_speed = Some(self.tower.clock.speed);
            self.tower.clock.speed = SimulationSpeed::Paused;
        } else {
            self.person_panel_resume_speed = None;
        }
        self.open_menu = None;
        self.speed_menu_open = false;
        self.game_menu_open = false;
        self.ux_scale_menu_open = false;
        self.mode_menu_open = false;
        self.person_panel = Some(PersonPanelState {
            person_id,
            renaming: false,
            rename_draft: name.clone(),
            original_name: name,
        });
        self.status = format!("Inspecting person #{person_id}");
    }

    fn close_person_panel(&mut self) {
        self.person_panel = None;
        if let Some(speed) = self.person_panel_resume_speed.take() {
            self.tower.clock.speed = speed;
        }
        self.status = "Selection cursor".to_owned();
    }

    fn commit_person_rename(&mut self) {
        let Some(panel) = &mut self.person_panel else {
            return;
        };
        let name = panel.rename_draft.trim().to_owned();
        if !name.is_empty() {
            self.person_names.insert(panel.person_id, name.clone());
            panel.rename_draft.clone_from(&name);
            panel.original_name = name;
        }
        panel.renaming = false;
    }

    fn update_person_panel(&mut self) {
        let Some(panel) = &mut self.person_panel else {
            return;
        };
        if !self
            .traffic
            .people()
            .iter()
            .any(|person| person.id == panel.person_id)
        {
            self.close_person_panel();
            return;
        }

        if panel.renaming {
            if is_key_pressed(KeyCode::Escape) {
                panel.renaming = false;
                panel.rename_draft.clone_from(&panel.original_name);
                return;
            }
            if is_key_pressed(KeyCode::Backspace) {
                panel.rename_draft.pop();
            }
            while let Some(character) = get_char_pressed() {
                if !character.is_control() && panel.rename_draft.chars().count() < 20 {
                    panel.rename_draft.push(character);
                }
            }
            if is_key_pressed(KeyCode::Enter) {
                self.commit_person_rename();
                return;
            }
        } else if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Enter) {
            self.close_person_panel();
            return;
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            let point = ui_mouse_position();
            if person_info_ok_rect().contains(point) {
                if self
                    .person_panel
                    .as_ref()
                    .is_some_and(|panel| panel.renaming)
                {
                    self.commit_person_rename();
                }
                self.close_person_panel();
            } else if person_info_rename_rect().contains(point) {
                if self
                    .person_panel
                    .as_ref()
                    .is_some_and(|panel| panel.renaming)
                {
                    self.commit_person_rename();
                } else if let Some(panel) = &mut self.person_panel {
                    discard_queued_text_input();
                    panel.renaming = true;
                }
            }
        }
    }

    fn update_save_name_dialog(&mut self, dt: f32) {
        let confirming_overwrite = self
            .save_name_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.overwrite_path.is_some());
        if confirming_overwrite {
            let mut confirm = is_key_pressed(KeyCode::Enter);
            let mut decline = is_key_pressed(KeyCode::Escape);
            if is_mouse_button_pressed(MouseButton::Left) {
                let point = ui_mouse_position();
                confirm |= save_dialog_save_rect().contains(point);
                decline |= save_dialog_cancel_rect().contains(point);
            }
            if decline {
                if let Some(dialog) = &mut self.save_name_dialog {
                    dialog.overwrite_path = None;
                    dialog.error = Some("Existing save kept; enter a different name".to_owned());
                    dialog.replace_default_on_input = true;
                }
                self.status = "Save was not overwritten".to_owned();
                return;
            }
            if !confirm {
                return;
            }
            let name = self
                .save_name_dialog
                .as_ref()
                .map(|dialog| dialog.name.clone())
                .unwrap_or_default();
            match self.save_named(&name) {
                Ok(path) => {
                    self.current_save_name = save_default_name_for_path(&path);
                    self.save_name_dialog = None;
                    self.status = format!("Replaced save game at {}", path.display());
                }
                Err(error) => {
                    if let Some(dialog) = &mut self.save_name_dialog {
                        dialog.overwrite_path = None;
                        dialog.error = Some(error);
                    }
                }
            }
            return;
        }

        let mut submit = is_key_pressed(KeyCode::Enter);
        let mut cancel = is_key_pressed(KeyCode::Escape);
        if let Some(dialog) = &mut self.save_name_dialog {
            if is_key_pressed(KeyCode::Backspace) {
                dialog.backspace();
                dialog.start_key_repeat(KeyCode::Backspace, TextEditAction::Backspace);
            }
            while let Some(character) = get_char_pressed() {
                let key = key_for_text_character(character);
                let generated_by_native_repeat =
                    key.is_some_and(|key| dialog.is_repeating_key(key) && !is_key_pressed(key));
                if !generated_by_native_repeat {
                    dialog.push_character(character);
                    if let Some(key) = key.filter(|key| is_key_pressed(*key)) {
                        dialog.start_key_repeat(key, TextEditAction::Insert(character));
                    }
                }
            }
            dialog.update_key_repeat(dt);
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let point = ui_mouse_position();
            submit |= save_dialog_save_rect().contains(point);
            cancel |= save_dialog_cancel_rect().contains(point);
        }
        if cancel {
            self.save_name_dialog = None;
            self.status = "Save cancelled".to_owned();
            return;
        }
        if !submit {
            return;
        }
        let name = self
            .save_name_dialog
            .as_ref()
            .map(|dialog| dialog.name.clone())
            .unwrap_or_default();
        match save_game_path(&name) {
            Ok(path) if path.exists() => {
                if let Some(dialog) = &mut self.save_name_dialog {
                    dialog.overwrite_path = Some(path.clone());
                    dialog.error = None;
                }
                self.status = format!("{} already exists; confirm replacement", path.display());
                return;
            }
            Err(error) => {
                if let Some(dialog) = &mut self.save_name_dialog {
                    dialog.error = Some(error);
                }
                return;
            }
            Ok(_) => {}
        }
        match self.save_named(&name) {
            Ok(path) => {
                self.current_save_name = save_default_name_for_path(&path);
                self.save_name_dialog = None;
                self.status = format!("Saved game to {}", path.display());
            }
            Err(error) => {
                if let Some(dialog) = &mut self.save_name_dialog {
                    dialog.error = Some(error);
                }
            }
        }
    }

    fn save_named(&self, name: &str) -> Result<PathBuf, String> {
        let path = save_game_path(name)?;
        let save = SaveGame {
            format_version: SAVE_FORMAT_VERSION,
            tower: self.tower.clone(),
            traffic: self.traffic.clone(),
            camera_x: self.camera_x,
            camera_floor: self.camera_floor,
            last_running_speed: self.last_running_speed,
            next_fire_minute: self.next_fire_minute,
            finance_ledger: self.finance_ledger.clone(),
            options: SaveOptions {
                muted: self.assets.sounds.is_muted(),
                ux_scale: self.ux_scale,
                automatic_finance_reports: self.automatic_finance_reports,
            },
            legacy_automatic_finance_reports: None,
            tenant_variants: self.tenant_variants.clone(),
            tenant_variant_bags: self.tenant_variant_bags.clone(),
            tenant_names: self.tenant_names.clone(),
            person_names: self.person_names.clone(),
            tenant_finances: self.tenant_finances.clone(),
            hotel_guest_stays: self.hotel_guest_stays.clone(),
            vip_visit: self.vip_visit,
            last_vip_attempt_day: self.last_vip_attempt_day,
            bomb_event: self.bomb_event,
        };
        let bytes = serde_json::to_vec_pretty(&save)
            .map_err(|error| format!("Could not encode the save game: {error}"))?;
        let directory = path
            .parent()
            .ok_or_else(|| "Save path does not have a parent folder".to_owned())?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
        fs::write(&path, bytes)
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
        Ok(path)
    }

    fn load_from_picker(&mut self) {
        match choose_save_game_file() {
            Ok(Some(path)) => match self.load_from_path(&path) {
                Ok(()) => self.status = format!("Loaded game from {}", path.display()),
                Err(error) => self.status = format!("Load failed: {error}"),
            },
            Ok(None) => self.status = "Load cancelled".to_owned(),
            Err(error) => self.status = format!("Could not open file picker: {error}"),
        }
    }

    fn load_from_path(&mut self, path: &Path) -> Result<(), String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let mut save: SaveGame = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid save-game file: {error}"))?;
        if save.format_version != SAVE_FORMAT_VERSION {
            return Err(format!(
                "unsupported save format {} (expected {})",
                save.format_version, SAVE_FORMAT_VERSION
            ));
        }
        save.tower.validate_save_state()?;
        save.traffic.validate_save_state(&save.tower)?;
        if !save.camera_x.is_finite() || !save.camera_floor.is_finite() {
            return Err("save contains an invalid camera position".to_owned());
        }
        // Saves written before OpenTower matched the original game's site
        // dimensions used a 256-unit construction area. Widen them in place;
        // existing floors, tenants, traffic, and the camera remain untouched.
        save.tower
            .expand_site_width(simtower_core::DEFAULT_TOWER_WIDTH);
        let options = save.resolved_options();
        save.traffic.sync_with_tower(&save.tower);
        self.tower = save.tower;
        self.traffic = save.traffic;
        self.camera_x = save
            .camera_x
            .clamp(0.0, f32::from(self.tower.width().saturating_sub(1)));
        self.camera_floor = save.camera_floor.clamp(f32::from(MIN_FLOOR), 100.0);
        self.last_running_speed = if save.last_running_speed == SimulationSpeed::Paused {
            SimulationSpeed::Normal
        } else {
            save.last_running_speed
        };
        self.next_fire_minute = save.next_fire_minute;
        self.finance_ledger = save.finance_ledger;
        self.automatic_finance_reports = options.automatic_finance_reports;
        self.ux_scale = options.ux_scale;
        set_active_ux_scale(options.ux_scale);
        self.assets.sounds.set_muted(options.muted);
        self.finance_ledger.ensure_initialized(self.tower.cash());
        self.tool = ToolMode::Inspect;
        self.view_mode = ViewMode::Edit;
        self.open_menu = None;
        self.speed_menu_open = false;
        self.game_menu_open = false;
        self.ux_scale_menu_open = false;
        self.mode_menu_open = false;
        self.construction.clear();
        self.paint_drag_last = None;
        self.elevator_drag = None;
        self.elevator_panel = None;
        self.finance_panel_open = false;
        self.finance_resume_speed = None;
        self.visual_people_x.clear();
        self.tenant_variants = save.tenant_variants;
        self.tenant_variant_bags = save.tenant_variant_bags;
        self.tenant_names = save.tenant_names;
        self.person_names = save.person_names;
        self.tenant_finances = save.tenant_finances;
        self.hotel_guest_stays = save.hotel_guest_stays;
        self.vip_visit = save.vip_visit;
        self.last_vip_attempt_day = save.last_vip_attempt_day;
        self.vip_dialog = None;
        self.vip_dialog_resume_speed = None;
        self.bomb_event = save.bomb_event;
        self.tenant_panel = None;
        self.tenant_panel_resume_speed = None;
        self.person_panel = None;
        self.person_panel_resume_speed = None;
        self.simulation_sounds = SimulationSoundState::default();
        self.parking_state = ParkingState::default();
        self.treasure_popup = None;
        self.fire_event = None;
        self.promotion_dialog_open = false;
        self.promotion_resume_speed = None;
        self.refresh_tower_metrics();
        self.last_star_rating = self.star_rating();
        self.promotion_rating = self.last_star_rating;
        self.intro_remaining = 0.0;
        self.welcome_dialog_open = false;
        self.welcome_selection = 0;
        self.assets.sounds.stop_opening_theme();
        self.current_save_name = save_default_name_for_path(path);
        self.sync_tenant_variants();
        Ok(())
    }

    fn open_welcome_dialog(&mut self) {
        if self.welcome_dialog_open {
            return;
        }
        self.welcome_dialog_open = true;
        self.welcome_selection = 0;
    }

    fn update_welcome_dialog(&mut self) {
        let point = physical_to_ui(vec2(mouse_position().0, mouse_position().1), self.ux_scale);
        let viewport = vec2(ui_screen_width(), ui_screen_height());
        let tab_pressed = is_key_pressed(KeyCode::Tab);
        if is_key_pressed(KeyCode::Down) || (tab_pressed && !shift_modifier_down()) {
            self.welcome_selection = (self.welcome_selection + 1) % WELCOME_ACTIONS.len();
        }
        if is_key_pressed(KeyCode::Up) || (tab_pressed && shift_modifier_down()) {
            self.welcome_selection =
                (self.welcome_selection + WELCOME_ACTIONS.len() - 1) % WELCOME_ACTIONS.len();
        }
        let clicked_action = is_mouse_button_pressed(MouseButton::Left)
            .then(|| welcome_action_at(viewport, point))
            .flatten();
        let keyboard_action = if is_key_pressed(KeyCode::Enter) {
            Some(WELCOME_ACTIONS[self.welcome_selection].0)
        } else if is_key_pressed(KeyCode::N) {
            Some(WelcomeAction::NewTower)
        } else if is_key_pressed(KeyCode::L) {
            Some(WelcomeAction::LoadSavedTower)
        } else if is_key_pressed(KeyCode::Q) {
            Some(WelcomeAction::ExitProgram)
        } else {
            None
        };
        match clicked_action.or(keyboard_action) {
            Some(WelcomeAction::NewTower) => self.start_new_tower(),
            Some(WelcomeAction::LoadSavedTower) => self.load_from_picker(),
            Some(WelcomeAction::ExitProgram) => {
                self.assets.sounds.stop_opening_theme();
                std::process::exit(0);
            }
            None => {}
        }
    }

    fn update(&mut self) {
        let dt = get_frame_time();
        let save_name_entry_active = self
            .save_name_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.overwrite_path.is_none());
        let tenant_name_entry_active = self
            .tenant_panel
            .as_ref()
            .is_some_and(|panel| panel.renaming);
        let person_name_entry_active = self
            .person_panel
            .as_ref()
            .is_some_and(|panel| panel.renaming);
        if !save_name_entry_active && !tenant_name_entry_active && !person_name_entry_active {
            discard_queued_text_input();
        }
        if self.intro_remaining > 0.0 {
            self.intro_remaining = (self.intro_remaining - dt).max(0.0);
            if is_key_pressed(KeyCode::Escape)
                || is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
            {
                self.intro_remaining = 0.0;
            }
            if self.intro_remaining <= 0.0 {
                self.open_welcome_dialog();
            }
            return;
        }
        if self.welcome_dialog_open {
            self.update_welcome_dialog();
            return;
        }
        if self.save_name_dialog.is_some() {
            self.update_save_name_dialog(dt);
            return;
        }
        if self.tenant_panel.is_some() {
            self.update_tenant_panel();
            return;
        }
        if self.person_panel.is_some() {
            self.update_person_panel();
            return;
        }
        if self.finance_panel_open {
            if is_key_pressed(KeyCode::Escape)
                || (is_mouse_button_pressed(MouseButton::Left)
                    && finance_ok_rect().contains(ui_mouse_position()))
            {
                self.close_finance_panel();
            }
            return;
        }
        if self.promotion_dialog_open {
            let dismiss = is_key_pressed(KeyCode::Escape)
                || is_key_pressed(KeyCode::Enter)
                || (is_mouse_button_pressed(MouseButton::Left)
                    && promotion_ok_rect().contains(ui_mouse_position()));
            if dismiss {
                self.promotion_dialog_open = false;
                if let Some(speed) = self.promotion_resume_speed.take() {
                    self.tower.clock.speed = speed;
                }
            }
            return;
        }
        if self.vip_dialog.is_some() {
            let dismiss = is_key_pressed(KeyCode::Escape)
                || is_key_pressed(KeyCode::Enter)
                || (is_mouse_button_pressed(MouseButton::Left)
                    && vip_dialog_ok_rect().contains(ui_mouse_position()));
            if dismiss {
                self.vip_dialog = None;
                if let Some(speed) = self.vip_dialog_resume_speed.take() {
                    self.tower.clock.speed = speed;
                }
            }
            return;
        }
        if self
            .bomb_event
            .is_some_and(|event| event.response == BombResponse::AwaitingDecision)
        {
            self.update_bomb_response_dialog(dt);
            return;
        }
        if self
            .fire_event
            .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
        {
            self.update_fire_response_dialog(dt);
            return;
        }
        self.finance_ledger.ensure_initialized(self.tower.cash());
        self.finance_ledger
            .reconcile_external_cash(self.tower.cash());
        let previous_absolute_minute = self.tower.clock.absolute_minute();
        let edit_mode_scale = if self.view_mode == ViewMode::Edit {
            1.0
        } else {
            0.0
        };
        let traffic_scale = edit_mode_scale
            * match self.tower.clock.speed {
                SimulationSpeed::Paused => 0.0,
                SimulationSpeed::Normal => 1.0,
                SimulationSpeed::Fast => 2.0,
                SimulationSpeed::Triple => 3.0,
                SimulationSpeed::Quintuple => 5.0,
                SimulationSpeed::Tenfold => 10.0,
            };
        let clock_dt = if self.view_mode == ViewMode::Edit {
            dt
        } else {
            0.0
        };
        let simulation_dt = dt * traffic_scale;
        self.capture_hotel_guest_stays();
        let reachable_lookup = self.traffic.reachable_facility_set().clone();
        let reachable_facilities = reachable_lookup.iter().copied().collect::<Vec<_>>();
        let visitor_counts = self
            .tower
            .facilities()
            .iter()
            .map(|facility| {
                (
                    facility.id,
                    self.economic_patronage(facility, reachable_lookup.contains(&facility.id)),
                )
            })
            .collect::<Vec<_>>();
        let elevator_cars = self
            .traffic
            .elevators()
            .iter()
            .map(|shaft| (shaft.id, shaft.kind, shaft.cars.len() as u8))
            .collect::<Vec<_>>();
        let income_events = self.tower.advance_time_with_connected_economy(
            clock_dt,
            &visitor_counts,
            &elevator_cars,
            Some(&reachable_facilities),
        );
        self.finance_ledger.record(&income_events);
        record_tenant_finances(
            &mut self.tenant_finances,
            self.tower.clock.day,
            &income_events,
        );
        self.finance_ledger.last_observed_cash = self.tower.cash();
        let closed_quarter =
            quarter_closed_between(previous_absolute_minute, self.tower.clock.absolute_minute());
        self.update_hotel_room_service(
            previous_absolute_minute,
            self.tower.clock.absolute_minute(),
        );
        self.traffic.advance_after_sync(&self.tower, simulation_dt);
        self.complete_housekeeping_visits();
        self.capture_hotel_guest_stays();
        self.refresh_tower_metrics();
        self.update_parking_state();
        let tower_status = self
            .tower
            .update_tower_status_for_population(self.cached_population);
        if self.tower.tower_status_awarded() {
            self.cached_star_rating = 6;
        } else if let TowerStatusProgress::WeddingInProgress { .. } = tower_status {
            self.status = "The Cathedral wedding is in progress".to_owned();
        }
        self.update_vip_visit();
        if self.tower.vip_approved() && self.cached_population >= 5_000 {
            self.cached_star_rating = self
                .tower
                .star_rating_for_population(self.cached_population);
        }
        if let Some((year, quarter)) = closed_quarter {
            let vacancies = self.apply_quarterly_evaluation();
            self.finance_ledger.close_quarter(
                year,
                quarter,
                self.tower.cash(),
                self.population_by_kind(),
            );
            if self.automatic_finance_reports {
                if self.tower.clock.speed != SimulationSpeed::Paused {
                    self.last_running_speed = self.tower.clock.speed;
                    self.finance_resume_speed = Some(self.tower.clock.speed);
                    self.tower.clock.speed = SimulationSpeed::Paused;
                }
                self.finance_panel_open = true;
            }
            if vacancies > 0 {
                self.status = format!(
                    "{vacancies} poorly evaluated tenant{} moved out",
                    if vacancies == 1 { "" } else { "s" }
                );
            }
        }
        self.update_bomb_event(simulation_dt);
        self.update_fire_event(simulation_dt);
        let current_rating = self.star_rating();
        if current_rating > self.last_star_rating {
            self.promotion_rating = current_rating;
            self.promotion_dialog_open = true;
            self.promotion_resume_speed = (self.tower.clock.speed != SimulationSpeed::Paused)
                .then_some(self.tower.clock.speed);
            self.tower.clock.speed = SimulationSpeed::Paused;
            self.status = if current_rating >= 6 {
                "Your building has earned TOWER status".to_owned()
            } else {
                format!("Tower promoted to {current_rating} stars")
            };
        }
        self.last_star_rating = current_rating;
        // Visual interpolation uses wall-clock frame time, not accelerated
        // simulation time. At 5x/10x the old code advanced a two-frame walk
        // cycle dozens of times per second and snapped visual positions to
        // every simulation step, which looked like flicker and stutter.
        let visual_dt = visual_animation_delta(dt, self.tower.clock.speed);
        self.people_animation_seconds += f64::from(visual_dt);
        self.simulation_sounds.queue_payments(&income_events);
        let escalator_riders = escalator_rider_count(&self.tower, &self.traffic);
        self.simulation_sounds.update(
            dt,
            &self.traffic,
            &self.assets.sounds,
            self.tower.clock.speed == SimulationSpeed::Paused,
            escalator_riders,
        );
        self.update_people_visuals(visual_dt);
        for animation in &mut self.construction {
            animation.remaining -= dt;
        }
        self.construction
            .retain(|animation| animation.remaining > 0.0);
        if let Some(event) = &mut self.seasonal_event {
            event.x += dt * 42.0;
            if event.x > screen_width() + 180.0 {
                event.x = -180.0;
            }
        }
        if let Some(popup) = &mut self.treasure_popup {
            popup.remaining -= dt;
        }
        if self
            .treasure_popup
            .is_some_and(|popup| popup.remaining <= 0.0)
        {
            self.treasure_popup = None;
        }
        if !income_events.is_empty() {
            let amount: i64 = income_events.iter().map(|event| event.amount).sum();
            self.status = if amount >= 0 {
                format!(
                    "Collected ${amount} in net tenant income from {} account{}",
                    income_events.len(),
                    if income_events.len() == 1 { "" } else { "s" }
                )
            } else {
                format!("Paid ${} in net operating costs", amount.saturating_abs())
            };
        }
        if let Some((year, quarter)) = closed_quarter {
            self.status = if self.automatic_finance_reports {
                format!("Year {year}, Quarter {quarter} financial report")
            } else {
                format!("Year {year}, Quarter {quarter} financial report is available")
            };
            return;
        }
        if self.elevator_panel.is_some() {
            if is_key_pressed(KeyCode::Escape) {
                self.close_elevator_controls();
            } else if is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
            {
                self.handle_elevator_panel_click(ui_mouse_position());
            }
            return;
        }
        self.update_camera();
        self.update_shortcuts();
        if is_mouse_button_pressed(MouseButton::Right) {
            let point = vec2(mouse_position().0, mouse_position().1);
            self.game_menu_open = false;
            self.ux_scale_menu_open = false;
            self.mode_menu_open = false;
            if !self.point_is_ui(point) {
                if self.view_mode == ViewMode::Pricing {
                    self.handle_mode_click(point, -1);
                    return;
                }
                if self.open_elevator_controls(self.grid_position(point)) {
                    return;
                }
                self.tool = ToolMode::Inspect;
                self.open_menu = None;
                self.paint_drag_last = None;
                self.elevator_drag = None;
                self.status = "Selection cursor".to_owned();
            } else {
                self.open_menu = None;
                self.stop_elevator_placement();
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            self.game_menu_open = false;
            self.ux_scale_menu_open = false;
            self.mode_menu_open = false;
            self.open_menu = None;
            if self.clear_active_build_tool() {
                self.status = "Build tool cleared; selection cursor active".to_owned();
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            self.handle_left_click(vec2(mouse_position().0, mouse_position().1));
        } else if is_mouse_button_down(MouseButton::Left) {
            self.continue_paint_drag(vec2(mouse_position().0, mouse_position().1));
        }
        if is_mouse_button_released(MouseButton::Left) {
            self.paint_drag_last = None;
            self.lobby_drag_stories = 1;
            self.elevator_drag = None;
        }
        self.sync_tenant_variants();
    }

    fn sync_tenant_variants(&mut self) {
        let live_tenant_ids = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind.has_tenant_occupancy())
            .map(|facility| facility.id)
            .collect::<HashSet<_>>();
        let live_tenants = self
            .tower
            .facilities()
            .iter()
            .filter_map(|facility| {
                tenant_variant_count(facility.kind).map(|count| (facility.id, facility.kind, count))
            })
            .collect::<Vec<_>>();
        let live_ids = live_tenants
            .iter()
            .map(|(facility_id, _, _)| *facility_id)
            .collect::<HashSet<_>>();
        self.tenant_variants
            .retain(|facility_id, _| live_ids.contains(facility_id));
        self.tenant_names
            .retain(|facility_id, _| live_tenant_ids.contains(facility_id));
        self.tenant_finances
            .retain(|facility_id, _| live_tenant_ids.contains(facility_id));
        self.hotel_guest_stays
            .retain(|facility_id| live_tenant_ids.contains(facility_id));
        for (facility_id, kind, variant_count) in live_tenants {
            if !self.tenant_variants.contains_key(&facility_id) {
                let variant = self
                    .tenant_variant_bags
                    .entry(kind)
                    .or_default()
                    .next(variant_count);
                self.tenant_variants.insert(facility_id, variant);
            }
        }
    }

    fn capture_hotel_guest_stays(&mut self) {
        self.hotel_guest_stays.extend(
            self.tower
                .facilities()
                .iter()
                .filter(|facility| {
                    is_hotel_room(facility.kind)
                        && facility.is_occupied()
                        && (self.traffic.visitors_at(facility.id) > 0
                            || self.traffic.daily_visitors_at(facility.id) > 0)
                })
                .map(|facility| facility.id),
        );
    }

    /// Advances the original hotel's checkout/housekeeping/infestation loop.
    /// Guest departures are staggered from 06:30 through noon. Each of the
    /// six workers in every housekeeping office gets 19 half-hour cleaning
    /// rounds, with the last room begun at 16:30 (114 rooms per office/day).
    fn update_hotel_room_service(&mut self, previous: u64, current: u64) {
        if current <= previous {
            return;
        }
        for absolute_minute in previous + 1..=current {
            let minute = (absolute_minute % (24 * 60)) as u16;
            let day = (absolute_minute / (24 * 60)) as u32 + 1;
            if (6 * 60 + 30..=12 * 60).contains(&minute) {
                self.checkout_hotel_rooms(day, minute);
            }
            if (7 * 60 + 30..=16 * 60 + 30).contains(&minute)
                && (minute - (7 * 60 + 30)).is_multiple_of(30)
            {
                self.run_housekeeping_round();
            }
            if minute == 17 * 60 {
                self.update_hotel_infestations(day);
            }
        }
    }

    fn checkout_hotel_rooms(&mut self, day: u32, minute: u16) {
        let checked_out = self
            .hotel_guest_stays
            .iter()
            .copied()
            .filter(|facility_id| hotel_checkout_minute(*facility_id) == minute)
            .collect::<Vec<_>>();
        for facility_id in checked_out {
            self.hotel_guest_stays.remove(&facility_id);
            if self.tower.mark_hotel_dirty(facility_id, day) {
                self.traffic.evacuate_facility(facility_id);
            }
        }
    }

    fn update_hotel_infestations(&mut self, day: u32) {
        let room_geometry = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| is_hotel_room(facility.kind))
            .map(|facility| {
                (
                    facility.id,
                    facility.position.floor,
                    facility.position.x,
                    facility.end_x(),
                )
            })
            .collect::<Vec<_>>();
        let infestation_sources = self
            .tower
            .facilities()
            .iter()
            .filter_map(|facility| match facility.occupancy {
                simtower_core::FacilityOccupancy::HotelInfested => Some(facility.id),
                simtower_core::FacilityOccupancy::HotelDirty { since_day }
                    if day.saturating_sub(since_day) > 3 =>
                {
                    Some(facility.id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut infest = infestation_sources.clone();
        for source_id in infestation_sources {
            let Some((_, floor, start, end)) = room_geometry
                .iter()
                .find(|(room_id, _, _, _)| *room_id == source_id)
                .copied()
            else {
                continue;
            };
            infest.extend(
                room_geometry
                    .iter()
                    .filter(|(_, other_floor, other_start, other_end)| {
                        *other_floor == floor && (*other_end == start || *other_start == end)
                    })
                    .map(|(room_id, _, _, _)| *room_id),
            );
        }
        infest.sort_unstable();
        infest.dedup();
        let mut newly_infested = 0;
        for facility_id in infest {
            if self.tower.mark_hotel_infested(facility_id) {
                self.traffic.evacuate_facility(facility_id);
                newly_infested += 1;
            }
        }

        if newly_infested > 0 {
            self.status = if newly_infested == 1 {
                "A hotel room has become infested and must be bulldozed".to_owned()
            } else {
                format!("{newly_infested} hotel rooms are infested and must be bulldozed")
            };
        }
    }

    fn run_housekeeping_round(&mut self) {
        let housekeeping_ids = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind == FacilityKind::Housekeeping)
            .map(|facility| facility.id)
            .collect::<Vec<_>>();
        let mut assigned_rooms = self
            .traffic
            .people()
            .iter()
            .filter(|person| housekeeping_ids.contains(&person.home_facility_id))
            .map(|person| person.destination_facility_id)
            .collect::<HashSet<_>>();
        for housekeeping_id in housekeeping_ids {
            // The original assigns at most one worker from a housekeeping
            // office to any one floor. Six workers can therefore cover six
            // floors in parallel, but cannot pile onto a single long floor.
            let mut serviced_floors = HashSet::new();
            let active_workers = self
                .traffic
                .people()
                .iter()
                .filter(|person| person.home_facility_id == housekeeping_id)
                .count();
            for _ in active_workers..6 {
                let Some((housekeeping_floor, housekeeping_x)) = self
                    .tower
                    .facilities()
                    .iter()
                    .find(|facility| facility.id == housekeeping_id)
                    .map(|facility| (facility.position.floor, facility.position.x))
                else {
                    break;
                };
                let mut candidates = self
                    .tower
                    .facilities()
                    .iter()
                    .filter(|facility| facility.is_hotel_dirty())
                    .filter(|facility| !assigned_rooms.contains(&facility.id))
                    .filter(|facility| !serviced_floors.contains(&facility.position.floor))
                    .filter(|facility| {
                        self.traffic.housekeeping_route_available(
                            &self.tower,
                            housekeeping_id,
                            facility.id,
                        )
                    })
                    .map(|facility| {
                        (
                            facility.id,
                            facility.position.floor,
                            (i32::from(facility.position.floor) - i32::from(housekeeping_floor))
                                .unsigned_abs(),
                            facility.position.x.abs_diff(housekeeping_x),
                        )
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by_key(|(facility_id, _, floors, columns)| {
                    (*floors, *columns, *facility_id)
                });
                let Some((facility_id, floor, _, _)) = candidates.first().copied() else {
                    break;
                };
                if self
                    .traffic
                    .dispatch_housekeeper(&self.tower, housekeeping_id, facility_id, 8.0)
                    .is_none()
                {
                    break;
                }
                assigned_rooms.insert(facility_id);
                serviced_floors.insert(floor);
            }
        }
    }

    fn complete_housekeeping_visits(&mut self) {
        let housekeeping_ids = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind == FacilityKind::Housekeeping)
            .map(|facility| facility.id)
            .collect::<HashSet<_>>();
        let cleaned_rooms = self
            .traffic
            .people()
            .iter()
            .filter(|person| {
                housekeeping_ids.contains(&person.home_facility_id)
                    && person.activity == PersonActivity::Visiting
            })
            .map(|person| person.destination_facility_id)
            .collect::<Vec<_>>();
        for room_id in cleaned_rooms {
            self.tower.clean_hotel_room(room_id);
        }
    }

    fn update_fire_event(&mut self, dt: f32) {
        if self.bomb_event.is_some() {
            return;
        }
        if let Some(event) = &mut self.fire_event {
            if event.response == FireResponse::AwaitingDecision {
                return;
            }
            event.elapsed += dt;
            self.traffic.evacuate_facility(event.facility_id);
            if event.elapsed >= event.duration {
                let facility_id = event.facility_id;
                let saves_unit = event.saves_unit;
                let resume_speed = event.resume_speed;
                self.fire_event = None;
                restore_emergency_speed(&mut self.tower.clock, resume_speed);
                if saves_unit {
                    self.status =
                        "The fire is out. The quick response saved the tenant space.".to_owned();
                } else {
                    self.tower.mark_facility_burned(facility_id);
                    self.traffic.evacuate_facility(facility_id);
                    self.status =
                        "The fire is out, but the tenant burned out. Bulldoze and rebuild it."
                            .to_owned();
                }
            }
            return;
        }

        let now = self.tower.clock.absolute_minute();
        if !emergency_events_unlocked(self.star_rating()) {
            // The original tower cannot be asked to handle an emergency before
            // the Security Office is available. Keep moving the first possible
            // event forward so unlocking Security starts a fresh grace period
            // instead of immediately releasing an overdue fire.
            self.next_fire_minute = now + 3 * 24 * 60;
            return;
        }
        if !self
            .tower
            .facilities()
            .iter()
            .any(|facility| facility.kind == FacilityKind::Security)
        {
            // The original manual states that fire and terrorist events begin
            // only after security personnel are present in the building.
            self.next_fire_minute = now + 24 * 60;
            return;
        }
        if now < self.next_fire_minute {
            return;
        }
        self.next_fire_minute = now + 3 * 24 * 60;
        let candidates = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| {
                facility.is_occupied()
                    && !matches!(
                        facility.kind,
                        FacilityKind::Lobby
                            | FacilityKind::Stairs
                            | FacilityKind::Escalator
                            | FacilityKind::Elevator
                            | FacilityKind::ServiceElevator
                            | FacilityKind::ExpressElevator
                            | FacilityKind::Ramp
                    )
            })
            .map(|facility| facility.id)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            // Recheck in one simulated hour instead of silently consuming the
            // event before the tower has an occupied tenant.
            self.next_fire_minute = now + 60;
            return;
        }
        let index = (u64::from(self.tower.clock.day).wrapping_mul(17) as usize) % candidates.len();
        let facility_id = candidates[index];
        if self.tower.clock.day.is_multiple_of(2) {
            self.start_bomb_event(facility_id);
            return;
        }
        self.traffic.evacuate_facility(facility_id);
        self.center_camera_on_facility(facility_id);
        let resume_speed = apply_emergency_speed(&mut self.tower.clock);
        self.speed_menu_open = false;
        self.fire_event = Some(FireEventState {
            facility_id,
            elapsed: 0.0,
            decision_elapsed: 0.0,
            duration: 0.0,
            response: FireResponse::AwaitingDecision,
            saves_unit: false,
            security_start_x: None,
            resume_speed,
        });
        self.status = format!(
            "Fire reported in tenant #{facility_id}; choose emergency response — speed set to 1x"
        );
        self.assets.sounds.play_fire_alert();
    }

    fn start_bomb_event(&mut self, facility_id: u64) {
        let Some(target) = self
            .tower
            .facilities()
            .iter()
            .find(|facility| facility.id == facility_id)
        else {
            return;
        };
        let bomb_floor = target.position.floor;
        let security_floor = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind == FacilityKind::Security)
            .min_by_key(|facility| {
                (simtower_core::floor_ordinal(facility.position.floor)
                    - simtower_core::floor_ordinal(bomb_floor))
                .unsigned_abs()
            })
            .map_or(1, |facility| facility.position.floor);
        let resume_speed = apply_emergency_speed(&mut self.tower.clock);
        self.bomb_event = Some(BombEventState {
            facility_id,
            bomb_floor,
            security_floor,
            elapsed: 0.0,
            decision_elapsed: 0.0,
            response: BombResponse::AwaitingDecision,
            resume_speed,
        });
        self.status =
            "A terrorist claims a bomb is hidden in the tower — speed set to 1x".to_owned();
        self.assets.sounds.play_bomb_threat();
    }

    fn update_bomb_response_dialog(&mut self, dt: f32) {
        if let Some(event) = &mut self.bomb_event {
            event.decision_elapsed =
                (event.decision_elapsed + dt).min(FIRE_RESPONSE_DECISION_SECONDS);
        }
        let mouse = ui_mouse_position();
        let pay = is_key_pressed(KeyCode::P)
            || (is_mouse_button_pressed(MouseButton::Left)
                && bomb_pay_button_rect().contains(mouse));
        let search = is_key_pressed(KeyCode::S)
            || is_key_pressed(KeyCode::Escape)
            || self
                .bomb_event
                .is_some_and(|event| event.decision_elapsed >= FIRE_RESPONSE_DECISION_SECONDS)
            || (is_mouse_button_pressed(MouseButton::Left)
                && bomb_search_button_rect().contains(mouse));
        if pay {
            if !self.tower.try_spend(BOMB_RANSOM_COST) {
                self.assets.play_no_money();
                self.status = format!("The ransom is ${BOMB_RANSOM_COST}; insufficient funds");
                return;
            }
            self.finance_ledger
                .record_emergency_charge(BOMB_RANSOM_COST, self.tower.cash());
            if let Some(event) = self.bomb_event.take() {
                restore_emergency_speed(&mut self.tower.clock, event.resume_speed);
            }
            self.status = "The ransom was paid and the bomb threat withdrawn".to_owned();
            return;
        }
        if search {
            if let Some(event) = &mut self.bomb_event {
                event.response = BombResponse::Searching;
                event.elapsed = 0.0;
            }
            let personnel = bomb_security_personnel_count(&self.tower);
            self.status = format!(
                "{personnel} security personnel are searching separate floors for the bomb"
            );
            self.assets.sounds.play_bomb_armed();
        }
    }

    fn update_bomb_event(&mut self, dt: f32) {
        let Some(mut event) = self.bomb_event else {
            return;
        };
        if event.response == BombResponse::AwaitingDecision {
            return;
        }
        event.elapsed += dt;
        let floors = bomb_search_floor_order(&self.tower, event.security_floor);
        let personnel = bomb_security_personnel_count(&self.tower);
        let searched = bomb_search_completed_floor_count(event.elapsed, personnel, floors.len());
        if floors
            .iter()
            .take(searched)
            .any(|floor| *floor == event.bomb_floor)
        {
            self.bomb_event = None;
            restore_emergency_speed(&mut self.tower.clock, event.resume_speed);
            self.status = format!(
                "Security found and disarmed the bomb on floor {}",
                format_floor(event.bomb_floor)
            );
            self.assets.sounds.play_fire_response();
            return;
        }
        if event.elapsed >= BOMB_FUSE_SECONDS || searched >= floors.len() {
            let bomb_ordinal = simtower_core::floor_ordinal(event.bomb_floor);
            let damaged = self
                .tower
                .facilities()
                .iter()
                .filter(|facility| {
                    let base = simtower_core::floor_ordinal(facility.position.floor);
                    let top = base + i16::try_from(facility.height()).unwrap_or(1) - 1;
                    facility.kind.has_tenant_occupancy()
                        && base <= bomb_ordinal + 2
                        && top >= bomb_ordinal - 2
                })
                .map(|facility| facility.id)
                .collect::<Vec<_>>();
            for facility_id in &damaged {
                self.tower.mark_facility_burned(*facility_id);
                self.traffic.evacuate_facility(*facility_id);
            }
            self.bomb_event = None;
            restore_emergency_speed(&mut self.tower.clock, event.resume_speed);
            self.status = format!(
                "The bomb exploded across five floors; {} tenant spaces must be rebuilt",
                damaged.len()
            );
            self.assets.sounds.play_bomb_explosion();
            return;
        }
        self.bomb_event = Some(event);
    }

    fn update_fire_response_dialog(&mut self, dt: f32) {
        if let Some(event) = &mut self.fire_event {
            event.decision_elapsed =
                (event.decision_elapsed + dt).min(FIRE_RESPONSE_DECISION_SECONDS);
        }
        let mouse = ui_mouse_position();
        let choose_helicopter = is_key_pressed(KeyCode::H)
            || (is_mouse_button_pressed(MouseButton::Left)
                && fire_helicopter_button_rect().contains(mouse));
        let choose_security = is_key_pressed(KeyCode::S)
            || is_key_pressed(KeyCode::Escape)
            || self
                .fire_event
                .is_some_and(|event| event.decision_elapsed >= FIRE_RESPONSE_DECISION_SECONDS)
            || (is_mouse_button_pressed(MouseButton::Left)
                && fire_security_button_rect().contains(mouse));

        if choose_helicopter {
            if !self.tower.try_spend(FIRE_RESCUE_COST) {
                self.assets.play_no_money();
                self.status = format!(
                    "The fire rescue helicopter costs ${FIRE_RESCUE_COST}; insufficient funds"
                );
                return;
            }
            self.finance_ledger
                .record_emergency_charge(FIRE_RESCUE_COST, self.tower.cash());
            if let Some(event) = &mut self.fire_event {
                event.response = FireResponse::Helicopter;
                event.elapsed = 0.0;
                event.duration = FIRE_HELICOPTER_DURATION_SECONDS;
                event.saves_unit = true;
            }
            self.status = "Fire rescue helicopter dispatched; the tenant can be saved".to_owned();
            return;
        }

        if choose_security {
            let Some(event) = self.fire_event else {
                return;
            };
            let Some(target) = self
                .tower
                .facilities()
                .iter()
                .find(|facility| facility.id == event.facility_id)
            else {
                restore_emergency_speed(&mut self.tower.clock, event.resume_speed);
                self.fire_event = None;
                return;
            };
            let Some((security_id, response_seconds)) = nearest_security_fire_response(
                target,
                self.tower
                    .facilities()
                    .iter()
                    .filter(|facility| facility.kind == FacilityKind::Security),
                self.tower.floors(),
                self.tower.width(),
            ) else {
                self.status =
                    "No Security Office can respond; call the fire rescue helicopter".to_owned();
                return;
            };
            let security_start_x = self
                .tower
                .facilities()
                .iter()
                .find(|facility| facility.id == security_id)
                .map(|_| security_team_start_x(target, self.tower.floors(), self.tower.width()))
                .unwrap_or(f32::from(target.position.x));
            if let Some(event) = &mut self.fire_event {
                event.response = FireResponse::Security;
                event.elapsed = 0.0;
                event.duration = response_seconds + 6.0;
                event.saves_unit = response_seconds <= FIRE_QUICK_RESPONSE_SECONDS;
                event.security_start_x = Some(security_start_x);
            }
            self.assets.sounds.play_fire_response();
            self.status = if response_seconds <= FIRE_QUICK_RESPONSE_SECONDS {
                "Security is close enough to save the tenant".to_owned()
            } else {
                "Security is responding, but the fire has already caused severe damage".to_owned()
            };
        }
    }

    fn update_vip_visit(&mut self) {
        if self.tower.vip_approved() {
            self.vip_visit = None;
            return;
        }
        if let Some(mut visit) = self.vip_visit {
            if let Some(person) = self
                .traffic
                .people()
                .iter()
                .find(|person| person.id == visit.person_id)
            {
                visit.worst_mood = visit.worst_mood.max(match person.mood {
                    PersonMood::Calm => 0,
                    PersonMood::Concerned => 1,
                    PersonMood::Angry => 2,
                });
                self.vip_visit = Some(visit);
                return;
            }

            let suite_score = self
                .tower
                .facilities()
                .iter()
                .find(|facility| facility.id == visit.suite_id)
                .map_or(0, |facility| self.tenant_evaluation_score(facility));
            let approved = visit.worst_mood < 2 && suite_score >= 55;
            if approved {
                self.tower.approve_vip_visit();
                self.status = "The VIP rated the tower favorably".to_owned();
                self.open_vip_dialog(VipDialogKind::Approved);
            } else {
                self.status = "The VIP left without recommending the tower".to_owned();
                self.open_vip_dialog(VipDialogKind::Failed);
            }
            self.vip_visit = None;
            return;
        }

        if self.cached_population < 5_000
            || self.tower.clock.day <= self.last_vip_attempt_day
            || !(9 * 60..17 * 60).contains(&self.tower.clock.minute_of_day)
        {
            return;
        }
        let has_required_services = [
            FacilityKind::Medical,
            FacilityKind::Recycling,
            FacilityKind::Parking,
        ]
        .into_iter()
        .all(|kind| {
            self.tower
                .facilities()
                .iter()
                .any(|facility| facility.kind == kind)
        });
        if !has_required_services {
            return;
        }
        let suite_id = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| {
                facility.kind == FacilityKind::HotelSuite
                    && facility.is_occupied()
                    && self.traffic.reachable_facility_set().contains(&facility.id)
            })
            .map(|facility| facility.id)
            .nth(1);
        let Some(suite_id) = suite_id else {
            return;
        };
        let Some(person_id) = self
            .traffic
            .spawn_special_visitor(&self.tower, suite_id, 24.0)
        else {
            return;
        };
        self.last_vip_attempt_day = self.tower.clock.day;
        self.vip_visit = Some(VipVisitState {
            suite_id,
            person_id,
            started_minute: self.tower.clock.absolute_minute(),
            worst_mood: 0,
        });
        self.center_camera_on_facility(suite_id);
        self.status = "A VIP has arrived to evaluate the tower".to_owned();
        self.open_vip_dialog(VipDialogKind::Arrival);
    }

    fn open_vip_dialog(&mut self, kind: VipDialogKind) {
        self.vip_dialog = Some(kind);
        self.vip_dialog_resume_speed =
            (self.tower.clock.speed != SimulationSpeed::Paused).then_some(self.tower.clock.speed);
        self.tower.clock.speed = SimulationSpeed::Paused;
    }

    fn update_camera(&mut self) {
        let speed = 14.0 * get_frame_time();
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            self.camera_x -= speed;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            self.camera_x += speed;
        }
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            self.camera_floor += speed * 0.7;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            self.camera_floor -= speed * 0.7;
        }
        let visible_columns = (screen_width() / CELL_WIDTH).floor();
        let maximum_camera_x = (f32::from(self.tower.width()) - visible_columns).max(0.0);
        self.camera_x = self.camera_x.clamp(0.0, maximum_camera_x);
        self.camera_floor = self.camera_floor.clamp(-8.0, 99.0);
        let mouse = vec2(mouse_position().0, mouse_position().1);
        if !self.point_is_ui(mouse) {
            let wheel = mouse_wheel();
            if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                // macOS may pre-convert Shift+wheel into an X-axis delta,
                // while Windows/Linux normally leave it on the Y axis.
                let horizontal_delta = if wheel.1.abs() >= wheel.0.abs() {
                    wheel.1
                } else {
                    wheel.0
                };
                self.camera_x = (self.camera_x
                    - horizontal_delta * HORIZONTAL_WHEEL_COLUMNS_PER_STEP)
                    .clamp(0.0, maximum_camera_x);
            } else {
                self.camera_floor = (self.camera_floor + wheel.1 * VERTICAL_WHEEL_FLOORS_PER_STEP)
                    .clamp(-8.0, 99.0);
            }
        }
    }

    fn center_camera_on_facility(&mut self, facility_id: u64) {
        let Some(facility) = self
            .tower
            .facilities()
            .iter()
            .find(|facility| facility.id == facility_id)
        else {
            return;
        };
        let visible_columns = (screen_width() / CELL_WIDTH).floor();
        let maximum_camera_x = (f32::from(self.tower.width()) - visible_columns).max(0.0);
        let center_x = f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5;
        self.camera_x = (center_x - visible_columns * 0.5).clamp(0.0, maximum_camera_x);

        let target_y = TOPBAR_HEIGHT + (screen_height() - TOPBAR_HEIGHT) * 0.48;
        let floor_offset = if facility.position.floor > 0 {
            f32::from(facility.position.floor)
        } else {
            f32::from(facility.position.floor + 1)
        };
        self.camera_floor =
            (floor_offset + (target_y - self.view_anchor_y()) / FLOOR_HEIGHT).clamp(-8.0, 99.0);
    }

    fn update_people_visuals(&mut self, dt: f32) {
        let live_people = self
            .traffic
            .people()
            .iter()
            .map(|person| person.id)
            .collect::<HashSet<_>>();
        self.visual_people_x
            .retain(|id, _| live_people.contains(id));
        self.person_names.retain(|id, _| live_people.contains(id));
        let blend = 1.0 - (-10.0 * dt).exp();
        for person in self.traffic.people() {
            let visual_x = self.visual_people_x.entry(person.id).or_insert(person.x);
            *visual_x += (person.x - *visual_x) * blend;
        }
    }

    fn population(&self) -> u32 {
        self.cached_population
    }

    fn population_by_kind(&self) -> HashMap<FacilityKind, u32> {
        self.cached_population_by_kind.clone()
    }

    fn update_parking_state(&mut self) {
        let now = self.tower.clock.absolute_minute();
        if !self.parking_state.layout_dirty && self.parking_state.last_absolute_minute == now {
            return;
        }
        let layout_changed = self.parking_state.layout_dirty;
        let initialized = self.parking_state.last_absolute_minute != u64::MAX;
        let connected = connected_parking_spaces(self.tower.facilities());
        let occupied =
            occupied_parking_spaces(&self.tower, &connected, self.tower.clock.minute_of_day);
        if initialized && !layout_changed {
            let arrivals = occupied
                .iter()
                .filter(|id| !self.parking_state.occupied.contains(id))
                .count();
            let departures = self
                .parking_state
                .occupied
                .iter()
                .filter(|id| !occupied.contains(id))
                .count();
            if arrivals > departures {
                self.assets.sounds.play_parking_arrival();
            } else if departures > 0 {
                self.assets.sounds.play_parking_departure();
            }
        }
        self.parking_state.connected = connected;
        self.parking_state.occupied = occupied;
        self.parking_state.last_absolute_minute = now;
        self.parking_state.layout_dirty = false;
    }

    fn refresh_tower_metrics(&mut self) {
        let reachable = self.traffic.reachable_facility_set();
        let mut totals = HashMap::new();
        for facility in self.tower.facilities().iter().filter(|facility| {
            facility.is_occupied()
                && reachable.contains(&facility.id)
                && (self.traffic.has_received_visitor(facility.id)
                    || has_independent_population(facility.kind))
        }) {
            let population = match facility.kind {
                FacilityKind::Restaurant
                | FacilityKind::FastFood
                | FacilityKind::Shop
                | FacilityKind::Cinema
                | FacilityKind::PartyHall => self
                    .economic_patronage(facility, true)
                    .min(facility.kind.population_capacity()),
                // Hotel population is the number actually checked in right
                // now. Unlike offices and sold condos, the original counter
                // rises and falls as guests arrive and leave.
                FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => {
                    (self.traffic.visitors_at(facility.id) as u32)
                        .min(facility.kind.population_capacity())
                }
                _ => facility.kind.population_capacity(),
            };
            let total = totals.entry(facility.kind).or_insert(0_u32);
            *total = total.saturating_add(population);
        }
        self.cached_population = totals.values().copied().sum();
        self.cached_star_rating = self
            .tower
            .star_rating_for_population(self.cached_population);
        self.cached_population_by_kind = totals;
    }

    fn economic_patronage(&self, facility: &Facility, reachable: bool) -> u32 {
        if !facility.is_occupied()
            || !reachable
            || (!self.traffic.has_received_visitor(facility.id)
                && !has_dedicated_patronage(facility.kind))
        {
            return 0;
        }
        let actual = self
            .traffic
            .daily_visitors_at(facility.id)
            .max(self.traffic.visitors_at(facility.id) as u32);
        actual.max(self.traffic.dedicated_patronage_at(
            facility.id,
            facility.kind,
            is_weekend(self.tower.clock),
        ))
    }

    fn star_rating(&self) -> u8 {
        self.cached_star_rating
    }

    fn update_shortcuts(&mut self) {
        for key in [
            KeyCode::GraveAccent,
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Kp1,
            KeyCode::Kp2,
            KeyCode::Kp3,
            KeyCode::Kp4,
            KeyCode::Kp5,
        ] {
            if is_key_pressed(key)
                && let Some(speed) = speed_keyboard_shortcut(key)
            {
                self.set_simulation_speed(speed);
                break;
            }
        }
        if is_key_pressed(KeyCode::Space) {
            self.toggle_pause();
        }
        if is_key_pressed(KeyCode::Tab) && self.tower.clock.speed != SimulationSpeed::Paused {
            self.tower.clock.speed = SimulationSpeed::Triple;
        }
        if is_key_released(KeyCode::Tab) && self.tower.clock.speed == SimulationSpeed::Triple {
            self.tower.clock.speed = self.last_running_speed;
        }
    }

    fn handle_left_click(&mut self, point: Vec2) {
        let ui_point = physical_to_ui(point, self.ux_scale);
        if game_menu_button_rect().contains(ui_point) {
            self.game_menu_open = !self.game_menu_open;
            if !self.game_menu_open {
                self.ux_scale_menu_open = false;
            }
            self.mode_menu_open = false;
            self.open_menu = None;
            self.speed_menu_open = false;
            return;
        }
        if self.game_menu_open {
            if self.ux_scale_menu_open {
                if let Some(scale) = ux_scale_menu_choice_at(ui_point) {
                    self.ux_scale = scale;
                    set_active_ux_scale(scale);
                    self.game_menu_open = false;
                    self.ux_scale_menu_open = false;
                    self.status = match save_ux_scale_preference(scale) {
                        Ok(()) => format!("Interface scale set to {}", scale.label()),
                        Err(error) => {
                            format!("Interface scale set to {} ({error})", scale.label())
                        }
                    };
                    return;
                }
                if ux_scale_menu_rect().contains(ui_point) {
                    return;
                }
            }
            let action = game_menu_action_at(ui_point);
            if action == Some(GameMenuAction::UxScale) {
                self.ux_scale_menu_open = !self.ux_scale_menu_open;
                return;
            }
            self.game_menu_open = false;
            self.ux_scale_menu_open = false;
            match action {
                Some(GameMenuAction::New) => self.start_new_tower(),
                Some(GameMenuAction::Save) => self.begin_save_name_dialog(),
                Some(GameMenuAction::Load) => self.load_from_picker(),
                Some(GameMenuAction::AutomaticReports) => {
                    self.automatic_finance_reports = !self.automatic_finance_reports;
                    self.status = if self.automatic_finance_reports {
                        "Automatic financial reports enabled"
                    } else {
                        "Automatic financial reports disabled"
                    }
                    .to_owned();
                }
                Some(GameMenuAction::Quit) => std::process::exit(0),
                None => {}
                Some(_) => unreachable!("all game menu actions are handled"),
            }
            return;
        }
        if mode_menu_button_rect().contains(ui_point) {
            self.mode_menu_open = !self.mode_menu_open;
            self.game_menu_open = false;
            self.ux_scale_menu_open = false;
            self.open_menu = None;
            self.speed_menu_open = false;
            return;
        }
        if self.mode_menu_open {
            if let Some(mode) = mode_menu_choice_at(ui_point) {
                self.select_view_mode(mode);
                return;
            }
            self.mode_menu_open = false;
            if mode_menu_rect().contains(ui_point) {
                return;
            }
        }
        if self.speed_menu_open
            && let Some(speed) = speed_menu_choice_at(ui_point)
        {
            self.set_simulation_speed(speed);
            self.speed_menu_open = false;
            return;
        }
        if ui_point.y <= 30.0 && ui_point.x >= ui_screen_width() - 360.0 {
            self.finance_panel_open = true;
            self.open_menu = None;
            self.speed_menu_open = false;
            self.status = "Quarterly financial report".to_owned();
            return;
        }
        if sound_menu_button_rect().contains(ui_point) {
            self.game_menu_open = false;
            self.ux_scale_menu_open = false;
            self.mode_menu_open = false;
            let muted = self.assets.sounds.toggle_muted();
            self.status = if muted {
                "Game sounds muted"
            } else {
                "Game sounds unmuted"
            }
            .to_owned();
            return;
        }
        if self.view_mode != ViewMode::Edit && overview_panel_rect().contains(ui_point) {
            self.handle_overview_click(ui_point);
            return;
        }
        if let Some((menu, index)) = self.menu_entry_at(ui_point) {
            let entry = menu.entries()[index];
            let rating = self.star_rating();
            if !entry.is_available(rating) {
                self.status = format!(
                    "{} unlocks at {} stars (current rating: {})",
                    entry.label,
                    entry.required_stars(),
                    rating
                );
            } else if entry.builds_floor {
                self.select_floor_tool();
            } else if let Some(kind) = entry.kind {
                self.select_build_tool(kind);
            } else {
                self.status = format!(
                    "{}: {}",
                    entry.label,
                    entry.disabled_reason.unwrap_or("Unavailable")
                );
            }
            return;
        }
        if let Some(index) = palette_button_at(ui_point) {
            let button = PALETTE_BUTTONS[index];
            if let PaletteAction::Menu(menu) = button.action
                && self.star_rating() < menu.unlock_stars()
            {
                self.open_menu = None;
                self.status = format!(
                    "{} tools unlock at {} stars",
                    button.label,
                    menu.unlock_stars()
                );
                return;
            }
            match button.action {
                PaletteAction::Speed => {
                    self.open_menu = None;
                    self.speed_menu_open = !self.speed_menu_open;
                    self.status =
                        format!("Simulation speed: {}", speed_label(self.tower.clock.speed));
                }
                PaletteAction::Inspect => {
                    self.view_mode = ViewMode::Edit;
                    self.speed_menu_open = false;
                    self.clear_active_build_tool();
                    self.tool = ToolMode::Inspect;
                    self.open_menu = None;
                    self.status = "Inspect tool: click a placed facility.".to_owned();
                }
                PaletteAction::Demolish => {
                    self.view_mode = ViewMode::Edit;
                    self.speed_menu_open = false;
                    self.clear_active_build_tool();
                    self.tool = ToolMode::Demolish;
                    self.open_menu = None;
                    self.status =
                        "Bulldozer: center the X over an object; repeated clicks remove layers top-down."
                            .to_owned();
                }
                PaletteAction::Menu(menu) => {
                    self.view_mode = ViewMode::Edit;
                    self.speed_menu_open = false;
                    self.clear_active_build_tool();
                    self.open_menu = if self.open_menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    self.status = format!("{} tools", menu.title());
                }
            }
            return;
        }
        if ui_point.y < TOPBAR_HEIGHT || palette_rect().contains(ui_point) {
            return;
        }
        self.speed_menu_open = false;
        self.open_menu = None;

        if self.view_mode != ViewMode::Edit {
            self.handle_mode_click(point, 1);
            return;
        }
        if self.tool != ToolMode::Demolish
            && let Some((kind, x, floor, direction)) = self.elevator_extension_at(point)
        {
            match self.tower.place(kind, GridPosition { x, floor }) {
                Ok(_) => {
                    self.assets.play_construction(kind);
                    self.traffic.sync_with_tower_after_addition(&self.tower);
                    if self.show_secret_discovery() {
                        return;
                    }
                    self.elevator_drag = Some(ElevatorDrag {
                        kind,
                        x,
                        last_floor: floor,
                        direction,
                    });
                    self.status = format!("Extended {} shaft to floor {}", kind.spec().name, floor);
                }
                Err(error) => self.report_placement_error(error),
            }
            return;
        }
        let position = self.grid_position(point);
        if shift_modifier_down() {
            match self.tool {
                ToolMode::Floor => {
                    self.build_floor_across_width(position.floor);
                    return;
                }
                ToolMode::Build(FacilityKind::Lobby)
                    if lobby_stories_for_position(&self.tower, position) == 1 =>
                {
                    self.build_lobby_across_width(position);
                    return;
                }
                ToolMode::Build(kind) if kind.has_tenant_occupancy() => {
                    self.build_tenants_to_right(kind, position);
                    return;
                }
                ToolMode::Build(_) | ToolMode::Inspect | ToolMode::Demolish => {}
            }
        }
        let empty_world = !self.tower.has_floor(position)
            && self.facility_at(position).is_none()
            && match self.tool {
                ToolMode::Floor => self.tower.can_place_floor(position).is_err(),
                ToolMode::Build(_) => true,
                ToolMode::Inspect | ToolMode::Demolish => false,
            };
        if empty_world && self.clear_active_build_tool() {
            self.status = "Build tool cleared; selection cursor active".to_owned();
            return;
        }
        match self.tool {
            ToolMode::Floor => self.begin_floor_drag(position),
            ToolMode::Build(FacilityKind::Lobby) => self.begin_lobby_drag(position),
            ToolMode::Build(kind) => {
                let required = kind.unlock_stars();
                let rating = self.star_rating();
                if rating < required {
                    self.status = format!(
                        "{} requires {} stars (current rating: {})",
                        kind.spec().name,
                        required,
                        rating
                    );
                } else if is_elevator_kind(kind)
                    && self
                        .elevator_facility_at(position)
                        .is_some_and(|facility| facility.kind == kind)
                {
                    self.add_elevator_car_at(kind, position);
                } else {
                    self.traffic.sync_with_tower(&self.tower);
                    let existing_shaft = is_elevator_kind(kind)
                        .then(|| {
                            self.traffic.elevators().iter().find(|shaft| {
                                shaft.kind == kind
                                    && position.x >= shaft.x
                                    && position.x < shaft.x + kind.spec().width
                            })
                        })
                        .flatten()
                        .map(|shaft| (shaft.x, shaft.cars.len()));
                    let placement = GridPosition {
                        x: existing_shaft.map_or(position.x, |(shaft_x, _)| shaft_x),
                        floor: position.floor,
                    };
                    let valid_landing = !is_elevator_kind(kind)
                        || elevator_landing_is_valid(
                            kind,
                            placement.x,
                            placement.floor,
                            self.tower.facilities(),
                        );
                    if is_elevator_kind(kind) && existing_shaft.is_none() && !valid_landing {
                        self.status =
                            "Start the elevator at the bottom landing of the multi-story lobby"
                                .to_owned();
                        return;
                    }
                    let adds_car_to_extension = existing_shaft.is_some()
                        && self.tower.is_elevator_extension(kind, placement)
                        && valid_landing;
                    if adds_car_to_extension
                        && existing_shaft
                            .is_some_and(|(_, cars)| cars >= simtower_core::MAX_ELEVATOR_CARS)
                    {
                        self.status = format!(
                            "{} already has the maximum of {} cars; use its arrows to extend the shaft",
                            kind.spec().name,
                            simtower_core::MAX_ELEVATOR_CARS
                        );
                        return;
                    }
                    let result = if adds_car_to_extension {
                        self.tower
                            .place_elevator_extension_with_car(kind, placement)
                    } else {
                        self.tower.place(kind, placement)
                    };
                    match result {
                        Ok(_) => {
                            if matches!(
                                kind,
                                FacilityKind::Parking | FacilityKind::Ramp | FacilityKind::Lobby
                            ) {
                                self.parking_state.layout_dirty = true;
                            }
                            self.assets.play_construction(kind);
                            self.traffic.sync_with_tower_after_addition(&self.tower);
                            if self.show_secret_discovery() {
                                return;
                            }
                            if adds_car_to_extension
                                && let Some((shaft_x, old_car_count)) = existing_shaft
                            {
                                let added = self
                                    .traffic
                                    .elevator_at(kind, shaft_x)
                                    .map(|shaft| shaft.id)
                                    .is_some_and(|shaft_id| {
                                        self.traffic.add_elevator_car(
                                            &self.tower,
                                            shaft_id,
                                            placement.floor,
                                        )
                                    });
                                self.status = if added {
                                    format!(
                                        "Extended {} shaft and added car {} with floor {} as its default waiting floor",
                                        kind.spec().name,
                                        old_car_count + 1,
                                        placement.floor
                                    )
                                } else {
                                    format!("Extended {} shaft", kind.spec().name)
                                };
                            } else if is_elevator_kind(kind) && existing_shaft.is_some() {
                                self.status = format!(
                                    "Extended {} through the open lobby; this floor is not a stop",
                                    kind.spec().name
                                );
                            } else if is_elevator_kind(kind) {
                                self.status = format!(
                                    "Built {} with 1 car waiting on floor {}",
                                    kind.spec().name,
                                    placement.floor
                                );
                            } else if kind.has_tenant_occupancy() {
                                self.status = format!(
                                    "Built {} on floor {} — vacant; waiting for a tenant to move in",
                                    kind.spec().name,
                                    placement.floor
                                );
                            } else {
                                self.status = format!(
                                    "Built {} on floor {}",
                                    kind.spec().name,
                                    placement.floor
                                );
                            }
                        }
                        Err(error) => self.report_placement_error(error),
                    }
                }
            }
            ToolMode::Inspect => {
                if let Some(person_id) = self.person_at_screen_point(point) {
                    self.open_person_panel(person_id);
                    return;
                }
                let selected = self
                    .facility_at_screen_point(point)
                    .map(|facility| (facility.id, facility.kind, facility.position));
                if selected.is_some_and(|(_, kind, position)| {
                    is_elevator_kind(kind) && self.open_elevator_controls_for(kind, position.x)
                }) {
                    return;
                }
                if let Some((id, kind, _)) = selected
                    && kind.has_tenant_occupancy()
                {
                    self.open_tenant_panel(id);
                    return;
                }
                self.status = selected.map_or_else(
                    || {
                        format!(
                            "Nothing built at floor {}, column {}",
                            position.floor, position.x
                        )
                    },
                    |(id, kind, facility_position)| {
                        let facility = self
                            .tower
                            .facilities()
                            .iter()
                            .find(|facility| facility.id == id)
                            .expect("selected facility still exists");
                        format!(
                            "{} #{} - floor {}, columns {}-{}{}",
                            kind.spec().name,
                            id,
                            facility_position.floor,
                            facility_position.x,
                            facility.end_x() - 1,
                            if kind.has_tenant_occupancy() {
                                if facility.is_occupied() {
                                    " - occupied"
                                } else {
                                    " - vacant; tenant pending"
                                }
                            } else {
                                ""
                            }
                        )
                    },
                );
            }
            ToolMode::Demolish => match self.demolition_target_at(point) {
                Some(DemolitionTarget::Facility(id)) => match self.tower.demolish(id) {
                    Ok(facility) => {
                        if matches!(
                            facility.kind,
                            FacilityKind::Parking | FacilityKind::Ramp | FacilityKind::Lobby
                        ) {
                            self.parking_state.layout_dirty = true;
                        }
                        self.traffic
                            .sync_with_tower_after_removal(&self.tower, &facility);
                        self.assets.play_demolition();
                        self.status = format!("Demolished {} #{}", facility.kind.spec().name, id)
                    }
                    Err(error) => self.status = error.to_string(),
                },
                Some(DemolitionTarget::Floor(position)) => {
                    match self.tower.demolish_floor(position) {
                        Ok(()) => {
                            self.traffic
                                .sync_with_tower_after_floor_removal(&self.tower, position);
                            self.assets.play_demolition();
                            self.status = format!(
                                "Demolished floor {}, column {}",
                                position.floor, position.x
                            )
                        }
                        Err(error) => self.status = error.to_string(),
                    }
                }
                None => self.status = "Nothing under the center of the X.".to_owned(),
            },
        }
    }

    fn report_placement_error(&mut self, error: PlacementError) {
        if placement_error_is_insufficient_funds(&error) {
            self.assets.play_no_money();
        }
        self.status = error.to_string();
    }

    fn begin_floor_drag(&mut self, position: GridPosition) {
        self.paint_drag_last = Some(position);
        match self.tower.place_floor(position) {
            Ok(()) => {
                self.traffic.sync_with_tower_after_addition(&self.tower);
                self.assets.play_floor_construction();
                self.status = format!(
                    "Built empty floor on level {}, column {} — keep dragging to extend",
                    position.floor, position.x
                );
            }
            Err(error) => self.report_placement_error(error),
        }
    }

    fn build_floor_across_width(&mut self, floor: i16) {
        let FillPlan {
            columns,
            limiting_error,
        } = floor_fill_plan(&self.tower, floor);
        if columns.is_empty() {
            self.status = if let Some(error) = limiting_error {
                error.to_string()
            } else {
                format!("No additional floor space can be built on level {floor}")
            };
            return;
        }

        let mut built = 0;
        let mut last_error = None;
        for x in columns {
            match self.tower.place_floor(GridPosition { x, floor }) {
                Ok(()) => built += 1,
                Err(error) => {
                    let out_of_money = placement_error_is_insufficient_funds(&error);
                    last_error = Some(error);
                    if out_of_money {
                        break;
                    }
                }
            }
        }

        self.paint_drag_last = None;
        if built > 0 {
            self.traffic.sync_with_tower_after_addition(&self.tower);
            self.assets.play_floor_construction();
        }
        let final_error = last_error.or(limiting_error);
        if final_error
            .as_ref()
            .is_some_and(placement_error_is_insufficient_funds)
        {
            self.assets.play_no_money();
        }
        self.status = match (built, final_error) {
            (0, Some(error)) => error.to_string(),
            (0, None) => format!("No additional floor space can be built on level {floor}"),
            (count, Some(error)) => format!(
                "Built {count} floor slice{} across level {floor}; {error}",
                if count == 1 { "" } else { "s" }
            ),
            (count, None) => format!(
                "Built {count} floor slice{} across the full buildable length of level {floor}",
                if count == 1 { "" } else { "s" }
            ),
        };
    }

    fn build_lobby_across_width(&mut self, position: GridPosition) {
        if position.x == 0 && position.floor == MIN_FLOOR {
            self.begin_lobby_drag(position);
            return;
        }
        let stories = lobby_stories_for_position(&self.tower, position);
        let plan = lobby_fill_plan(&self.tower, position.floor, stories);
        if plan.columns.is_empty() {
            let mut preview = self.tower.clone();
            match preview.place_lobby(position, stories) {
                Ok(_) => unreachable!("valid lobby fill position must appear in its plan"),
                Err(error) => self.report_placement_error(error),
            }
            return;
        }

        let mut built = 0;
        for x in plan.columns {
            match self.tower.place_lobby(
                GridPosition {
                    x,
                    floor: position.floor,
                },
                stories,
            ) {
                Ok(id) => {
                    self.construction.push(ConstructionAnimation {
                        facility_id: id,
                        remaining: CONSTRUCTION_SECONDS,
                    });
                    built += 1;
                }
                Err(error) => {
                    self.report_placement_error(error);
                    break;
                }
            }
        }
        self.paint_drag_last = None;
        self.lobby_drag_stories = stories;
        if built > 0 {
            self.parking_state.layout_dirty = true;
            self.traffic.sync_with_tower_after_addition(&self.tower);
            self.assets.play_construction(FacilityKind::Lobby);
            self.status = format!(
                "Painted {built} {} slice{} across the full buildable length of floor {}",
                lobby_display_name(position.floor, stories),
                if built == 1 { "" } else { "s" },
                position.floor
            );
        }
        if plan
            .limiting_error
            .as_ref()
            .is_some_and(placement_error_is_insufficient_funds)
        {
            self.assets.play_no_money();
            self.status.push_str("; stopped when funds ran out");
        }
    }

    fn build_tenants_to_right(&mut self, kind: FacilityKind, position: GridPosition) {
        let rating = self.star_rating();
        if rating < kind.unlock_stars() {
            self.status = format!(
                "{} requires {} stars (current rating: {})",
                kind.spec().name,
                kind.unlock_stars(),
                rating
            );
            return;
        }

        let TenantFillPlan {
            positions,
            limiting_error,
        } = tenant_fill_plan(&self.tower, kind, position);
        if positions.is_empty() {
            if let Some(error) = limiting_error {
                self.report_placement_error(error);
            }
            return;
        }

        let mut built = 0;
        for placement in positions {
            match self.tower.place(kind, placement) {
                Ok(_) => built += 1,
                Err(error) => {
                    self.report_placement_error(error);
                    break;
                }
            }
        }
        self.paint_drag_last = None;
        self.traffic.sync_with_tower_after_addition(&self.tower);
        self.sync_tenant_variants();
        if built > 0 {
            self.assets.play_construction(kind);
            self.status = format!(
                "Built {built} {} tenant{} from column {} to the right — vacant; waiting for move-in",
                kind.spec().name,
                if built == 1 { "" } else { "s" },
                position.x
            );
        }
        if limiting_error
            .as_ref()
            .is_some_and(placement_error_is_insufficient_funds)
        {
            self.assets.play_no_money();
            self.status.push_str("; stopped when funds ran out");
        }
        self.show_secret_discovery();
    }

    fn show_secret_discovery(&mut self) -> bool {
        let Some(discovery) = self.tower.take_secret_discovery() else {
            return false;
        };
        self.treasure_popup = Some(TreasurePopup {
            position: discovery.position,
            amount: discovery.amount,
            remaining: 5.0,
        });
        self.status = format!(
            "Ancient buried treasure discovered! Added ${}",
            discovery.amount
        );
        true
    }

    fn begin_lobby_drag(&mut self, position: GridPosition) {
        if position.x == 0 && position.floor == MIN_FLOOR {
            self.paint_drag_last = None;
            if self.tower.activate_starting_cash_cheat() {
                self.status =
                    "Hidden fund activated: starting cash doubled to $4,000,000".to_owned();
            } else {
                self.status =
                    "The hidden starting fund only works before placing a tenant or lobby"
                        .to_owned();
            }
            return;
        }
        self.lobby_drag_stories = lobby_stories_for_position(&self.tower, position);
        self.paint_drag_last = Some(position);
        match self.tower.place_lobby(position, self.lobby_drag_stories) {
            Ok(id) => {
                self.parking_state.layout_dirty = true;
                self.traffic.sync_with_tower_after_addition(&self.tower);
                self.construction.push(ConstructionAnimation {
                    facility_id: id,
                    remaining: CONSTRUCTION_SECONDS,
                });
                self.assets.play_construction(FacilityKind::Lobby);
                self.status = if self.lobby_drag_stories > 1 {
                    format!(
                        "Built {}-story super-lobby slice — keep dragging to extend",
                        self.lobby_drag_stories
                    )
                } else {
                    format!(
                        "Built {} slice on floor {}, column {} — keep dragging to extend",
                        lobby_display_name(position.floor, 1),
                        position.floor,
                        position.x
                    )
                };
            }
            Err(error) => self.report_placement_error(error),
        }
    }

    fn continue_paint_drag(&mut self, point: Vec2) {
        if self.elevator_drag.is_some() {
            self.continue_elevator_drag(point);
            return;
        }
        if !matches!(
            self.tool,
            ToolMode::Floor | ToolMode::Build(FacilityKind::Lobby)
        ) || self.point_is_ui(point)
        {
            return;
        }
        let Some(previous) = self.paint_drag_last else {
            return;
        };
        let current = self.grid_position(point);
        if current.floor != previous.floor || current.x == previous.x {
            return;
        }

        let mut built = 0;
        let mut last_error = None;
        for x in dragged_columns(previous.x, current.x) {
            let position = GridPosition {
                x,
                floor: current.floor,
            };
            match self.tool {
                ToolMode::Floor => match self.tower.place_floor(position) {
                    Ok(()) => built += 1,
                    Err(error) => last_error = Some(error),
                },
                ToolMode::Build(FacilityKind::Lobby) => {
                    match self.tower.place_lobby(position, self.lobby_drag_stories) {
                        Ok(id) => {
                            self.construction.push(ConstructionAnimation {
                                facility_id: id,
                                remaining: CONSTRUCTION_SECONDS,
                            });
                            built += 1;
                        }
                        Err(error) => last_error = Some(error),
                    }
                }
                ToolMode::Build(_) | ToolMode::Inspect | ToolMode::Demolish => {}
            }
        }
        self.paint_drag_last = Some(current);
        if last_error
            .as_ref()
            .is_some_and(placement_error_is_insufficient_funds)
        {
            self.assets.play_no_money();
        }
        if built > 0 {
            if self.tool == ToolMode::Build(FacilityKind::Lobby) {
                self.parking_state.layout_dirty = true;
            }
            self.traffic.sync_with_tower_after_addition(&self.tower);
            self.status = match self.tool {
                ToolMode::Floor => format!(
                    "Extended empty floor by {built} slice{} on level {}",
                    if built == 1 { "" } else { "s" },
                    current.floor
                ),
                ToolMode::Build(FacilityKind::Lobby) => format!(
                    "Painted {built} {} slice{} on floor {}",
                    lobby_display_name(current.floor, self.lobby_drag_stories),
                    if built == 1 { "" } else { "s" },
                    current.floor
                ),
                ToolMode::Build(_) | ToolMode::Inspect | ToolMode::Demolish => unreachable!(),
            };
        } else if let Some(error) = last_error {
            self.status = error.to_string();
        }
    }

    fn continue_elevator_drag(&mut self, point: Vec2) {
        let Some(mut drag) = self.elevator_drag else {
            return;
        };
        let target = self.grid_position(point).floor;
        let current_ordinal = simtower_core::floor_ordinal(drag.last_floor);
        let target_ordinal = simtower_core::floor_ordinal(target);
        if (drag.direction > 0 && target_ordinal <= current_ordinal)
            || (drag.direction < 0 && target_ordinal >= current_ordinal)
        {
            return;
        }

        let mut built = 0;
        let mut ordinal = current_ordinal + i16::from(drag.direction);
        while (drag.direction > 0 && ordinal <= target_ordinal)
            || (drag.direction < 0 && ordinal >= target_ordinal)
        {
            let floor = simtower_core::floor_from_ordinal(ordinal);
            match self
                .tower
                .place(drag.kind, GridPosition { x: drag.x, floor })
            {
                Ok(_) => {
                    drag.last_floor = floor;
                    built += 1;
                }
                Err(error) => {
                    self.report_placement_error(error);
                    break;
                }
            }
            ordinal += i16::from(drag.direction);
        }
        if built > 0 {
            self.assets.play_construction(drag.kind);
            self.traffic.sync_with_tower_after_addition(&self.tower);
            self.status = format!(
                "Extended {} shaft by {built} floor{}",
                drag.kind.spec().name,
                if built == 1 { "" } else { "s" }
            );
        }
        self.elevator_drag = Some(drag);
    }

    fn add_elevator_car_at(&mut self, kind: FacilityKind, position: GridPosition) {
        self.traffic.sync_with_tower(&self.tower);
        let Some(shaft_x) = self
            .elevator_facility_at(position)
            .filter(|facility| facility.kind == kind)
            .map(|facility| facility.position.x)
        else {
            self.status = "No matching elevator shaft here".to_owned();
            return;
        };
        let Some((shaft_id, car_count)) = self
            .traffic
            .elevator_at(kind, shaft_x)
            .map(|shaft| (shaft.id, shaft.cars.len()))
        else {
            self.status = "No matching elevator shaft here".to_owned();
            return;
        };
        if car_count >= simtower_core::MAX_ELEVATOR_CARS {
            self.status = format!(
                "{} already has the maximum of {} cars",
                kind.spec().name,
                simtower_core::MAX_ELEVATOR_CARS
            );
            return;
        }
        if !self
            .traffic
            .elevator_can_stop_at(&self.tower, shaft_id, position.floor)
        {
            self.status =
                "Elevator cars cannot wait in the middle of a multi-story lobby".to_owned();
            return;
        }
        if let Err(error) = self.tower.purchase_elevator_car(kind) {
            self.report_placement_error(error);
            return;
        }
        if self
            .traffic
            .add_elevator_car(&self.tower, shaft_id, position.floor)
        {
            self.assets.play_construction(kind);
            self.status = format!(
                "Added car {} to {} with floor {} as its default waiting floor",
                car_count + 1,
                kind.spec().name,
                position.floor
            );
        }
    }

    fn select_floor_tool(&mut self) {
        self.view_mode = ViewMode::Edit;
        self.mode_menu_open = false;
        self.speed_menu_open = false;
        self.clear_active_build_tool();
        self.tool = ToolMode::Floor;
        self.open_menu = None;
        self.paint_drag_last = None;
        self.status = format!(
            "Selected Floor - ${FLOOR_CONSTRUCTION_COST} per slice; drag to create empty space"
        );
    }

    fn select_build_tool(&mut self, kind: FacilityKind) {
        self.view_mode = ViewMode::Edit;
        self.mode_menu_open = false;
        self.speed_menu_open = false;
        self.clear_active_build_tool();
        let required = kind.unlock_stars();
        let rating = self.star_rating();
        if rating < required {
            self.status = format!(
                "{} unlocks at {} stars (current rating: {})",
                kind.spec().name,
                required,
                rating
            );
            return;
        }
        self.tool = ToolMode::Build(kind);
        self.open_menu = None;
        self.paint_drag_last = None;
        self.status = if kind == FacilityKind::Lobby {
            format!(
                "Selected Lobby - {}; {}",
                build_price_label(kind),
                LOBBY_HEIGHT_HELP
            )
        } else if is_elevator_kind(kind) {
            format!(
                "Selected {} - {}; endpoint arrows extend the shaft without another shaft charge",
                kind.spec().name,
                build_price_label(kind)
            )
        } else {
            format!(
                "Selected {} - {}",
                kind.spec().name,
                build_price_label(kind)
            )
        };
    }

    fn toggle_pause(&mut self) {
        self.tower.clock.speed = if self.tower.clock.speed == SimulationSpeed::Paused {
            self.last_running_speed
        } else {
            self.last_running_speed = self.tower.clock.speed;
            SimulationSpeed::Paused
        };
        self.status = if self.tower.clock.speed == SimulationSpeed::Paused {
            "Simulation paused"
        } else {
            "Simulation running"
        }
        .to_owned();
    }

    fn set_simulation_speed(&mut self, speed: SimulationSpeed) {
        if speed != SimulationSpeed::Paused {
            self.last_running_speed = speed;
        }
        self.tower.clock.speed = speed;
        self.status = format!("Simulation speed set to {}", speed_label(speed));
    }

    fn handle_mode_click(&mut self, point: Vec2, price_step: i8) {
        let position = self.grid_position(point);
        let Some((id, kind, occupied, burned, current_price)) =
            self.facility_at_screen_point(point).map(|facility| {
                (
                    facility.id,
                    facility.kind,
                    facility.is_occupied(),
                    facility.is_burned(),
                    facility.price_level,
                )
            })
        else {
            self.status = format!(
                "No facility at floor {}, column {}",
                position.floor, position.x
            );
            return;
        };
        if burned {
            self.status = format!(
                "{} #{} was destroyed by fire — bulldoze and rebuild it",
                kind.spec().name,
                id
            );
            return;
        }
        match self.view_mode {
            ViewMode::Edit => {}
            ViewMode::Evaluation => {
                let visitors = self.traffic.visitors_at(id);
                let score = self
                    .tower
                    .facilities()
                    .iter()
                    .find(|facility| facility.id == id)
                    .map_or(0, |facility| self.tenant_evaluation_score(facility));
                let evaluation = match score {
                    70..=100 => "excellent",
                    40..=69 => "average",
                    _ => "needs attention",
                };
                self.status = format!(
                    "{} #{}: {} ({score}/100) — {} people currently inside",
                    kind.spec().name,
                    id,
                    evaluation,
                    visitors
                );
            }
            ViewMode::Pricing => {
                if !kind.has_tenant_occupancy() {
                    self.status = format!(
                        "{} does not have an adjustable tenant price",
                        kind.spec().name
                    );
                    return;
                }
                let price = stepped_price_level(current_price, price_step);
                if control_modifier_down() {
                    let changed = set_price_for_tenant_kind(&mut self.tower, kind, price);
                    self.status = format!(
                        "Set all {} {} tenants to {}",
                        changed,
                        kind.spec().name,
                        price_level_label(price)
                    );
                } else {
                    self.tower.set_price_level(id, price);
                    self.status = format!(
                        "{} #{} price set to {}",
                        kind.spec().name,
                        id,
                        price_level_label(price)
                    );
                }
            }
            ViewMode::Hotel => {
                if is_hotel_room(kind) {
                    self.status = format!(
                        "{} #{}: {}, clean",
                        kind.spec().name,
                        id,
                        if occupied { "occupied" } else { "vacant" }
                    );
                } else {
                    self.status = "Hotel mode only reports hotel rooms".to_owned();
                }
            }
        }
    }

    fn grid_position(&self, point: Vec2) -> GridPosition {
        GridPosition {
            x: (point.x / CELL_WIDTH + self.camera_x).floor().max(0.0) as u16,
            floor: world_floor_at_y(self.ground_y(), point.y),
        }
    }

    fn facility_at(&self, position: GridPosition) -> Option<&Facility> {
        self.tower.facilities().iter().find(|facility| {
            position.floor >= facility.position.floor
                && position.floor < facility.position.floor + facility.height() as i16
                && position.x >= facility.position.x
                && position.x < facility.end_x()
        })
    }

    fn facility_at_screen_point(&self, point: Vec2) -> Option<&Facility> {
        // Match the renderer in reverse: vertical transport is in front of
        // tenants, but only an opaque pixel is allowed to intercept a click.
        for vertical in [true, false] {
            if let Some(facility) = self.tower.facilities().iter().rev().find(|facility| {
                is_vertical_transport(facility.kind) == vertical
                    && self.facility_pixel_contains(facility, point)
            }) {
                return Some(facility);
            }
        }
        None
    }

    fn person_at_screen_point(&self, point: Vec2) -> Option<u64> {
        let mut queued_per_stop = HashMap::<(u64, i16), usize>::new();
        let queue_ranks = self
            .traffic
            .people()
            .iter()
            .filter_map(|person| {
                let PersonActivity::WaitingForElevator { shaft_id } = person.activity else {
                    return None;
                };
                let rank = queued_per_stop
                    .entry((shaft_id, person.current_floor))
                    .or_default();
                let result = (person.id, *rank);
                *rank += 1;
                Some(result)
            })
            .collect::<HashMap<_, _>>();

        self.traffic.people().iter().rev().find_map(|person| {
            if matches!(
                person.activity,
                PersonActivity::Visiting
                    | PersonActivity::RidingElevator { .. }
                    | PersonActivity::UsingStairs
            ) {
                return None;
            }
            let visual_x = self
                .visual_people_x
                .get(&person.id)
                .copied()
                .unwrap_or(person.x);
            let x = match person.activity {
                PersonActivity::WaitingForElevator { shaft_id } => self
                    .elevator_queue_draw_x(shaft_id, *queue_ranks.get(&person.id).unwrap_or(&0))
                    .unwrap_or((visual_x - self.camera_x) * CELL_WIDTH - 4.0),
                _ => (visual_x - self.camera_x) * CELL_WIDTH - 4.0,
            };
            let exterior_ground = person.current_floor == 1
                && !self.person_has_floor_below(person.x, person.current_floor);
            let y = if exterior_ground {
                self.ground_y() - PERSON_SPRITE_HEIGHT
            } else {
                interior_person_top_y(self.ground_y(), person.floor_position)
            };
            Rect::new(x - 2.0, y, 12.0, PERSON_SPRITE_HEIGHT)
                .contains(point)
                .then_some(person.id)
        })
    }

    fn facility_pixel_contains(&self, facility: &Facility, point: Vec2) -> bool {
        if !self.facility_screen_rect(facility).contains(point) {
            return false;
        }
        if self
            .construction
            .iter()
            .any(|animation| animation.facility_id == facility.id)
            || is_elevator_kind(facility.kind)
        {
            return true;
        }

        let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
        let base_y = floor_top_y(self.ground_y(), facility.position.floor);
        let width = f32::from(facility.kind.spec().width) * CELL_WIDTH;
        if facility.kind == FacilityKind::Lobby {
            let story = ((base_y - point.y) / FLOOR_HEIGHT).floor().max(0.0) as usize;
            if story >= usize::from(facility.stories) {
                return false;
            }
            let mut run_start = facility.position.x;
            while run_start > 0 && self.lobby_at(run_start - 1, facility.position.floor) {
                run_start -= 1;
            }
            let (sprite, source_base) = self.assets.facilities.lobby_layer(
                story,
                facility.stories,
                facility.position.floor,
                self.star_rating(),
            );
            let source_x = source_base
                + (f32::from(facility.position.x - run_start) * CELL_WIDTH)
                    .rem_euclid(LOBBY_BODY_WIDTH);
            return sprite_pixel_is_opaque(
                sprite,
                Rect::new(source_x, 0.0, width, FLOOR_HEIGHT),
                Rect::new(x, base_y - story as f32 * FLOOR_HEIGHT, width, FLOOR_HEIGHT),
                point,
            );
        }

        let (sprite, mut source) = self.assets.facility_sprite_for(
            facility,
            self.facility_crowd(facility),
            self.tenant_variants.get(&facility.id).copied(),
            self.tower.clock.minute_of_day,
            self.facility_visual_load(facility),
        );
        if matches!(
            facility.kind,
            FacilityKind::Stairs | FacilityKind::Escalator
        ) {
            source = self.stair_sprite_source(facility, sprite);
            let stories = vertical_walkway_story_span(facility, self.tower.facilities());
            return sprite_pixel_is_opaque(
                sprite,
                source,
                Rect::new(
                    x,
                    base_y - (f32::from(stories) - 1.0) * FLOOR_HEIGHT,
                    width,
                    FLOOR_HEIGHT * f32::from(stories),
                ),
                point,
            );
        }
        let floors = f32::from(facility.kind.spec().height);
        let structural_draw_y = base_y - (floors - 1.0) * FLOOR_HEIGHT;
        let draw_height = if floors > 1.0 || sprite.source.h > ROOM_HEIGHT {
            floors * FLOOR_HEIGHT
        } else {
            ROOM_HEIGHT
        };
        let draw_y = if draw_height == ROOM_HEIGHT {
            tenant_room_top_y(self.ground_y(), facility.position.floor)
        } else {
            structural_draw_y
        };
        sprite_pixel_is_opaque(
            sprite,
            source,
            Rect::new(x, draw_y, width, draw_height),
            point,
        )
    }

    fn stair_sprite_source(&self, facility: &Facility, sprite: &Sprite) -> Rect {
        let base_ordinal = simtower_core::floor_ordinal(facility.position.floor);
        let stories = vertical_walkway_story_span(facility, self.tower.facilities());
        let upper_ordinal = base_ordinal + stories.saturating_sub(1) as i16;
        let center_x = f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5;
        let riders = self.traffic.people().iter().filter(|person| {
            person.activity == PersonActivity::UsingStairs
                && (person.x - center_x).abs() < 0.1
                && person.floor_position >= f32::from(base_ordinal) - 0.01
                && person.floor_position <= f32::from(upper_ordinal) + 0.01
        });
        let frame = if facility.kind == FacilityKind::Escalator {
            riders.min_by_key(|person| person.id).map_or(0, |person| {
                escalator_frame_for_position(
                    facility.position.floor,
                    person.current_floor,
                    person.floor_position,
                    stories.saturating_sub(1),
                )
            })
        } else {
            riders.count().min(6)
        };
        Rect::new(
            frame as f32 * f32::from(facility.kind.spec().width) * CELL_WIDTH,
            0.0,
            f32::from(facility.kind.spec().width) * CELL_WIDTH,
            sprite.source.h,
        )
    }

    fn demolition_target_at(&self, point: Vec2) -> Option<DemolitionTarget> {
        // This mirrors the world draw order in reverse. Vertical transport is
        // painted last, so a shaft/stair/escalator is removed before a tenant
        // behind it; the next click reaches the tenant and the next the floor.
        if let Some(facility) = self.facility_at_screen_point(point) {
            return Some(DemolitionTarget::Facility(facility.id));
        }
        let position = self.grid_position(point);
        self.tower
            .has_floor(position)
            .then_some(DemolitionTarget::Floor(position))
    }

    fn facility_screen_rect(&self, facility: &Facility) -> Rect {
        let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
        let base_y = floor_top_y(self.ground_y(), facility.position.floor);
        let stories = facility.height();
        let full_floor_art = self
            .construction
            .iter()
            .any(|animation| animation.facility_id == facility.id)
            || facility.kind == FacilityKind::Lobby
            || is_vertical_transport(facility.kind)
            || stories > 1;
        let height = if full_floor_art {
            f32::from(stories) * FLOOR_HEIGHT
        } else {
            ROOM_HEIGHT
        };
        let y = if full_floor_art {
            base_y - (f32::from(stories) - 1.0) * FLOOR_HEIGHT
        } else {
            tenant_room_top_y(self.ground_y(), facility.position.floor)
        };
        Rect::new(
            x,
            y,
            f32::from(facility.kind.spec().width) * CELL_WIDTH,
            height,
        )
    }

    fn elevator_facility_at(&self, position: GridPosition) -> Option<&Facility> {
        self.tower.facilities().iter().find(|facility| {
            matches!(
                facility.kind,
                FacilityKind::Elevator
                    | FacilityKind::ServiceElevator
                    | FacilityKind::ExpressElevator
            ) && facility.position.floor == position.floor
                && position.x >= facility.position.x
                && position.x < facility.end_x()
        })
    }

    fn open_elevator_controls(&mut self, position: GridPosition) -> bool {
        let Some((kind, x)) = self
            .elevator_facility_at(position)
            .map(|elevator| (elevator.kind, elevator.position.x))
        else {
            return false;
        };
        self.open_elevator_controls_for(kind, x)
    }

    fn open_elevator_controls_for(&mut self, kind: FacilityKind, x: u16) -> bool {
        self.traffic.sync_with_tower(&self.tower);
        let Some(shaft) = self.traffic.elevator_at(kind, x) else {
            return false;
        };
        let weekend = is_weekend(self.tower.clock);
        self.elevator_panel = Some(ElevatorPanelState {
            shaft_id: shaft.id,
            weekend,
            period: simtower_core::schedule_period(self.tower.clock.minute_of_day),
            simulate: false,
        });
        self.tool = ToolMode::Inspect;
        self.open_menu = None;
        self.elevator_drag = None;
        self.status = format!("Elevator controls - shaft #{}", shaft.id);
        true
    }

    fn close_elevator_controls(&mut self) {
        self.elevator_panel = None;
        self.tool = ToolMode::Inspect;
        self.open_menu = None;
        self.elevator_drag = None;
        self.status = "Closed elevator controls; elevator placement stopped".to_owned();
    }

    fn stop_elevator_placement(&mut self) {
        if matches!(self.tool, ToolMode::Build(kind) if is_elevator_kind(kind)) {
            self.tool = ToolMode::Inspect;
            self.elevator_drag = None;
            self.status = "Elevator placement stopped".to_owned();
        }
    }

    fn clear_active_build_tool(&mut self) -> bool {
        if matches!(self.tool, ToolMode::Floor | ToolMode::Build(_)) {
            self.tool = ToolMode::Inspect;
            self.paint_drag_last = None;
            self.lobby_drag_stories = 1;
            self.elevator_drag = None;
            true
        } else {
            false
        }
    }

    fn select_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.mode_menu_open = false;
        self.open_menu = None;
        self.speed_menu_open = false;
        self.status = match mode {
            ViewMode::Edit => "Edit mode — construction and simulation running",
            ViewMode::Evaluation => {
                "Evaluation mode — blue is good, yellow is average, red needs attention"
            }
            ViewMode::Pricing => {
                "Rent mode — left raises, right lowers; hold Control to change every tenant of that type"
            }
            ViewMode::Hotel => "Hotel mode — hotel room occupancy and condition overview",
        }
        .to_owned();
    }

    fn elevator_extension_at(&self, point: Vec2) -> Option<(FacilityKind, u16, i16, i8)> {
        for shaft in self.traffic.elevators() {
            let top = *shaft.served_floors.last()?;
            let bottom = *shaft.served_floors.first()?;
            let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
            let width = f32::from(shaft.kind.spec().width) * CELL_WIDTH;
            let upper_extension = floor_above(top);
            let lower_extension = floor_below(bottom);
            let top_y = floor_top_y(self.ground_y(), upper_extension);
            let bottom_y = floor_top_y(self.ground_y(), lower_extension);
            if Rect::new(x, top_y, width, FLOOR_HEIGHT).contains(point) {
                return Some((shaft.kind, shaft.x, upper_extension, 1));
            }
            if Rect::new(x, bottom_y, width, FLOOR_HEIGHT).contains(point) {
                return Some((shaft.kind, shaft.x, lower_extension, -1));
            }
        }
        None
    }

    fn view_anchor_y(&self) -> f32 {
        TOPBAR_HEIGHT + (screen_height() - TOPBAR_HEIGHT) * 0.7
    }
    fn ground_y(&self) -> f32 {
        world_ground_y(self.view_anchor_y(), self.camera_floor)
    }

    fn menu_entry_at(&self, point: Vec2) -> Option<(BuildMenu, usize)> {
        let menu = self.open_menu?;
        menu.entries()
            .iter()
            .enumerate()
            .find(|(index, _)| submenu_entry_rect(*index).contains(point))
            .map(|(index, _)| (menu, index))
    }

    fn point_is_ui(&self, point: Vec2) -> bool {
        let point = physical_to_ui(point, self.ux_scale);
        self.save_name_dialog.is_some()
            || self.tenant_panel.is_some()
            || self.person_panel.is_some()
            || self.elevator_panel.is_some()
            || self.promotion_dialog_open
            || self.vip_dialog.is_some()
            || self
                .bomb_event
                .is_some_and(|event| event.response == BombResponse::AwaitingDecision)
            || self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            || point.y < TOPBAR_HEIGHT
            || palette_rect().contains(point)
            || (self.view_mode != ViewMode::Edit && overview_panel_rect().contains(point))
            || (self.game_menu_open && game_menu_rect().contains(point))
            || (self.game_menu_open
                && self.ux_scale_menu_open
                && ux_scale_menu_rect().contains(point))
            || (self.mode_menu_open && mode_menu_rect().contains(point))
            || (self.speed_menu_open && speed_menu_rect().contains(point))
            || self
                .open_menu
                .is_some_and(|menu| submenu_rect(menu.entries().len()).contains(point))
    }

    fn draw(&self) {
        let mouse = vec2(mouse_position().0, mouse_position().1);
        let demolition_cursor = self.tool == ToolMode::Demolish
            && self.view_mode == ViewMode::Edit
            && self.elevator_panel.is_none()
            && self.tenant_panel.is_none()
            && self.person_panel.is_none()
            && !self.finance_panel_open
            && !self.promotion_dialog_open
            && !self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            && self.intro_remaining <= 0.0
            && !self.welcome_dialog_open
            && !self.point_is_ui(mouse);
        let inspect_cursor = self.tool == ToolMode::Inspect
            && self.view_mode == ViewMode::Edit
            && self.elevator_panel.is_none()
            && self.tenant_panel.is_none()
            && self.person_panel.is_none()
            && !self.finance_panel_open
            && !self.promotion_dialog_open
            && !self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            && self.intro_remaining <= 0.0
            && !self.welcome_dialog_open
            && !self.point_is_ui(mouse);
        let resize_handle = (!self.promotion_dialog_open
            && !self.welcome_dialog_open
            && self.intro_remaining <= 0.0
            && !demolition_cursor
            && !self.point_is_ui(mouse))
        .then(|| self.elevator_extension_at(mouse))
        .flatten();
        window::show_mouse(resize_handle.is_none() && !demolition_cursor && !inspect_cursor);
        self.draw_world_background();
        self.draw_world();
        self.draw_world_lighting();
        begin_ui_camera(self.ux_scale);
        self.draw_topbar();
        self.draw_palette();
        if self.view_mode != ViewMode::Edit {
            self.draw_tower_overview();
        }
        if let Some(menu) = self.open_menu {
            self.draw_submenu(menu);
        }
        if self.speed_menu_open {
            self.draw_speed_menu();
        }
        if self.game_menu_open {
            self.draw_game_menu();
            if self.ux_scale_menu_open {
                self.draw_ux_scale_menu();
            }
        }
        if self.mode_menu_open {
            self.draw_mode_menu();
        }
        if self.elevator_panel.is_some() {
            self.draw_elevator_panel();
        }
        if self.finance_panel_open {
            self.draw_finance_panel();
        }
        if self.tenant_panel.is_some() {
            self.draw_tenant_panel();
        }
        if self.person_panel.is_some() {
            self.draw_person_panel();
        }
        self.draw_ui_tooltip(physical_to_ui(mouse, self.ux_scale));
        if self.save_name_dialog.is_some() {
            self.draw_save_name_dialog();
        }
        if self.promotion_dialog_open {
            self.draw_promotion_dialog();
        }
        if self.vip_dialog.is_some() {
            self.draw_vip_dialog();
        }
        if self
            .bomb_event
            .is_some_and(|event| event.response == BombResponse::AwaitingDecision)
        {
            self.draw_bomb_response_dialog();
        }
        if self
            .fire_event
            .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
        {
            self.draw_fire_response_dialog();
        }
        UI_CAMERA_ACTIVE.store(false, Ordering::Relaxed);
        set_default_camera();
        if demolition_cursor {
            self.draw_demolition_cursor(mouse);
        } else if let Some((_, _, _, direction)) = resize_handle {
            self.draw_elevator_resize_cursor(mouse, direction);
        } else if inspect_cursor {
            self.draw_magnifier_cursor(mouse);
        }
        if self.intro_remaining > 0.0 || self.welcome_dialog_open {
            self.draw_intro();
        }
    }

    fn draw_intro(&self) {
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), BLACK);
        let viewport = vec2(screen_width(), screen_height());
        let (scale, origin) = title_screen_transform(viewport);
        if self.welcome_dialog_open {
            draw_texture_ex(
                &self.assets.intro_store,
                origin.x,
                origin.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(
                        TITLE_SCREEN_WIDTH * scale,
                        TITLE_SCREEN_HEIGHT * scale,
                    )),
                    ..Default::default()
                },
            );
            begin_ui_camera(self.ux_scale);
            self.draw_welcome_dialog(vec2(ui_screen_width(), ui_screen_height()));
            UI_CAMERA_ACTIVE.store(false, Ordering::Relaxed);
            set_default_camera();
        } else if self.intro_remaining > INTRO_TITLE_SECONDS {
            let size = vec2(236.0, 198.0) * scale;
            draw_texture_ex(
                &self.assets.intro_maxis,
                (screen_width() - size.x) * 0.5,
                (screen_height() - size.y) * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        } else {
            let size = vec2(334.0, 270.0) * scale;
            draw_texture_ex(
                &self.assets.intro_title,
                (screen_width() - size.x) * 0.5,
                (screen_height() - size.y) * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_world_background(&self) {
        // Bitmap 352 contains one complete original sky gradient: its first
        // 264 rows run from the dark-blue zenith to the pale horizon, followed
        // by the dirt strip. The zenith color continues above that artwork;
        // repeating or mirroring the entire gradient creates false alternating
        // light/dark bands as the tower grows.
        clear_background(Color::from_rgba(74, 180, 255, 255));
        let ground_y = self.ground_y();
        let horizon_top = ground_y - SKY_HORIZON;
        let first_x = sky_tile_origin_x(self.camera_x);
        let horizontal_tiles = (screen_width() / SKY_TILE_WIDTH).ceil() as i32 + 3;
        for column in 0..horizontal_tiles {
            draw_texture_ex(
                &self.assets.sky,
                first_x + column as f32 * SKY_TILE_WIDTH,
                horizon_top,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(SKY_TILE_WIDTH, SKY_HORIZON)),
                    source: Some(Rect::new(0.0, 0.0, SKY_TILE_WIDTH, SKY_HORIZON)),
                    ..Default::default()
                },
            );
        }
        self.draw_distant_city_and_clouds(ground_y);
        if weather_is_raining(self.tower.clock.day) {
            // Rain belongs to the outdoor background. The full-height tiling
            // remains visible around the building while tower art stays crisp
            // and unobscured in the foreground.
            self.draw_rain(ground_y);
        }
        self.draw_seasonal_event();
        self.draw_ground_dirt(ground_y);
        draw_rectangle(
            0.0,
            TOPBAR_HEIGHT,
            screen_width(),
            screen_height() - TOPBAR_HEIGHT,
            Color::from_rgba(20, 28, 42, 20),
        );
    }

    fn draw_world_lighting(&self) {
        let lighting = world_lighting(self.tower.clock.minute_of_day);
        let height = screen_height() - TOPBAR_HEIGHT;
        if lighting.warm_alpha > 0 {
            draw_rectangle(
                0.0,
                TOPBAR_HEIGHT,
                screen_width(),
                height,
                Color::from_rgba(194, 88, 34, lighting.warm_alpha),
            );
        }
        if lighting.night_alpha > 0 {
            draw_rectangle(
                0.0,
                TOPBAR_HEIGHT,
                screen_width(),
                height,
                Color::from_rgba(12, 18, 68, lighting.night_alpha),
            );
        }
    }

    fn draw_distant_city_and_clouds(&self, ground_y: f32) {
        let city_y = ground_y - 55.0;
        for (x, mirrored) in
            city_tile_layout(screen_width(), self.camera_x, self.assets.city.width())
        {
            draw_texture_ex(
                &self.assets.city,
                x,
                city_y,
                Color::from_rgba(255, 255, 255, 190),
                DrawTextureParams {
                    dest_size: Some(vec2(self.assets.city.width(), 55.0)),
                    flip_x: mirrored,
                    ..Default::default()
                },
            );
        }

        for (index, cloud) in self.assets.clouds.iter().enumerate() {
            let width = cloud.width();
            let parallax = 0.05 + index as f32 * 0.025;
            let span = screen_width() + width + 340.0;
            let clock_motion = self.tower.clock.absolute_minute() as f32 * (0.12 + parallax);
            let cloud_x = (index as f32 * 317.0 + clock_motion
                - self.camera_x * CELL_WIDTH * parallax)
                .rem_euclid(span)
                - width;
            let cloud_y = TOPBAR_HEIGHT + 38.0 + index as f32 * 47.0;
            draw_texture_ex(
                cloud,
                cloud_x,
                cloud_y,
                Color::from_rgba(255, 255, 255, 150),
                DrawTextureParams::default(),
            );
        }
    }

    fn draw_rain(&self, ground_y: f32) {
        let frame = rain_palette_index(self.tower.clock.minute_of_day);
        let texture = &self.assets.rain[frame];
        let top = TOPBAR_HEIGHT;
        let bottom = ground_y.min(screen_height());
        if bottom <= top {
            return;
        }
        for origin in rain_tile_layout(
            screen_width(),
            top,
            bottom,
            self.camera_x,
            texture.width(),
            texture.height(),
        ) {
            draw_texture_ex(
                texture,
                origin.x,
                origin.y,
                Color::from_rgba(255, 255, 255, 205),
                DrawTextureParams {
                    dest_size: Some(vec2(texture.width(), texture.height())),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_welcome_dialog(&self, viewport: Vec2) {
        let dialog = welcome_dialog_rect(viewport);
        let mouse = ui_mouse_position();
        let (scale, _) = title_screen_transform(viewport);
        draw_title_screen_panel(dialog, scale);
        for (index, (_, label)) in WELCOME_ACTIONS.iter().enumerate() {
            let button = welcome_button_rect(viewport, index);
            let pressed = button.contains(mouse) && is_mouse_button_down(MouseButton::Left);
            draw_title_screen_button(button, scale, pressed, index == self.welcome_selection);
            draw_centered_text(
                label,
                button.x + button.w * 0.5,
                button.y + 19.0 * scale,
                (16.0 * scale).round().max(9.0) as u16,
                BLACK,
            );
        }
    }

    fn draw_ground_dirt(&self, ground_y: f32) {
        const DIRT_TILE_WIDTH: f32 = 32.0;
        const DIRT_DEPTH: f32 = 360.0;
        if ground_y >= screen_height() {
            return;
        }
        let source_y = (TOPBAR_HEIGHT - ground_y).clamp(0.0, DIRT_DEPTH);
        let y = ground_y.max(TOPBAR_HEIGHT);
        let visible_depth = (DIRT_DEPTH - source_y).min(screen_height() - y);
        if y + visible_depth < screen_height() {
            draw_rectangle(
                0.0,
                y + visible_depth,
                screen_width(),
                screen_height() - y - visible_depth,
                Color::from_rgba(51, 25, 0, 255),
            );
        }
        let first_x = -(self.camera_x * CELL_WIDTH).rem_euclid(DIRT_TILE_WIDTH);
        let columns = (screen_width() / DIRT_TILE_WIDTH).ceil() as i32 + 1;
        if visible_depth <= 0.0 {
            return;
        }
        for column in 0..columns {
            draw_texture_ex(
                &self.assets.ground,
                first_x + column as f32 * DIRT_TILE_WIDTH,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(DIRT_TILE_WIDTH, visible_depth)),
                    source: Some(Rect::new(0.0, source_y, DIRT_TILE_WIDTH, visible_depth)),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_seasonal_event(&self) {
        let Some(event) = self.seasonal_event else {
            return;
        };
        let y = TOPBAR_HEIGHT + 54.0;
        match event.kind {
            SeasonalEventKind::Santa => draw_texture_ex(
                &self.assets.santa,
                event.x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(140.0, 48.0)),
                    ..Default::default()
                },
            ),
            SeasonalEventKind::Witch => {
                // The PC asset set contains Santa but no witch bitmap. Keep
                // the October event at the original 8-bit scale and palette.
                let x = event.x;
                draw_line(x, y + 26.0, x + 72.0, y + 29.0, 3.0, BROWN);
                draw_triangle(
                    vec2(x + 35.0, y),
                    vec2(x + 22.0, y + 16.0),
                    vec2(x + 48.0, y + 16.0),
                    Color::from_rgba(38, 18, 55, 255),
                );
                draw_rectangle(x + 19.0, y + 14.0, 34.0, 4.0, BLACK);
                draw_circle(x + 35.0, y + 21.0, 6.0, Color::from_rgba(92, 143, 64, 255));
                draw_triangle(
                    vec2(x + 35.0, y + 25.0),
                    vec2(x + 23.0, y + 39.0),
                    vec2(x + 50.0, y + 39.0),
                    Color::from_rgba(34, 19, 48, 255),
                );
                for offset in [0.0, 5.0, 10.0, 15.0] {
                    draw_line(
                        x + 69.0,
                        y + 28.0,
                        x + 82.0,
                        y + 21.0 + offset,
                        2.0,
                        Color::from_rgba(111, 71, 29, 255),
                    );
                }
            }
        }
    }

    fn draw_treasure_popup(&self) {
        let Some(popup) = self.treasure_popup else {
            return;
        };
        let x = (f32::from(popup.position.x) - self.camera_x) * CELL_WIDTH - 38.0;
        let y = floor_top_y(self.ground_y(), popup.position.floor) - 86.0;
        draw_texture_ex(
            &self.assets.treasure,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(84.0, 80.0)),
                ..Default::default()
            },
        );
        let label = format!("+${}", popup.amount);
        draw_rectangle(
            x - 4.0,
            y - 20.0,
            96.0,
            18.0,
            Color::from_rgba(20, 20, 20, 220),
        );
        draw_text(&label, x + 2.0, y - 6.0, 16.0, YELLOW);
    }

    fn draw_world(&self) {
        let visible = ((screen_height() - TOPBAR_HEIGHT) / FLOOR_HEIGHT).ceil() as i16 + 3;
        let first = self.camera_floor.floor() as i16 - visible / 3;
        for floor in first..=first + visible {
            let y = self.view_anchor_y() - (f32::from(floor) - self.camera_floor) * FLOOR_HEIGHT;
            if floor != 0 && y > TOPBAR_HEIGHT + 12.0 && y < screen_height() {
                let label = floor.to_string();
                let label_top = if floor > 0 { y + 2.0 } else { y - 13.0 };
                let label_width = measure_text(&label, None, 10, 1.0).width.ceil();
                draw_rectangle(
                    0.0,
                    label_top,
                    label_width + 7.0,
                    12.0,
                    Color::from_rgba(18, 28, 38, 72),
                );
                draw_text(
                    &label,
                    3.0,
                    label_top + 9.0,
                    10.0,
                    Color::from_rgba(220, 229, 234, 160),
                );
            }
        }
        // Keep the untextured room fills and textured utility strips in
        // separate passes. Alternating them once per 8-pixel floor slice
        // forced Macroquad to break the GPU batch thousands of times in a
        // wide tower, which made scrolling and even the cursor stutter.
        for position in self
            .tower
            .floors()
            .iter()
            .filter(|position| self.floor_cell_is_visible(**position))
        {
            self.draw_empty_floor_room(*position);
        }
        for position in self
            .tower
            .floors()
            .iter()
            .filter(|position| self.floor_cell_is_visible(**position))
        {
            self.draw_empty_floor_strip(*position);
        }
        self.draw_floor_end_stairs();
        for facility in self.tower.facilities().iter().filter(|facility| {
            !is_vertical_transport(facility.kind) && self.facility_is_visible(facility)
        }) {
            self.draw_facility(facility);
        }
        // Lobby facades and the ground entrance awnings belong to the lobby layer.
        // Draw them before vertical transport so stairs always remain in the
        // foreground when the two systems occupy the same floor space.
        for facility in self.tower.facilities() {
            if facility.kind == FacilityKind::Lobby && self.facility_is_visible(facility) {
                self.draw_lobby_entrances(facility);
            }
        }
        for facility in self.tower.facilities().iter().filter(|facility| {
            is_vertical_transport(facility.kind) && self.facility_is_visible(facility)
        }) {
            self.draw_facility(facility);
        }
        self.draw_elevator_endcaps();
        self.draw_elevator_cars();
        // Hidden shafts retain a crisp outer border. Draw it after cars so a
        // car at either edge cannot cover the shaft silhouette.
        self.draw_disabled_elevator_outlines();
        self.draw_roof_crane();
        self.draw_people();
        self.draw_fire_response();
        self.draw_bomb_search();
        self.draw_treasure_popup();
        self.draw_mode_overlays();
        let mouse = vec2(mouse_position().0, mouse_position().1);
        if self.view_mode == ViewMode::Edit && !self.point_is_ui(mouse) {
            self.draw_hover(mouse);
        }
    }

    fn draw_roof_crane(&self) {
        let Some(anchor) = roof_crane_anchor(self.tower.floors()) else {
            return;
        };
        let x = (f32::from(anchor.x) - self.camera_x) * CELL_WIDTH;
        let roof_y = floor_top_y(self.ground_y(), anchor.floor);
        draw_texture_ex(
            &self.assets.roof_crane,
            x,
            roof_y - 36.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(36.0, 36.0)),
                ..Default::default()
            },
        );
    }

    fn draw_fire_response(&self) {
        let Some(event) = self.fire_event else {
            return;
        };
        let Some(target) = self
            .tower
            .facilities()
            .iter()
            .find(|facility| facility.id == event.facility_id)
        else {
            return;
        };
        match event.response {
            FireResponse::Helicopter => {
                let target_x = (f32::from(target.position.x) - self.camera_x) * CELL_WIDTH;
                let target_width = f32::from(target.kind.spec().width) * CELL_WIDTH;
                let destination_x = target_x + target_width * 0.5 - 48.0;
                let approach = (event.elapsed / 5.0).clamp(0.0, 1.0);
                let helicopter_x = -110.0 + (destination_x + 110.0) * approach;
                let helicopter_y = floor_top_y(self.ground_y(), target.position.floor) - 43.0;
                draw_texture_ex(
                    &self.assets.fire_helicopter,
                    helicopter_x,
                    helicopter_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(96.0, 36.0)),
                        ..Default::default()
                    },
                );
            }
            FireResponse::Security => self.draw_security_response_team(event, target),
            FireResponse::AwaitingDecision => {}
        }
    }

    fn draw_bomb_search(&self) {
        let Some(event) = self
            .bomb_event
            .filter(|event| event.response == BombResponse::Searching)
        else {
            return;
        };
        let floors = bomb_search_floor_order(&self.tower, event.security_floor);
        let personnel = bomb_security_personnel_count(&self.tower);
        let first_active =
            bomb_search_completed_floor_count(event.elapsed, personnel, floors.len());
        if floors.get(first_active).is_none() {
            return;
        }
        let phase = (event.elapsed / BOMB_SEARCH_SECONDS_PER_FLOOR).fract();
        for (guard, floor) in floors
            .iter()
            .skip(first_active)
            .take(personnel)
            .copied()
            .enumerate()
        {
            let mut cells = self
                .tower
                .floors()
                .iter()
                .filter(|position| position.floor == floor)
                .map(|position| position.x);
            let Some(first) = cells.next() else {
                continue;
            };
            let (left, right) = cells.fold((first, first), |(left, right), x| {
                (left.min(x), right.max(x))
            });
            let world_x = f32::from(right + 1)
                - phase * f32::from(right.saturating_sub(left).saturating_add(1));
            let x = (world_x - self.camera_x) * CELL_WIDTH;
            let y = floor_top_y(self.ground_y(), floor) + FLOOR_HEIGHT - PERSON_SPRITE_HEIGHT;
            self.draw_security_team_frame(
                80.0 + ((event.elapsed * 6.0 + guard as f32) as usize % 2) as f32 * 16.0,
                x,
                y,
            );
        }
    }

    fn draw_security_response_team(&self, event: FireEventState, target: &Facility) {
        let floor_extent = floor_extent_for(target.position.floor, self.tower.floors())
            .unwrap_or((target.position.x, target.end_x()));
        let floor_left = f32::from(floor_extent.0);
        let floor_right = f32::from(floor_extent.1);
        let start_x = event
            .security_start_x
            .unwrap_or((floor_right - 2.0).max(floor_left));
        // Original guards arrive through the right emergency stair and sweep
        // left. Their action pose is inside the burning tenant, never beyond
        // the structural edge where it would appear to float in open sky.
        let destination_x = (f32::from(target.end_x()) - 2.0)
            .max(f32::from(target.position.x))
            .clamp(floor_left, (floor_right - 2.0).max(floor_left));
        let travel_seconds = (event.duration - 6.0).max(0.1);
        let progress = (event.elapsed / travel_seconds).clamp(0.0, 1.0);
        let team_world_x = (start_x + (destination_x - start_x) * progress)
            .clamp(floor_left, (floor_right - 2.0).max(floor_left));
        let screen_x = (team_world_x - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), target.position.floor);
        let arrived = progress >= 1.0;
        let source_x = emergency_guard_source_x(arrived, team_world_x);
        self.draw_security_team_frame(source_x, screen_x, y);
    }

    fn draw_fire_response_dialog(&self) {
        let panel = fire_response_dialog_rect();
        draw_classic_panel(panel);
        draw_rectangle(
            panel.x + 4.0,
            panel.y + 4.0,
            panel.w - 8.0,
            panel.h - 8.0,
            CLASSIC_FACE,
        );
        draw_texture_ex(
            &self.assets.fire_alert,
            panel.x + 18.0,
            panel.y + 26.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(76.0, 60.0)),
                ..Default::default()
            },
        );
        draw_texture_ex(
            &self.assets.fire_dispatch,
            panel.x + 10.0,
            panel.y + 92.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(92.0, 50.0)),
                ..Default::default()
            },
        );
        draw_text("Fire!", panel.x + 116.0, panel.y + 34.0, 24.0, BLACK);
        draw_text(
            "Call the fire rescue helicopter?",
            panel.x + 116.0,
            panel.y + 66.0,
            18.0,
            BLACK,
        );
        draw_text(
            "A helicopter guarantees the unit is saved.",
            panel.x + 116.0,
            panel.y + 91.0,
            15.0,
            BLACK,
        );
        draw_text(
            "Security is free, but a distant response may be too late.",
            panel.x + 116.0,
            panel.y + 113.0,
            15.0,
            BLACK,
        );
        let remaining = self.fire_event.map_or(0, |event| {
            fire_response_timeout_remaining(event.decision_elapsed)
        });
        draw_text(
            format!("Security dispatches automatically in {remaining} seconds."),
            panel.x + 116.0,
            panel.y + 137.0,
            15.0,
            BLACK,
        );

        let helicopter = fire_helicopter_button_rect();
        draw_classic_button(helicopter, false);
        draw_centered_text(
            "Call Rescue  $300,000",
            helicopter.x + helicopter.w * 0.5,
            helicopter.y + 21.0,
            15,
            BLACK,
        );
        let security = fire_security_button_rect();
        draw_classic_button(security, false);
        draw_centered_text(
            "Use Security",
            security.x + security.w * 0.5,
            security.y + 21.0,
            15,
            BLACK,
        );
    }

    fn draw_bomb_response_dialog(&self) {
        let rect = bomb_response_dialog_rect();
        draw_classic_panel(rect);
        draw_rectangle(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 23.0, TITLE_BLUE);
        draw_text("Bomb Threat", rect.x + 10.0, rect.y + 19.0, 14.0, WHITE);
        draw_texture_ex(
            &self.assets.terrorist_portrait,
            rect.x + 20.0,
            rect.y + 42.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(84.0, 84.0)),
                ..Default::default()
            },
        );
        draw_text(
            "A terrorist says a bomb is hidden in your tower.",
            rect.x + 122.0,
            rect.y + 59.0,
            16.0,
            BLACK,
        );
        draw_text(
            "Pay the ransom, or order Security to search every floor.",
            rect.x + 122.0,
            rect.y + 87.0,
            14.0,
            BLACK,
        );
        draw_text(
            "A missed bomb can destroy five floors.",
            rect.x + 122.0,
            rect.y + 111.0,
            14.0,
            BLACK,
        );
        let remaining = self.bomb_event.map_or(0, |event| {
            fire_response_timeout_remaining(event.decision_elapsed)
        });
        draw_text(
            format!("Security dispatches automatically in {remaining} seconds."),
            rect.x + 122.0,
            rect.y + 135.0,
            14.0,
            BLACK,
        );
        let pay = bomb_pay_button_rect();
        let search = bomb_search_button_rect();
        draw_classic_button(pay, false);
        draw_classic_button(search, false);
        draw_centered_text("Pay $300,000", pay.x + pay.w * 0.5, pay.y + 21.0, 14, BLACK);
        draw_centered_text(
            "Search with Security",
            search.x + search.w * 0.5,
            search.y + 21.0,
            14,
            BLACK,
        );
    }

    fn draw_promotion_dialog(&self) {
        let rect = promotion_dialog_rect();
        draw_classic_panel(rect);
        // The award bitmap's background is RGB #cccccc. Use that exact gray
        // inside the dialog so its rectangular canvas disappears into the UI.
        draw_rectangle(
            rect.x + 2.0,
            rect.y + 2.0,
            rect.w - 4.0,
            rect.h - 4.0,
            PROMOTION_FACE,
        );
        draw_rectangle_lines(
            rect.x + 3.0,
            rect.y + 3.0,
            rect.w - 6.0,
            rect.h - 6.0,
            1.0,
            CLASSIC_SHADOW,
        );
        draw_promotion_award(Rect::new(rect.x + 17.0, rect.y + 20.0, 110.0, 110.0));
        draw_text(
            "Congratulations!",
            rect.x + 139.0,
            rect.y + 51.0,
            19.0,
            BLACK,
        );
        draw_text(
            "Your tower has been given a",
            rect.x + 139.0,
            rect.y + 79.0,
            16.0,
            BLACK,
        );
        draw_text(
            promotion_rating_label(self.promotion_rating),
            rect.x + 139.0,
            rect.y + 105.0,
            16.0,
            BLACK,
        );
        let ok = promotion_ok_rect();
        draw_classic_button(ok, false);
        let label = measure_text("OK", None, 15, 1.0);
        draw_text(
            "OK",
            (ok.x + (ok.w - label.width) * 0.5).round(),
            (ok.y + (ok.h + label.height) * 0.5 - 2.0).round(),
            15.0,
            BLACK,
        );
    }

    fn draw_vip_dialog(&self) {
        let Some(kind) = self.vip_dialog else {
            return;
        };
        let rect = vip_dialog_rect();
        draw_classic_panel(rect);
        draw_rectangle(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 23.0, TITLE_BLUE);
        draw_text("VIP Evaluation", rect.x + 10.0, rect.y + 19.0, 14.0, WHITE);
        draw_texture_ex(
            &self.assets.vip_arrival,
            rect.x + 20.0,
            rect.y + 42.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(112.0, 82.0)),
                ..Default::default()
            },
        );
        let (heading, first, second) = match kind {
            VipDialogKind::Arrival => (
                "A VIP has arrived!",
                "The guest will travel to a hotel suite",
                "and judge access, waiting stress, and services.",
            ),
            VipDialogKind::Approved => (
                "A favorable review!",
                "The VIP enjoyed the visit.",
                "Your tower may now qualify for four stars.",
            ),
            VipDialogKind::Failed => (
                "The VIP was dissatisfied.",
                "Improve transportation and tenant evaluation.",
                "Another evaluator will visit on a later day.",
            ),
        };
        draw_text(heading, rect.x + 150.0, rect.y + 55.0, 18.0, BLACK);
        draw_text(first, rect.x + 150.0, rect.y + 82.0, 14.0, BLACK);
        draw_text(second, rect.x + 150.0, rect.y + 104.0, 14.0, BLACK);
        let ok = vip_dialog_ok_rect();
        draw_classic_button(ok, false);
        draw_centered_text("OK", ok.x + ok.w * 0.5, ok.y + 19.0, 14, BLACK);
    }

    fn draw_tenant_panel(&self) {
        let Some(panel_state) = &self.tenant_panel else {
            return;
        };
        let Some(facility) = self
            .tower
            .facilities()
            .iter()
            .find(|facility| facility.id == panel_state.facility_id)
        else {
            return;
        };
        let panel = tenant_info_rect();
        draw_classic_panel(panel);
        draw_rectangle_lines(
            panel.x + 5.0,
            panel.y + 5.0,
            panel.w - 10.0,
            panel.h - 10.0,
            1.0,
            CLASSIC_SHADOW,
        );

        let name_field = Rect::new(panel.x + 14.0, panel.y + 13.0, panel.w - 28.0, 25.0);
        draw_recessed_field(name_field);
        let stored_name = self
            .tenant_names
            .get(&facility.id)
            .map(String::as_str)
            .unwrap_or_else(|| {
                default_tenant_name(
                    facility.kind,
                    self.tenant_variants.get(&facility.id).copied(),
                )
            });
        let title = if panel_state.renaming {
            format!("{}|", panel_state.rename_draft)
        } else {
            format!(
                "{stored_name}, Floor {}",
                format_floor(facility.position.floor)
            )
        };
        draw_fitted_text(
            &title,
            name_field.x + 7.0,
            name_field.y + 18.0,
            name_field.w - 14.0,
            17,
            BLACK,
        );
        if panel_state.renaming {
            draw_rectangle_lines(
                name_field.x + 2.0,
                name_field.y + 2.0,
                name_field.w - 4.0,
                name_field.h - 4.0,
                1.0,
                Color::from_rgba(20, 55, 190, 255),
            );
        }

        let crowd = self.facility_crowd(facility);
        let (sprite, source) = self.assets.facility_sprite_for(
            facility,
            crowd,
            self.tenant_variants.get(&facility.id).copied(),
            self.tower.clock.minute_of_day,
            self.facility_visual_load(facility),
        );
        let scale = (300.0 / source.w).min(54.0 / source.h).min(2.0);
        let sprite_size = vec2(source.w * scale, source.h * scale);
        draw_texture_ex(
            &sprite.texture,
            (panel.x + panel.w * 0.5 - sprite_size.x * 0.5).round(),
            panel.y + 48.0 + (54.0 - sprite_size.y) * 0.5,
            WHITE,
            DrawTextureParams {
                source: Some(source),
                dest_size: Some(sprite_size),
                ..Default::default()
            },
        );

        let reachable = self.traffic.reachable_facility_set().contains(&facility.id);
        let patronage = self.traffic.daily_visitors_at(facility.id);
        let current_inside = self.traffic.visitors_at(facility.id) as u32;
        let previous_profit = self
            .tenant_finances
            .get(&facility.id)
            .map_or(0, |state| state.previous_profit);
        let details = tenant_info_details(facility, patronage, current_inside, previous_profit);
        let field_x = panel.x + 222.0;
        let field_w = panel.w - 247.0;
        for (index, (label, value)) in details.fields.iter().enumerate() {
            let y = panel.y + 119.0 + index as f32 * 31.0;
            draw_right_aligned_text(label, field_x - 10.0, y + 19.0, 16, BLACK);
            let field = Rect::new(field_x, y, field_w, 24.0);
            draw_recessed_field(field);
            if index == 0 && details.progress_max > 0 {
                let filled = (field.w - 4.0)
                    * (details.progress_value as f32 / details.progress_max as f32).clamp(0.0, 1.0);
                if filled > 0.0 {
                    draw_rectangle(
                        field.x + 2.0,
                        field.y + 2.0,
                        filled,
                        field.h - 4.0,
                        Color::from_rgba(22, 40, 225, 255),
                    );
                }
            }
            draw_fitted_text(
                value,
                field.x + 6.0,
                field.y + 18.0,
                field.w - 12.0,
                16,
                BLACK,
            );
        }

        let people_box = Rect::new(panel.x + 14.0, panel.y + 183.0, panel.w - 28.0, 39.0);
        draw_recessed_field(people_box);
        if facility.kind == FacilityKind::Condo && details.people_icons > 0 {
            draw_condo_family_meter(&self.assets.people, people_box);
        } else {
            draw_tenant_people_meter(
                &self.assets.people_silhouette[0],
                people_box,
                details.people_icons,
            );
        }

        let evaluation_box = Rect::new(panel.x + 14.0, panel.y + 231.0, panel.w - 28.0, 55.0);
        draw_recessed_field(evaluation_box);
        let evaluation = tenant_evaluation(facility, reachable, patronage, current_inside);
        draw_fitted_text(
            evaluation,
            evaluation_box.x + 8.0,
            evaluation_box.y + 22.0,
            evaluation_box.w - 16.0,
            16,
            BLACK,
        );

        let rename = tenant_info_rename_rect();
        draw_classic_button(rename, false);
        draw_centered_text(
            if panel_state.renaming {
                "Save name"
            } else if facility.kind == FacilityKind::Cinema {
                "Change movie"
            } else {
                "Rename"
            },
            rename.x + rename.w * 0.5,
            rename.y + 20.0,
            15,
            BLACK,
        );
        let ok = tenant_info_ok_rect();
        draw_classic_button(ok, false);
        draw_centered_text("OK", ok.x + ok.w * 0.5, ok.y + 20.0, 15, BLACK);
    }

    fn draw_person_panel(&self) {
        let Some(panel_state) = &self.person_panel else {
            return;
        };
        let Some(person) = self
            .traffic
            .people()
            .iter()
            .find(|person| person.id == panel_state.person_id)
        else {
            return;
        };
        let panel = person_info_rect();
        draw_classic_panel(panel);
        draw_rectangle_lines(
            panel.x + 5.0,
            panel.y + 5.0,
            panel.w - 10.0,
            panel.h - 10.0,
            1.0,
            CLASSIC_SHADOW,
        );

        let portrait = Rect::new(panel.x + 14.0, panel.y + 14.0, 52.0, 72.0);
        draw_recessed_field(portrait);
        // The original inspector uses the same solid-black person artwork as
        // its tenant meters, not the colored walking animation.
        let portrait_source = Rect::new(0.0, 0.0, 16.0, 36.0);
        draw_texture_ex(
            &self.assets.people_silhouette[0],
            portrait.x + 14.0,
            portrait.y + 9.0,
            WHITE,
            DrawTextureParams {
                source: Some(portrait_source),
                dest_size: Some(vec2(24.0, 54.0)),
                ..Default::default()
            },
        );

        let name_field = Rect::new(panel.x + 78.0, panel.y + 14.0, panel.w - 92.0, 24.0);
        draw_recessed_field(name_field);
        let default_name = person_role(person, &self.tower);
        let name = if panel_state.renaming {
            format!("{}|", panel_state.rename_draft)
        } else {
            self.person_names
                .get(&person.id)
                .cloned()
                .unwrap_or_else(|| default_name.to_owned())
        };
        draw_fitted_text(
            &name,
            name_field.x + 6.0,
            name_field.y + 18.0,
            name_field.w - 12.0,
            16,
            BLACK,
        );

        let (from, going_to) = person_trip_labels(person, &self.tower);
        draw_right_aligned_text("From", panel.x + 116.0, panel.y + 66.0, 14, BLACK);
        let from_field = Rect::new(panel.x + 122.0, panel.y + 47.0, panel.w - 136.0, 22.0);
        draw_recessed_field(from_field);
        draw_fitted_text(
            &from,
            from_field.x + 5.0,
            from_field.y + 17.0,
            from_field.w - 10.0,
            14,
            BLACK,
        );

        draw_right_aligned_text("Eval", panel.x + 116.0, panel.y + 96.0, 14, BLACK);
        let evaluation_field = Rect::new(panel.x + 122.0, panel.y + 77.0, panel.w - 136.0, 22.0);
        draw_person_status_meter(
            evaluation_field,
            person_evaluation_fraction(person),
            Color::from_rgba(18, 42, 225, 255),
            true,
        );

        draw_right_aligned_text("Going To", panel.x + 116.0, panel.y + 126.0, 14, BLACK);
        let destination_field = Rect::new(panel.x + 122.0, panel.y + 107.0, panel.w - 136.0, 22.0);
        draw_recessed_field(destination_field);
        draw_fitted_text(
            &going_to,
            destination_field.x + 5.0,
            destination_field.y + 17.0,
            destination_field.w - 10.0,
            14,
            BLACK,
        );

        draw_right_aligned_text("Stress", panel.x + 116.0, panel.y + 156.0, 14, BLACK);
        let stress_field = Rect::new(panel.x + 122.0, panel.y + 137.0, panel.w - 136.0, 22.0);
        draw_person_status_meter(
            stress_field,
            person_stress_fraction(person),
            Color::from_rgba(235, 25, 25, 255),
            false,
        );

        let rename = person_info_rename_rect();
        draw_classic_button(rename, false);
        draw_centered_text(
            if panel_state.renaming {
                "Save name"
            } else {
                "Rename"
            },
            rename.x + rename.w * 0.5,
            rename.y + 19.0,
            14,
            BLACK,
        );
        let ok = person_info_ok_rect();
        draw_classic_button(ok, false);
        draw_centered_text("OK", ok.x + ok.w * 0.5, ok.y + 19.0, 14, BLACK);
    }

    fn draw_empty_floor_room(&self, position: GridPosition) {
        let x = (f32::from(position.x) - self.camera_x) * CELL_WIDTH;
        let room_y = tenant_room_top_y(self.ground_y(), position.floor);
        draw_rectangle(
            x,
            room_y,
            CELL_WIDTH,
            ROOM_HEIGHT,
            Color::from_rgba(55, 56, 57, 255),
        );
    }

    fn draw_empty_floor_strip(&self, position: GridPosition) {
        let x = (f32::from(position.x) - self.camera_x) * CELL_WIDTH;
        self.draw_floor_strip(
            position.x,
            x,
            utility_strip_top_y(self.ground_y(), position.floor),
            CELL_WIDTH,
        );
    }

    fn floor_cell_is_visible(&self, position: GridPosition) -> bool {
        let x = (f32::from(position.x) - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), position.floor);
        x + CELL_WIDTH >= 0.0
            && x <= screen_width()
            && y >= TOPBAR_HEIGHT - FLOOR_HEIGHT
            && y <= screen_height() + FLOOR_HEIGHT
    }

    fn facility_is_visible(&self, facility: &Facility) -> bool {
        let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
        let width = f32::from(facility.kind.spec().width) * CELL_WIDTH;
        let bottom = floor_top_y(self.ground_y(), facility.position.floor) + FLOOR_HEIGHT;
        let stories = if matches!(
            facility.kind,
            FacilityKind::Stairs | FacilityKind::Escalator
        ) {
            vertical_walkway_story_span(facility, self.tower.facilities())
        } else {
            facility.height()
        };
        let top = bottom - f32::from(stories) * FLOOR_HEIGHT;
        x + width >= 0.0
            && x <= screen_width()
            && bottom >= TOPBAR_HEIGHT - FLOOR_HEIGHT
            && top <= screen_height() + FLOOR_HEIGHT
    }

    fn draw_floor_end_stairs(&self) {
        let floor_cells = self.tower.floors().iter().copied().collect::<HashSet<_>>();
        for start in self.tower.floors() {
            if start.x > 0
                && floor_cells.contains(&GridPosition {
                    x: start.x - 1,
                    floor: start.floor,
                })
            {
                continue;
            }

            let y = floor_top_y(self.ground_y(), start.floor);
            if y + FLOOR_HEIGHT < TOPBAR_HEIGHT || y > screen_height() {
                continue;
            }

            let mut end_x = start.x + 1;
            while end_x < self.tower.width()
                && floor_cells.contains(&GridPosition {
                    x: end_x,
                    floor: start.floor,
                })
            {
                end_x += 1;
            }

            let destinations = emergency_stair_screen_xs(start.x, end_x, self.camera_x);
            for (destination_x, source_x) in destinations
                .into_iter()
                .zip([0.0, EMERGENCY_STAIR_HALF_WIDTH])
            {
                if !horizontal_span_is_visible(
                    destination_x,
                    EMERGENCY_STAIR_HALF_WIDTH,
                    screen_width(),
                ) {
                    continue;
                }
                draw_texture_ex(
                    &self.assets.emergency_stairs,
                    destination_x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(EMERGENCY_STAIR_HALF_WIDTH, FLOOR_HEIGHT)),
                        source: Some(Rect::new(
                            source_x,
                            0.0,
                            EMERGENCY_STAIR_HALF_WIDTH,
                            FLOOR_HEIGHT,
                        )),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_facility(&self, facility: &Facility) {
        let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), facility.position.floor);
        let width = f32::from(facility.kind.spec().width) * CELL_WIDTH;
        if self
            .construction
            .iter()
            .any(|animation| animation.facility_id == facility.id)
        {
            let stories = if matches!(
                facility.kind,
                FacilityKind::Stairs | FacilityKind::Escalator
            ) {
                vertical_walkway_story_span(facility, self.tower.facilities())
            } else {
                facility.height()
            };
            for story in 0..stories {
                self.draw_scaffolding(x, y - f32::from(story) * FLOOR_HEIGHT, width);
            }
            return;
        }
        if facility.is_burned() {
            self.draw_burned_facility(facility, x, y, width);
            return;
        }
        let crowd = self.facility_crowd(facility);
        let (sprite, sprite_source) = self.assets.facility_sprite_for(
            facility,
            crowd,
            self.tenant_variants.get(&facility.id).copied(),
            self.tower.clock.minute_of_day,
            self.facility_visual_load(facility),
        );
        if facility.kind == FacilityKind::Lobby {
            self.draw_lobby_slice(facility, x, y, width);
            return;
        }
        if matches!(
            facility.kind,
            FacilityKind::Stairs | FacilityKind::Escalator
        ) {
            self.draw_stair_transport(facility, x, y, width, sprite);
            return;
        }
        if is_elevator_kind(facility.kind) {
            let shaft = self.traffic.elevator_at(facility.kind, facility.position.x);
            let visible = shaft.is_none_or(|shaft| shaft.visible);
            let default_waiting_floor = shaft.is_some_and(|shaft| {
                shaft
                    .cars
                    .iter()
                    .any(|car| car.home_floor == facility.position.floor)
            });
            self.draw_empty_elevator_shaft(
                x,
                y,
                width,
                facility.position.floor,
                visible,
                default_waiting_floor,
            );
            return;
        }
        let floors = f32::from(facility.kind.spec().height);
        let structural_draw_y = y - (floors - 1.0) * FLOOR_HEIGHT;
        let draw_height = if floors > 1.0 || sprite.source.h > ROOM_HEIGHT {
            floors * FLOOR_HEIGHT
        } else {
            ROOM_HEIGHT
        };
        let draw_y = if draw_height == ROOM_HEIGHT {
            tenant_room_top_y(self.ground_y(), facility.position.floor)
        } else {
            structural_draw_y
        };
        draw_texture_ex(
            &sprite.texture,
            x,
            draw_y,
            facility_light_tint(facility.kind, crowd),
            DrawTextureParams {
                dest_size: Some(vec2(width, draw_height)),
                source: Some(sprite_source),
                ..Default::default()
            },
        );
        let overlay_base_y = if draw_height == ROOM_HEIGHT {
            draw_y
        } else {
            y
        };
        self.draw_facility_state_overlay(facility, x, overlay_base_y, width);
    }

    fn draw_burned_facility(&self, facility: &Facility, x: f32, y: f32, width: f32) {
        let source_size = vec2(
            self.assets.fire_aftermath.width(),
            self.assets.fire_aftermath.height(),
        );
        for story in 0..facility.height() {
            let story_top_y = y - f32::from(story) * FLOOR_HEIGHT;
            let room_y = if facility.position.floor > 0 {
                story_top_y + FLOOR_HEIGHT - ROOM_HEIGHT
            } else {
                story_top_y
            };
            let (destination, source) = burned_unit_frame(x, room_y, width, source_size);
            draw_texture_ex(
                &self.assets.fire_aftermath,
                destination.x,
                destination.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(destination.w, destination.h)),
                    source: Some(source),
                    ..Default::default()
                },
            );
        }
    }

    fn facility_crowd(&self, facility: &Facility) -> FacilityCrowd {
        if self
            .fire_event
            .is_some_and(|event| event.facility_id == facility.id)
        {
            return FacilityCrowd::Closed;
        }
        let visitors = match facility.kind {
            FacilityKind::Condo
            | FacilityKind::HotelSingle
            | FacilityKind::HotelTwin
            | FacilityKind::HotelSuite
                if self.traffic.has_received_visitor(facility.id) =>
            {
                1
            }
            _ => self.traffic.visitors_at(facility.id),
        };
        facility_crowd_level(
            facility.kind,
            facility.is_occupied(),
            visitors,
            facility_is_open_at(facility.kind, self.tower.clock),
        )
    }

    fn facility_visual_load(&self, facility: &Facility) -> usize {
        match facility.kind {
            // Zero is a connected, vacant space; 1..=13 select one of the
            // parked-car cels; 14 is the disconnected red-X warning cel.
            FacilityKind::Parking if !self.parking_state.connected.contains(&facility.id) => 14,
            FacilityKind::Parking if self.parking_state.occupied.contains(&facility.id) => {
                parking_car_frame(facility)
            }
            FacilityKind::Parking => 0,
            FacilityKind::Metro => usize::from(
                self.traffic.people().len() >= 8
                    && matches!(self.tower.clock.minute_of_day, 390..=570 | 960..=1140),
            ),
            FacilityKind::Medical => self
                .traffic
                .people()
                .iter()
                .filter(|person| person.mood != PersonMood::Calm)
                .count()
                .min(2),
            FacilityKind::Recycling => {
                let centers = self
                    .tower
                    .facilities()
                    .iter()
                    .filter(|other| other.kind == FacilityKind::Recycling)
                    .count()
                    .max(1);
                (self.population() as usize / (centers * 350)).min(5)
            }
            FacilityKind::Ramp => {
                let connected_parking = self.tower.facilities().iter().filter(|other| {
                    other.kind == FacilityKind::Parking
                        && (other.position.x as i32 - facility.position.x as i32).abs() <= 20
                });
                connected_parking.count().min(2)
            }
            FacilityKind::Cathedral => usize::from(self.tower.cathedral_wedding_active()),
            _ => self.traffic.visitors_at(facility.id),
        }
    }

    fn draw_facility_state_overlay(&self, facility: &Facility, x: f32, base_y: f32, width: f32) {
        if facility.kind == FacilityKind::Office {
            let worker_count = office_visible_worker_count(self.traffic.visitors_at(facility.id));
            if worker_count > 0 {
                let sheet = person_sprite_sheet(Some(FacilityKind::Office), facility.id);
                let frame_count = (self.assets.people[sheet].width() as u64 / 8).max(1);
                let slot_width = width / (worker_count as f32 + 1.0);
                for worker in 0..worker_count {
                    // Keep each worker's appearance deterministic so occupancy changes add or
                    // remove people without making everyone already in the office jump poses.
                    let frame = (facility.id + worker as u64 * 4) % frame_count;
                    let worker_x = x + slot_width * (worker as f32 + 1.0) - 4.0;
                    self.draw_person_frame(sheet, frame, worker_x, base_y, WHITE);
                }
            }
        }

        if facility.kind == FacilityKind::Recycling {
            let level = self.facility_visual_load(facility);
            if level > 0 {
                draw_texture_ex(
                    &self.assets.recycling_fill[level - 1],
                    x,
                    base_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(width, ROOM_HEIGHT)),
                        ..Default::default()
                    },
                );
            }
            if level >= 4 && (5 * 60..7 * 60).contains(&self.tower.clock.minute_of_day) {
                draw_texture_ex(
                    &self.assets.recycling_truck,
                    x,
                    base_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(width, FLOOR_HEIGHT)),
                        ..Default::default()
                    },
                );
            }
        }

        let Some(event) = self
            .fire_event
            .filter(|event| event.facility_id == facility.id)
        else {
            return;
        };
        let fire_y = floor_top_y(self.ground_y(), facility.position.floor);
        if event.elapsed < 5.0 {
            let frame = ((event.elapsed * 8.0) as usize) % 4;
            self.draw_fire_strip(
                &self.assets.fire_small,
                Some(Rect::new(frame as f32 * 96.0, 0.0, 96.0, 36.0)),
                x,
                fire_y,
                width,
            );
        } else {
            let phase = ((event.elapsed * 5.0) as usize) % self.assets.fire_large.len();
            self.draw_fire_strip(&self.assets.fire_large[phase], None, x, fire_y, width);
        }
    }

    fn draw_fire_strip(
        &self,
        texture: &Texture2D,
        source: Option<Rect>,
        x: f32,
        y: f32,
        width: f32,
    ) {
        let tile_width = source.map_or(texture.width(), |rect| rect.w).min(96.0);
        let source_x = source.map_or(0.0, |rect| rect.x);
        let source_y = source.map_or(0.0, |rect| rect.y);
        for (destination, source_segment) in
            fire_strip_segments(x, y, width, tile_width, source_x, source_y)
        {
            draw_texture_ex(
                texture,
                destination.x,
                destination.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(destination.w, destination.h)),
                    source: Some(source_segment),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_empty_elevator_shaft(
        &self,
        x: f32,
        y: f32,
        width: f32,
        floor: i16,
        visible: bool,
        default_waiting_floor: bool,
    ) {
        let tint = Color::new(1.0, 1.0, 1.0, elevator_display_alpha(visible));
        let tile_width = self.assets.elevator_shaft.width();
        let mut offset = 0.0;
        while offset < width {
            let draw_width = (width - offset).min(tile_width);
            draw_texture_ex(
                &self.assets.elevator_shaft,
                x + offset,
                y,
                tint,
                DrawTextureParams {
                    dest_size: Some(vec2(draw_width, FLOOR_HEIGHT)),
                    source: Some(Rect::new(0.0, 0.0, draw_width, FLOOR_HEIGHT)),
                    ..Default::default()
                },
            );
            offset += draw_width;
        }

        // Resources 2024-2026 are the light-gray shaft designations;
        // 2027-2029 are their original light-pink default-floor counterparts.
        // Cars are drawn later and naturally cover the label.
        self.draw_elevator_floor_number(floor, x, y, width, tint, default_waiting_floor);
    }

    fn draw_elevator_floor_number(
        &self,
        floor: i16,
        x: f32,
        y: f32,
        width: f32,
        tint: Color,
        default_waiting_floor: bool,
    ) {
        let (single_atlas, digit_atlas, basement_atlas) =
            elevator_number_texture_indices(default_waiting_floor);
        if (1..=9).contains(&floor) || floor == 100 {
            let frame = if floor == 100 { 10 } else { floor as usize };
            draw_texture_ex(
                &self.assets.elevator_numbers[single_atlas],
                x + (width - 32.0) * 0.5,
                y,
                tint,
                DrawTextureParams {
                    source: Some(Rect::new(frame as f32 * 32.0, 0.0, 32.0, 36.0)),
                    dest_size: Some(vec2(32.0, 36.0)),
                    ..Default::default()
                },
            );
            return;
        }

        // The compound-number atlases use 16-pixel cells, but pixel zero is
        // the shaft divider and the final four pixels are cell padding. Draw
        // only the eleven-pixel glyph area so two-digit labels remain a
        // single centered number instead of two separated shaft panels.
        const CELL_WIDTH: f32 = 16.0;
        const GLYPH_INSET: f32 = 1.0;
        const GLYPH_WIDTH: f32 = 11.0;
        // Compound labels use transparent glyph cels. Redraw frame zero from
        // the single-number atlas first so the shaft's vertical ribs cannot
        // show through the label as gray lines on floors 10 and above.
        let backplate = elevator_compound_label_backplate(x, y, width);
        draw_texture_ex(
            &self.assets.elevator_numbers[single_atlas],
            backplate.x,
            backplate.y,
            tint,
            DrawTextureParams {
                source: Some(Rect::new(0.0, 0.0, 32.0, 36.0)),
                dest_size: Some(vec2(backplate.w, backplate.h)),
                ..Default::default()
            },
        );
        let label = floor.unsigned_abs().to_string();
        let glyph_count = label.len() + usize::from(floor < 0);
        let total_width = GLYPH_WIDTH * glyph_count as f32;
        let mut draw_x = x + (width - total_width) * 0.5;

        if floor < 0 {
            // Resource 2026 starts with packed B1/B2 cels. Its third cell is
            // the standalone B used when composing normal shaft labels.
            self.draw_elevator_label_glyph(
                &self.assets.elevator_numbers[basement_atlas],
                2.0 * CELL_WIDTH + 4.0,
                draw_x,
                y,
                GLYPH_WIDTH,
                tint,
            );
            draw_x += GLYPH_WIDTH;
        }

        for character in label.chars() {
            let digit = character.to_digit(10).unwrap_or(0) as f32;
            draw_texture_ex(
                &self.assets.elevator_numbers[digit_atlas],
                draw_x,
                y,
                tint,
                DrawTextureParams {
                    source: Some(Rect::new(
                        digit * CELL_WIDTH + GLYPH_INSET,
                        0.0,
                        GLYPH_WIDTH,
                        36.0,
                    )),
                    dest_size: Some(vec2(GLYPH_WIDTH, 36.0)),
                    ..Default::default()
                },
            );
            draw_x += GLYPH_WIDTH;
        }
    }

    fn draw_elevator_label_glyph(
        &self,
        texture: &Texture2D,
        source_x: f32,
        x: f32,
        y: f32,
        width: f32,
        tint: Color,
    ) {
        draw_texture_ex(
            texture,
            x,
            y,
            tint,
            DrawTextureParams {
                source: Some(Rect::new(source_x, 0.0, width, 36.0)),
                dest_size: Some(vec2(width, 36.0)),
                ..Default::default()
            },
        );
    }

    fn draw_stair_transport(
        &self,
        facility: &Facility,
        x: f32,
        base_y: f32,
        width: f32,
        sprite: &Sprite,
    ) {
        let stories = vertical_walkway_story_span(facility, self.tower.facilities());
        draw_texture_ex(
            &sprite.texture,
            x,
            base_y - (f32::from(stories) - 1.0) * FLOOR_HEIGHT,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(width, FLOOR_HEIGHT * f32::from(stories))),
                source: Some(self.stair_sprite_source(facility, sprite)),
                ..Default::default()
            },
        );
    }

    fn draw_lobby_slice(&self, facility: &Facility, x: f32, y: f32, width: f32) {
        let column = facility.position.x;
        let floor = facility.position.floor;
        let mut run_start = column;
        while run_start > 0 && self.lobby_at(run_start - 1, floor) {
            run_start -= 1;
        }
        let rating = self.star_rating();
        for story in 0..facility.stories {
            let (sprite, source_base) = self.assets.facilities.lobby_layer(
                usize::from(story),
                facility.stories,
                floor,
                rating,
            );
            let source_x = source_base
                + (f32::from(column - run_start) * CELL_WIDTH).rem_euclid(LOBBY_BODY_WIDTH);
            draw_texture_ex(
                &sprite.texture,
                x,
                y - f32::from(story) * FLOOR_HEIGHT,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(width, FLOOR_HEIGHT)),
                    source: Some(Rect::new(source_x, 0.0, width, FLOOR_HEIGHT)),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_floor_strip(&self, column: u16, x: f32, y: f32, width: f32) {
        let mut offset = 0.0;
        let source_width = self.assets.floor_strip.width();
        let mut source_x = (f32::from(column) * CELL_WIDTH).rem_euclid(source_width);
        while offset < width {
            let tile_width = (width - offset).min(source_width - source_x);
            draw_texture_ex(
                &self.assets.floor_strip,
                x + offset,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(tile_width, FLOOR_HEIGHT - ROOM_HEIGHT)),
                    source: Some(Rect::new(source_x, 0.0, tile_width, 12.0)),
                    ..Default::default()
                },
            );
            offset += tile_width;
            source_x = 0.0;
        }
    }

    fn draw_lobby_entrances(&self, facility: &Facility) {
        if self
            .construction
            .iter()
            .any(|animation| animation.facility_id == facility.id)
            || self.lobby_at(
                facility.position.x.saturating_sub(1),
                facility.position.floor,
            ) && facility.position.x > 0
        {
            return;
        }

        let mut end = facility.position.x + 1;
        while self.lobby_at(end, facility.position.floor) {
            end += 1;
        }
        let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), facility.position.floor);
        let width = f32::from(end - facility.position.x) * CELL_WIDTH;
        let facade_width = width.min(LOBBY_FACADE_WIDTH);
        // Each 328-pixel lobby chunk carries a 256-pixel tiled body and its
        // own left facade at +272. Sky lobbies use chunk 1; grand-lobby
        // stories select the matching bottom, middle, and top chunks.
        let rating = self.star_rating();
        for story in 0..facility.stories {
            let (lobby, source_base) = self.assets.facilities.lobby_layer(
                usize::from(story),
                facility.stories,
                facility.position.floor,
                rating,
            );
            draw_texture_ex(
                &lobby.texture,
                x,
                y - f32::from(story) * FLOOR_HEIGHT,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(facade_width, FLOOR_HEIGHT)),
                    source: Some(Rect::new(
                        source_base + LOBBY_FACADE_OFFSET,
                        0.0,
                        facade_width,
                        FLOOR_HEIGHT,
                    )),
                    ..Default::default()
                },
            );
        }

        if facility.position.floor != 1 {
            return;
        }

        // Bitmap 1001 is the two exterior OPEN awnings. Anchor them to the
        // complete structural floor run, not to the painted lobby run.
        let mut floor_start = facility.position.x;
        while floor_start > 0
            && self.tower.has_floor(GridPosition {
                x: floor_start - 1,
                floor: facility.position.floor,
            })
        {
            floor_start -= 1;
        }
        let mut floor_end = end;
        while floor_end < self.tower.width()
            && self.tower.has_floor(GridPosition {
                x: floor_end,
                floor: facility.position.floor,
            })
        {
            floor_end += 1;
        }
        let floor_left_x = (f32::from(floor_start) - self.camera_x) * CELL_WIDTH;
        let floor_right_x = (f32::from(floor_end) - self.camera_x) * CELL_WIDTH;
        let cap = LOBBY_AWNING_HALF_WIDTH;
        for (destination_x, source_x) in [
            (floor_left_x - cap, 0.0),
            (floor_right_x, LOBBY_AWNING_HALF_WIDTH),
        ] {
            draw_texture_ex(
                &self.assets.lobby_awning,
                destination_x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(cap, FLOOR_HEIGHT)),
                    source: Some(Rect::new(source_x, 0.0, cap, FLOOR_HEIGHT)),
                    ..Default::default()
                },
            );
        }
    }

    fn lobby_at(&self, x: u16, floor: i16) -> bool {
        self.tower.facilities().iter().any(|facility| {
            facility.kind == FacilityKind::Lobby
                && facility.position.x == x
                && facility.position.floor == floor
        })
    }

    fn draw_scaffolding(&self, x: f32, y: f32, width: f32) {
        let source_width = width.min(self.assets.scaffolding.width());
        draw_texture_ex(
            &self.assets.scaffolding,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(width, FLOOR_HEIGHT)),
                source: Some(Rect::new(0.0, 0.0, source_width, FLOOR_HEIGHT)),
                ..Default::default()
            },
        );
        draw_rectangle_lines(
            x,
            y,
            width,
            FLOOR_HEIGHT,
            1.0,
            Color::from_rgba(95, 52, 33, 255),
        );
    }

    fn draw_hover(&self, mouse: Vec2) {
        if matches!(self.tool, ToolMode::Inspect | ToolMode::Demolish) {
            return;
        }
        let position = self.grid_position(mouse);
        if shift_modifier_down() {
            let fill = match self.tool {
                ToolMode::Floor => Some((floor_fill_plan(&self.tower, position.floor), 1_u8)),
                ToolMode::Build(FacilityKind::Lobby) => {
                    let stories = lobby_stories_for_position(&self.tower, position);
                    (stories == 1).then(|| {
                        (
                            lobby_fill_plan(&self.tower, position.floor, stories),
                            stories,
                        )
                    })
                }
                ToolMode::Build(kind) if kind.has_tenant_occupancy() => {
                    let plan = tenant_fill_plan(&self.tower, kind, position);
                    if let (Some(first), Some(last)) =
                        (plan.positions.first(), plan.positions.last())
                    {
                        let x = (f32::from(first.x) - self.camera_x) * CELL_WIDTH;
                        let y = floor_top_y(self.ground_y(), first.floor) + FLOOR_HEIGHT
                            - f32::from(kind.spec().height) * FLOOR_HEIGHT;
                        let end_x = last.x + kind.spec().width;
                        draw_rectangle_lines(
                            x,
                            y,
                            f32::from(end_x - first.x) * CELL_WIDTH,
                            f32::from(kind.spec().height) * FLOOR_HEIGHT,
                            3.0,
                            GREEN,
                        );
                        return;
                    }
                    None
                }
                ToolMode::Build(_) | ToolMode::Inspect | ToolMode::Demolish => None,
            };
            if let Some((plan, stories)) = fill
                && !plan.columns.is_empty()
            {
                let height = f32::from(stories) * FLOOR_HEIGHT;
                let y = floor_top_y(self.ground_y(), position.floor) + FLOOR_HEIGHT - height;
                for (start, end) in contiguous_column_runs(&plan.columns) {
                    let x = (f32::from(start) - self.camera_x) * CELL_WIDTH;
                    let width = f32::from(end - start) * CELL_WIDTH;
                    draw_rectangle_lines(x, y, width, height, 3.0, GREEN);
                }
                return;
            }
        }
        let (preview_position, width, height, valid) = match self.tool {
            ToolMode::Floor => (
                position,
                CELL_WIDTH,
                FLOOR_HEIGHT,
                self.tower.can_place_floor(position).is_ok(),
            ),
            ToolMode::Build(FacilityKind::Lobby) => {
                let stories = lobby_stories_for_position(&self.tower, position);
                let mut preview = self.tower.clone();
                (
                    position,
                    CELL_WIDTH,
                    f32::from(stories) * FLOOR_HEIGHT,
                    preview.place_lobby(position, stories).is_ok(),
                )
            }
            ToolMode::Build(kind) => {
                let existing_car = is_elevator_kind(kind).then(|| {
                    self.elevator_facility_at(position)
                        .filter(|facility| facility.kind == kind)
                        .map(|facility| {
                            let shaft = self.traffic.elevator_at(kind, facility.position.x);
                            let valid = shaft.is_some_and(|shaft| {
                                shaft.cars.len() < simtower_core::MAX_ELEVATOR_CARS
                                    && self.tower.cash() >= elevator_car_cost(kind)
                                    && self.traffic.elevator_can_stop_at(
                                        &self.tower,
                                        shaft.id,
                                        position.floor,
                                    )
                            });
                            (facility.position, valid)
                        })
                });
                if let Some((shaft_position, valid)) = existing_car.flatten() {
                    (
                        shaft_position,
                        f32::from(kind.spec().width) * CELL_WIDTH,
                        f32::from(kind.spec().height) * FLOOR_HEIGHT,
                        valid,
                    )
                } else {
                    let preview_stories =
                        vertical_walkway_story_span_at(kind, position, self.tower.facilities());
                    let starts_invalid_elevator = is_elevator_kind(kind)
                        && self.traffic.elevator_at(kind, position.x).is_none()
                        && !elevator_landing_is_valid(
                            kind,
                            position.x,
                            position.floor,
                            self.tower.facilities(),
                        );
                    (
                        position,
                        f32::from(kind.spec().width) * CELL_WIDTH,
                        f32::from(preview_stories) * FLOOR_HEIGHT,
                        self.tower.can_place(kind, position).is_ok() && !starts_invalid_elevator,
                    )
                }
            }
            ToolMode::Inspect | ToolMode::Demolish => unreachable!(),
        };
        let x = (f32::from(preview_position.x) - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), preview_position.floor);
        let draw_y = y + FLOOR_HEIGHT - height;
        let color = if valid { GREEN } else { RED };
        draw_rectangle_lines(x, draw_y, width, height, 3.0, color);
    }

    fn draw_demolition_cursor(&self, mouse: Vec2) {
        let scale = self.ux_scale.factor();
        let radius = 9.0 * scale;
        // The intersection is the hotspot: demolition hit-testing uses this
        // exact mouse pixel rather than the surrounding grid cell.
        for (thickness, color) in [(5.0 * scale, BLACK), (2.0 * scale, RED)] {
            draw_line(
                mouse.x - radius,
                mouse.y - radius,
                mouse.x + radius,
                mouse.y + radius,
                thickness,
                color,
            );
            draw_line(
                mouse.x + radius,
                mouse.y - radius,
                mouse.x - radius,
                mouse.y + radius,
                thickness,
                color,
            );
        }
        draw_circle(mouse.x, mouse.y, 1.25 * scale, WHITE);
    }

    fn draw_magnifier_cursor(&self, mouse: Vec2) {
        let scale = self.ux_scale.factor();
        let lens_center = vec2(7.0, 7.0) * scale;
        let size = vec2(22.0, 21.0) * scale;
        // Hit-testing uses `mouse` itself, so the original lens is centered on
        // that exact physical pixel. The source is a 4x edge-aware upscale.
        let top_left = mouse - lens_center;
        draw_texture_ex(
            &self.assets.magnifier_cursor,
            top_left.x,
            top_left.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
    }

    fn draw_elevator_resize_cursor(&self, mouse: Vec2, direction: i8) {
        let cursor_size = 25.0 * self.ux_scale.factor();
        // The mouse hotspot is the fingertip, matching the original cursor.
        let top_left = if direction > 0 {
            mouse - vec2(cursor_size * 0.5, 0.0)
        } else {
            mouse - vec2(cursor_size * 0.5, cursor_size)
        };
        draw_texture_ex(
            &self.assets.elevator_cursor,
            top_left.x,
            top_left.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(cursor_size, cursor_size)),
                flip_y: direction < 0,
                ..Default::default()
            },
        );
    }

    fn draw_people(&self) {
        let facility_kinds = self
            .tower
            .facilities()
            .iter()
            .map(|facility| (facility.id, facility.kind))
            .collect::<HashMap<_, _>>();
        let mut queued_per_stop = HashMap::<(u64, i16), usize>::new();
        let queue_ranks = self
            .traffic
            .people()
            .iter()
            .filter_map(|person| {
                let PersonActivity::WaitingForElevator { shaft_id } = person.activity else {
                    return None;
                };
                let rank = queued_per_stop
                    .entry((shaft_id, person.current_floor))
                    .or_default();
                let result = (person.id, *rank);
                *rank += 1;
                Some(result)
            })
            .collect::<HashMap<_, _>>();
        for person in self.traffic.people() {
            if matches!(
                person.activity,
                PersonActivity::Visiting
                    | PersonActivity::RidingElevator { .. }
                    | PersonActivity::UsingStairs
            ) {
                continue;
            }
            let visual_x = self
                .visual_people_x
                .get(&person.id)
                .copied()
                .unwrap_or(person.x);
            let x = match person.activity {
                PersonActivity::WaitingForElevator { shaft_id } => self
                    .elevator_queue_draw_x(shaft_id, *queue_ranks.get(&person.id).unwrap_or(&0))
                    .unwrap_or((visual_x - self.camera_x) * CELL_WIDTH - 4.0),
                _ => (visual_x - self.camera_x) * CELL_WIDTH - 4.0,
            };
            let exterior_ground = person.current_floor == 1
                && !self.person_has_floor_below(person.x, person.current_floor);
            let y = if exterior_ground {
                self.ground_y() - PERSON_SPRITE_HEIGHT
            } else {
                interior_person_top_y(self.ground_y(), person.floor_position)
            };
            let tint = match person.mood {
                PersonMood::Calm => WHITE,
                PersonMood::Concerned => Color::from_rgba(
                    QUEUE_CONCERNED_PINK[0],
                    QUEUE_CONCERNED_PINK[1],
                    QUEUE_CONCERNED_PINK[2],
                    255,
                ),
                PersonMood::Angry => Color::from_rgba(255, 35, 35, 255),
            };
            if self
                .vip_visit
                .is_some_and(|visit| visit.person_id == person.id)
            {
                draw_triangle(
                    vec2(x + 4.0, y - 5.0),
                    vec2(x + 1.0, y - 10.0),
                    vec2(x + 7.0, y - 10.0),
                    YELLOW,
                );
                draw_circle_lines(x + 4.0, y + 11.0, 7.0, 1.0, YELLOW);
            }
            if matches!(person.activity, PersonActivity::WaitingForElevator { .. }) {
                self.draw_queue_person(person.mood, person.id, x, y);
                continue;
            }
            if self.view_mode != ViewMode::Edit {
                self.draw_silhouette_person(person.id, x, y);
                continue;
            }
            let destination_kind = facility_kinds.get(&person.destination_facility_id).copied();
            let home_kind = facility_kinds.get(&person.home_facility_id).copied();
            let sheet = traveler_sprite_sheet(person.role, home_kind, destination_kind, person.id);
            let pair_count = (self.assets.people[sheet].width() as u64 / 16).max(1);
            let pair = (person.id % pair_count) * 2;
            if matches!(
                person.activity,
                PersonActivity::Walking | PersonActivity::LeavingElevatorQueue
            ) {
                let paused = self.tower.clock.speed == SimulationSpeed::Paused;
                let (current_weight, next_weight) =
                    person_walk_frame_weights(self.people_animation_seconds, person.id, paused);
                if current_weight > 0.0 {
                    self.draw_person_frame(sheet, pair, x, y, with_alpha(tint, current_weight));
                }
                if next_weight > 0.0 {
                    self.draw_person_frame(sheet, pair + 1, x, y, with_alpha(tint, next_weight));
                }
            } else {
                self.draw_person_frame(sheet, pair, x, y, tint);
            }
        }
    }

    fn elevator_queue_draw_x(&self, shaft_id: u64, rank: usize) -> Option<f32> {
        let shaft = self
            .traffic
            .elevators()
            .iter()
            .find(|shaft| shaft.id == shaft_id)?;
        let rank = rank.min(MAX_ELEVATOR_QUEUE_PER_FLOOR.saturating_sub(1));
        let shaft_left = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
        let shaft_right = shaft_left + f32::from(shaft.kind.spec().width) * CELL_WIDTH;
        let left_cells = usize::from(shaft.x);
        let right_cells = usize::from(
            self.tower
                .width()
                .saturating_sub(shaft.x + shaft.kind.spec().width),
        );
        Some(
            if left_cells >= right_cells.min(MAX_ELEVATOR_QUEUE_PER_FLOOR) {
                shaft_left - (rank as f32 + 1.0) * CELL_WIDTH
            } else {
                shaft_right + rank as f32 * CELL_WIDTH
            },
        )
    }

    fn draw_mode_overlays(&self) {
        if self.view_mode == ViewMode::Edit {
            return;
        }
        let evaluation =
            (self.view_mode == ViewMode::Evaluation).then(|| self.tenant_evaluation_context());
        for facility in self
            .tower
            .facilities()
            .iter()
            .filter(|facility| self.facility_is_visible(facility))
        {
            let Some(color) = self.facility_mode_color(facility, evaluation.as_ref()) else {
                continue;
            };
            let width = f32::from(facility.kind.spec().width) * CELL_WIDTH;
            let height = f32::from(facility.kind.spec().height) * FLOOR_HEIGHT;
            let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
            let y = floor_top_y(self.ground_y(), facility.position.floor) - height + FLOOR_HEIGHT;
            draw_rectangle(x, y, width, height, with_alpha(color, 0.16));
            draw_rectangle_lines(x, y, width, height, 2.0, color);
        }
    }

    fn facility_mode_color(
        &self,
        facility: &Facility,
        evaluation: Option<&TenantEvaluationContext>,
    ) -> Option<Color> {
        match self.view_mode {
            ViewMode::Edit => None,
            ViewMode::Evaluation if facility.kind.has_tenant_occupancy() => Some(match evaluation
                .map_or_else(
                    || self.tenant_evaluation_score(facility),
                    |context| self.tenant_evaluation_score_with_context(facility, context),
                ) {
                70..=100 => Color::from_rgba(45, 105, 225, 255),
                40..=69 => Color::from_rgba(235, 196, 35, 255),
                _ => Color::from_rgba(220, 45, 45, 255),
            }),
            ViewMode::Evaluation => None,
            ViewMode::Pricing if facility.kind.has_tenant_occupancy() => {
                Some(match facility.price_level {
                    0 => Color::from_rgba(45, 105, 225, 255),
                    1 => Color::from_rgba(235, 196, 35, 255),
                    2 => Color::from_rgba(235, 125, 35, 255),
                    _ => Color::from_rgba(220, 45, 45, 255),
                })
            }
            ViewMode::Pricing => None,
            ViewMode::Hotel if is_hotel_room(facility.kind) => {
                Some(if facility.is_hotel_infested() {
                    Color::from_rgba(35, 25, 20, 255)
                } else if facility.is_hotel_dirty() {
                    Color::from_rgba(220, 45, 45, 255)
                } else if facility.is_occupied() {
                    Color::from_rgba(45, 105, 225, 255)
                } else {
                    Color::from_rgba(110, 115, 120, 255)
                })
            }
            ViewMode::Hotel => None,
        }
    }

    fn tenant_evaluation_score(&self, facility: &Facility) -> u8 {
        let context = self.tenant_evaluation_context();
        self.tenant_evaluation_score_with_context(facility, &context)
    }

    fn tenant_evaluation_context(&self) -> TenantEvaluationContext {
        let mut mood_penalties = HashMap::new();
        for person in self.traffic.people() {
            *mood_penalties
                .entry(person.destination_facility_id)
                .or_insert(0) += match person.mood {
                PersonMood::Calm => 0,
                PersonMood::Concerned => 6,
                PersonMood::Angry => 14,
            };
        }
        let noisy_sources = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| {
                matches!(
                    facility.kind,
                    FacilityKind::Cinema
                        | FacilityKind::PartyHall
                        | FacilityKind::Metro
                        | FacilityKind::Recycling
                )
            })
            .map(|facility| (i32::from(facility.position.x), facility.position.floor))
            .collect();
        let recycling_capacity = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind == FacilityKind::Recycling)
            .count()
            * 2_500;
        TenantEvaluationContext {
            mood_penalties,
            noisy_sources,
            recycling_capacity,
        }
    }

    fn tenant_evaluation_score_with_context(
        &self,
        facility: &Facility,
        context: &TenantEvaluationContext,
    ) -> u8 {
        if facility.is_burned()
            || facility.is_hotel_infested()
            || facility.is_hotel_dirty()
            || !facility.is_occupied()
            || !self.traffic.reachable_facility_set().contains(&facility.id)
        {
            return 0;
        }

        let mut score = 80_i32;
        score += match facility.price_level {
            0 => 10,
            1 => 0,
            2 => -12,
            _ => -28,
        };
        score -= context
            .mood_penalties
            .get(&facility.id)
            .copied()
            .unwrap_or(0);

        let center = i32::from(facility.position.x) + i32::from(facility.kind.spec().width) / 2;
        let noisy_neighbor = context.noisy_sources.iter().any(|(x, floor)| {
            (*floor - facility.position.floor).abs() <= 1 && (*x - center).abs() <= 24
        });
        if noisy_neighbor {
            score -= 18;
        }

        if self.cached_population as usize > context.recycling_capacity {
            score -= 25;
        }

        score -= i32::from(
            self.traffic
                .access_distance_penalty(&self.tower, facility.id),
        );

        if facility.kind == FacilityKind::HotelSuite && self.parking_state.connected.is_empty() {
            score -= 20;
        }

        if matches!(
            facility.kind,
            FacilityKind::Restaurant
                | FacilityKind::FastFood
                | FacilityKind::Shop
                | FacilityKind::Cinema
                | FacilityKind::PartyHall
        ) {
            let capacity = facility.kind.population_capacity().max(1);
            let patronage = self.economic_patronage(facility, true);
            if patronage.saturating_mul(100) / capacity < 20 {
                score -= 25;
            }
        }
        score.clamp(0, 100) as u8
    }

    fn apply_quarterly_evaluation(&mut self) -> usize {
        let day = self.tower.clock.day;
        let evaluation = self.tenant_evaluation_context();
        let vacancies = self
            .tower
            .facilities()
            .iter()
            .filter(|facility| {
                facility.kind.has_tenant_occupancy()
                    && !is_hotel_room(facility.kind)
                    && facility.is_occupied()
                    && self.tenant_evaluation_score_with_context(facility, &evaluation) < 25
                    && (facility.id + u64::from(day)).is_multiple_of(3)
            })
            .map(|facility| facility.id)
            .collect::<Vec<_>>();
        for facility_id in &vacancies {
            self.tower.vacate_tenant(*facility_id, 12 * 60);
            self.traffic.evacuate_facility(*facility_id);
        }
        vacancies.len()
    }

    fn draw_tower_overview(&self) {
        let panel = overview_panel_rect();
        draw_classic_panel(panel);
        draw_rectangle(
            panel.x + 3.0,
            panel.y + 3.0,
            panel.w - 6.0,
            21.0,
            TITLE_BLUE,
        );
        draw_text("TOWER OVERVIEW", panel.x + 8.0, panel.y + 18.0, 13.0, WHITE);
        for (index, label) in MODE_MENU_LABELS.iter().enumerate() {
            let tab = overview_tab_rect(index);
            let selected = self.view_mode.menu_index() == index;
            draw_classic_button(tab, selected);
            draw_centered_text(label, tab.x + tab.w * 0.5, tab.y + 16.0, 11, BLACK);
        }

        let map = overview_map_rect();
        draw_rectangle(
            map.x,
            map.y,
            map.w,
            map.h,
            Color::from_rgba(65, 178, 226, 255),
        );
        let x_scale = map.w / f32::from(self.tower.width());
        let floor_count = (simtower_core::floor_ordinal(MAX_FLOOR)
            - simtower_core::floor_ordinal(MIN_FLOOR)
            + 1) as f32;
        let y_scale = map.h / floor_count;
        let top_ordinal = simtower_core::floor_ordinal(MAX_FLOOR);
        for floor in self.tower.floors() {
            let x = map.x + f32::from(floor.x) * x_scale;
            let y = map.y
                + f32::from(top_ordinal - simtower_core::floor_ordinal(floor.floor)) * y_scale;
            draw_rectangle(
                x,
                y,
                x_scale.max(1.0),
                y_scale.max(1.0),
                Color::from_rgba(38, 55, 61, 255),
            );
        }
        let evaluation =
            (self.view_mode == ViewMode::Evaluation).then(|| self.tenant_evaluation_context());
        for facility in self.tower.facilities() {
            let Some(color) = self.facility_mode_color(facility, evaluation.as_ref()) else {
                continue;
            };
            let x = map.x + f32::from(facility.position.x) * x_scale;
            let top = simtower_core::floor_ordinal(facility.position.floor)
                + i16::try_from(facility.height()).unwrap_or(1)
                - 1;
            let y = map.y + f32::from(top_ordinal - top) * y_scale;
            draw_rectangle(
                x,
                y,
                (f32::from(facility.kind.spec().width) * x_scale).max(1.0),
                (f32::from(facility.height()) * y_scale).max(1.0),
                color,
            );
        }

        let visible_columns = screen_width() / CELL_WIDTH;
        let visible_floors = (screen_height() - TOPBAR_HEIGHT) / FLOOR_HEIGHT;
        let viewport = Rect::new(
            map.x + self.camera_x * x_scale,
            map.y
                + f32::from(
                    top_ordinal - simtower_core::floor_ordinal(self.camera_floor.round() as i16),
                ) * y_scale
                - visible_floors * y_scale * 0.7,
            (visible_columns * x_scale).min(map.w),
            (visible_floors * y_scale).min(map.h),
        );
        draw_rectangle_lines(viewport.x, viewport.y, viewport.w, viewport.h, 1.0, WHITE);
        draw_rectangle_lines(map.x, map.y, map.w, map.h, 1.0, CLASSIC_DARK);

        let legend_y = panel.y + panel.h - 19.0;
        for (index, (label, color)) in [
            ("Good", Color::from_rgba(45, 105, 225, 255)),
            ("Average", Color::from_rgba(235, 196, 35, 255)),
            ("Poor", Color::from_rgba(220, 45, 45, 255)),
        ]
        .iter()
        .enumerate()
        {
            let x = panel.x + 8.0 + index as f32 * 74.0;
            draw_rectangle(x, legend_y - 9.0, 9.0, 9.0, *color);
            draw_text(label, x + 12.0, legend_y, 10.0, BLACK);
        }
    }

    fn handle_overview_click(&mut self, point: Vec2) {
        for index in 0..MODE_MENU_LABELS.len() {
            if overview_tab_rect(index).contains(point)
                && let Some(mode) = ViewMode::from_menu_index(index)
            {
                self.select_view_mode(mode);
                return;
            }
        }
        let map = overview_map_rect();
        if !map.contains(point) {
            return;
        }
        let x_fraction = ((point.x - map.x) / map.w).clamp(0.0, 1.0);
        self.camera_x = (x_fraction * f32::from(self.tower.width())
            - screen_width() / CELL_WIDTH * 0.5)
            .clamp(0.0, f32::from(self.tower.width()) - 1.0);
        let top = f32::from(simtower_core::floor_ordinal(MAX_FLOOR));
        let bottom = f32::from(simtower_core::floor_ordinal(MIN_FLOOR));
        let ordinal = top - ((point.y - map.y) / map.h).clamp(0.0, 1.0) * (top - bottom);
        self.camera_floor = f32::from(simtower_core::floor_from_ordinal(ordinal.round() as i16));
    }

    fn draw_person_frame(&self, sheet: usize, frame: u64, x: f32, y: f32, tint: Color) {
        draw_texture_ex(
            &self.assets.people[sheet],
            x,
            y,
            tint,
            DrawTextureParams {
                dest_size: Some(vec2(8.0, PERSON_SPRITE_HEIGHT)),
                source: Some(Rect::new(
                    frame as f32 * 8.0,
                    0.0,
                    8.0,
                    PERSON_SPRITE_HEIGHT,
                )),
                ..Default::default()
            },
        );
    }

    fn draw_security_team_frame(&self, source_x: f32, x: f32, y: f32) {
        draw_texture_ex(
            &self.assets.people_silhouette[1],
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(16.0, FLOOR_HEIGHT)),
                source: Some(Rect::new(source_x, 0.0, 16.0, FLOOR_HEIGHT)),
                ..Default::default()
            },
        );
    }

    fn draw_queue_person(&self, mood: PersonMood, id: u64, x: f32, y: f32) {
        let sheet = queue_sheet_for_mood(mood);
        let frame = (id % 8) as f32;
        draw_texture_ex(
            &self.assets.queue_people[sheet],
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(8.0, PERSON_SPRITE_HEIGHT)),
                source: Some(Rect::new(frame * 12.0, 0.0, 12.0, PERSON_SPRITE_HEIGHT)),
                ..Default::default()
            },
        );
    }

    fn draw_silhouette_person(&self, id: u64, x: f32, y: f32) {
        let sheet = usize::from(id.is_multiple_of(11));
        let texture = &self.assets.people_silhouette[sheet];
        let frame_count = (texture.width() as u64 / 16).max(1);
        let frame = (id % frame_count) as f32;
        draw_texture_ex(
            texture,
            x,
            y - 4.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(10.0, 28.0)),
                source: Some(Rect::new(frame * 16.0, 0.0, 16.0, 36.0)),
                ..Default::default()
            },
        );
    }

    fn person_has_floor_below(&self, x: f32, floor: i16) -> bool {
        if x < 0.0 || x >= f32::from(self.tower.width()) {
            return false;
        }
        self.tower.has_floor(GridPosition {
            x: x.floor() as u16,
            floor,
        })
    }

    fn draw_elevator_cars(&self) {
        for shaft in self.traffic.elevators() {
            let (texture, frame_width, load_frames) = match shaft.kind {
                FacilityKind::Elevator => (&self.assets.elevator_cars, 32.0, 4),
                FacilityKind::ServiceElevator => {
                    (&self.assets.facilities.service_elevator.texture, 32.0, 5)
                }
                FacilityKind::ExpressElevator => {
                    (&self.assets.facilities.express_elevator.texture, 48.0, 5)
                }
                _ => continue,
            };
            for car in &shaft.cars {
                let passenger_count = car.passengers.len();
                let frame = if passenger_count == 0 {
                    0
                } else {
                    (passenger_count * load_frames)
                        .div_ceil(simtower_core::elevator_capacity(shaft.kind))
                        .saturating_sub(1)
                        .min(load_frames - 1)
                };
                let (car_texture, source) =
                    if passenger_count == 0 && shaft.kind == FacilityKind::Elevator {
                        (
                            &self.assets.facilities.elevator.texture,
                            self.assets.facilities.elevator.source,
                        )
                    } else {
                        (
                            texture,
                            Rect::new(frame as f32 * frame_width, 0.0, frame_width, FLOOR_HEIGHT),
                        )
                    };
                // All cars share the shaft origin. Offsetting later cars made
                // them drift outside the shaft whenever they occupied
                // different floors.
                let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
                let y = floor_position_top_y(self.ground_y(), car.floor_position);
                draw_texture_ex(
                    car_texture,
                    x,
                    y,
                    Color::new(1.0, 1.0, 1.0, elevator_infrastructure_alpha(shaft.visible)),
                    DrawTextureParams {
                        dest_size: Some(vec2(frame_width, FLOOR_HEIGHT)),
                        source: Some(source),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_disabled_elevator_outlines(&self) {
        for shaft in self
            .traffic
            .elevators()
            .iter()
            .filter(|shaft| !shaft.visible && !shaft.served_floors.is_empty())
        {
            let bottom = *shaft.served_floors.first().expect("checked above");
            let top = *shaft.served_floors.last().expect("checked above");
            let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
            let y = floor_top_y(self.ground_y(), top);
            let width = f32::from(shaft.kind.spec().width) * CELL_WIDTH;
            let height = f32::from(
                simtower_core::floor_ordinal(top) - simtower_core::floor_ordinal(bottom) + 1,
            ) * FLOOR_HEIGHT;
            draw_rectangle_lines(x, y, width, height, 1.0, hidden_elevator_outline_color());
        }
    }

    fn draw_elevator_endcaps(&self) {
        for shaft in self
            .traffic
            .elevators()
            .iter()
            .filter(|shaft| !shaft.served_floors.is_empty())
        {
            let top = *shaft.served_floors.last().expect("checked above");
            let bottom = *shaft.served_floors.first().expect("checked above");
            let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
            let width = f32::from(shaft.kind.spec().width) * CELL_WIDTH;
            let tint = Color::new(1.0, 1.0, 1.0, elevator_infrastructure_alpha(shaft.visible));
            let (texture, frame_width, up_frame, down_frame) = match shaft.kind {
                FacilityKind::Elevator => (&self.assets.elevator_cars, 32.0, 4.0, 5.0),
                FacilityKind::ExpressElevator => (
                    &self.assets.facilities.express_elevator.texture,
                    48.0,
                    5.0,
                    6.0,
                ),
                // The service car family has no endpoint artwork; the standard
                // 32-pixel machinery caps exactly match its shaft width.
                FacilityKind::ServiceElevator => (&self.assets.elevator_cars, 32.0, 4.0, 5.0),
                _ => continue,
            };
            // Machinery lives in the unserved slot immediately beyond each
            // end of the shaft. Even a one-floor elevator therefore shows a
            // complete upper and lower machine/arrow rather than two cropped
            // halves drawn over the car.
            for (floor, frame) in [
                (floor_above(top), up_frame),
                (floor_below(bottom), down_frame),
            ] {
                draw_texture_ex(
                    texture,
                    x,
                    floor_top_y(self.ground_y(), floor),
                    tint,
                    DrawTextureParams {
                        dest_size: Some(vec2(width, FLOOR_HEIGHT)),
                        source: Some(Rect::new(
                            frame * frame_width,
                            0.0,
                            frame_width,
                            FLOOR_HEIGHT,
                        )),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_elevator_panel(&self) {
        let Some(panel) = self.elevator_panel else {
            return;
        };
        let Some(shaft) = self
            .traffic
            .elevators()
            .iter()
            .find(|shaft| shaft.id == panel.shaft_id)
        else {
            return;
        };
        let rect = elevator_panel_rect();
        draw_rectangle(
            0.0,
            TOPBAR_HEIGHT,
            ui_screen_width(),
            ui_screen_height() - TOPBAR_HEIGHT,
            Color::from_rgba(0, 0, 0, 80),
        );
        draw_classic_panel(rect);
        draw_rectangle(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 20.0, TITLE_BLUE);
        draw_text("Elevator", rect.x + 8.0, rect.y + 18.0, 15.0, WHITE);

        let wd = Rect::new(rect.x + 58.0, rect.y + 29.0, 70.0, 24.0);
        let we = Rect::new(rect.x + 132.0, rect.y + 29.0, 70.0, 24.0);
        draw_classic_button(wd, !panel.weekend);
        draw_classic_button(we, panel.weekend);
        draw_text("WD", wd.x + 24.0, wd.y + 17.0, 14.0, BLACK);
        draw_text("WE", we.x + 24.0, we.y + 17.0, 14.0, BLACK);

        let periods = ["7-12", "12-1", "1-5", "5-9", "9-12", "12-7"];
        for (index, label) in periods.iter().enumerate() {
            let button = elevator_period_rect(rect, index);
            draw_classic_button(button, panel.period == index);
            let color = match index {
                0 | 1 => Color::from_rgba(112, 205, 244, 255),
                2 | 3 => Color::from_rgba(245, 214, 71, 255),
                _ => Color::from_rgba(62, 79, 150, 255),
            };
            draw_circle(button.x + button.w * 0.5, button.y + 7.0, 3.0, color);
            draw_text(label, button.x + 3.0, button.y + 20.0, 10.0, BLACK);
        }

        let schedule = shaft.schedules[panel.period + if panel.weekend { 6 } else { 0 }];
        let mode_rect = Rect::new(rect.x + 56.0, rect.y + 87.0, 148.0, 23.0);
        draw_text("Mode", rect.x + 12.0, rect.y + 103.0, 13.0, BLACK);
        draw_classic_button(mode_rect, false);
        let mode_label = match schedule.mode {
            ElevatorMode::Local => "Local",
            ElevatorMode::ExpressToTop => "Express to Top",
            ElevatorMode::ExpressToBottom => "Express to Bottom",
        };
        draw_text(
            mode_label,
            mode_rect.x + 8.0,
            mode_rect.y + 16.0,
            13.0,
            BLACK,
        );

        draw_classic_group(
            Rect::new(rect.x + 10.0, rect.y + 116.0, rect.w - 20.0, 52.0),
            "Waiting Car Response",
        );
        draw_text(
            "Floors closer than moving cars",
            rect.x + 51.0,
            rect.y + 151.0,
            11.0,
            BLACK,
        );
        draw_spinner(rect.x + 18.0, rect.y + 137.0, schedule.waiting_car_response);

        draw_classic_group(
            Rect::new(rect.x + 10.0, rect.y + 174.0, rect.w - 20.0, 52.0),
            "Standard Floor Departure",
        );
        draw_text(
            "Seconds to wait before departing",
            rect.x + 51.0,
            rect.y + 209.0,
            11.0,
            BLACK,
        );
        draw_spinner(rect.x + 18.0, rect.y + 195.0, schedule.departure_seconds);

        let grid = Rect::new(rect.x + 12.0, rect.y + 235.0, 170.0, 184.0);
        draw_rectangle(grid.x, grid.y, grid.w, grid.h, WHITE);
        draw_rectangle_lines(grid.x, grid.y, grid.w, grid.h, 1.0, BLACK);
        draw_text("Floor", grid.x + 3.0, grid.y + 13.0, 11.0, BLACK);
        for (index, _) in shaft.cars.iter().enumerate() {
            draw_text(
                format!("Car {}", index + 1),
                grid.x + 49.0 + index as f32 * 35.0,
                grid.y + 13.0,
                10.0,
                BLACK,
            );
        }
        let mut floors = shaft.served_floors.clone();
        floors.sort_by_key(|floor| std::cmp::Reverse(simtower_core::floor_ordinal(*floor)));
        for (row, floor) in floors.iter().take(12).enumerate() {
            let row_y = grid.y + 18.0 + row as f32 * 13.0;
            draw_rectangle(grid.x + 1.0, row_y, 42.0, 12.0, BLACK);
            draw_text(
                format_floor(*floor),
                grid.x + 6.0,
                row_y + 10.0,
                10.0,
                WHITE,
            );
            draw_line(
                grid.x + 43.0,
                row_y,
                grid.x + grid.w - 1.0,
                row_y,
                1.0,
                CLASSIC_SHADOW,
            );
            for (car_index, car) in shaft.cars.iter().enumerate() {
                let cell_x = grid.x + 50.0 + car_index as f32 * 35.0;
                draw_rectangle_lines(cell_x, row_y, 24.0, 12.0, 1.0, CLASSIC_SHADOW);
                if car.home_floor == *floor {
                    draw_rectangle_lines(cell_x + 4.0, row_y + 2.0, 8.0, 8.0, 1.0, BLACK);
                }
                if car.target_floor == Some(*floor) {
                    draw_circle(cell_x + 17.0, row_y + 6.0, 3.0, BLACK);
                }
                if simtower_core::floor_from_ordinal(car.floor_position.round() as i16) == *floor {
                    let marker = match car.direction {
                        ElevatorDirection::Up => "^",
                        ElevatorDirection::Down => "v",
                        ElevatorDirection::Idle => "o",
                    };
                    draw_text(marker, cell_x + 7.0, row_y + 10.0, 10.0, RED);
                }
            }
        }

        draw_text("Show", rect.x + 194.0, rect.y + 255.0, 12.0, BLACK);
        let show_on = Rect::new(rect.x + 190.0, rect.y + 262.0, 34.0, 21.0);
        let show_off = Rect::new(rect.x + 226.0, rect.y + 262.0, 34.0, 21.0);
        draw_classic_button(show_on, shaft.visible);
        draw_classic_button(show_off, !shaft.visible);
        draw_text("On", show_on.x + 8.0, show_on.y + 15.0, 11.0, BLACK);
        draw_text("Off", show_off.x + 7.0, show_off.y + 15.0, 11.0, BLACK);
        draw_text(
            format!("Waiting: {}", shaft.waiting_count(self.traffic.people())),
            rect.x + 190.0,
            rect.y + 305.0,
            11.0,
            BLACK,
        );
        draw_text(
            format!("Capacity: {}", simtower_core::elevator_capacity(shaft.kind)),
            rect.x + 190.0,
            rect.y + 321.0,
            11.0,
            BLACK,
        );

        let simulate = Rect::new(rect.x + 12.0, rect.y + rect.h - 32.0, 82.0, 22.0);
        let ok = Rect::new(rect.x + rect.w - 66.0, rect.y + rect.h - 32.0, 54.0, 22.0);
        draw_classic_button(simulate, panel.simulate);
        draw_classic_button(ok, false);
        draw_text(
            if panel.simulate { "Resume" } else { "Simulate" },
            simulate.x + 10.0,
            simulate.y + 15.0,
            12.0,
            BLACK,
        );
        draw_text("OK", ok.x + 18.0, ok.y + 15.0, 12.0, BLACK);
    }

    fn draw_finance_panel(&self) {
        let rect = finance_panel_rect();
        draw_rectangle(
            0.0,
            TOPBAR_HEIGHT,
            ui_screen_width(),
            ui_screen_height() - TOPBAR_HEIGHT,
            Color::from_rgba(0, 0, 0, 80),
        );
        draw_texture_ex(
            &self.assets.finance_dialog,
            rect.x,
            rect.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(rect.w, rect.h)),
                ..Default::default()
            },
        );
        let current_population = self.population_by_kind();
        let current_period = self.tower.clock.day.saturating_sub(1) / 3;
        let report = self.finance_ledger.last_report.as_ref();
        let year = report.map_or(current_period / 4 + 1, |report| report.year);
        let quarter = report.map_or((current_period % 4) as u8 + 1, |report| report.quarter);
        let income = report.map_or(self.finance_ledger.tenant_income, |report| {
            report.total_income
        });
        let maintenance = report.map_or(self.finance_ledger.maintenance, |report| {
            report.total_maintenance
        });
        let construction = report.map_or(self.finance_ledger.construction_costs, |report| {
            report.construction_costs
        });
        let other_income = report.map_or(self.finance_ledger.other_income, |report| {
            report.other_income
        });
        let starting_balance = report.map_or(self.finance_ledger.period_start_balance, |report| {
            report.starting_balance
        });
        let ending_balance = report.map_or(self.tower.cash(), |report| report.ending_balance);
        let income_by_kind = report.map_or(&self.finance_ledger.income_by_kind, |report| {
            &report.income_by_kind
        });
        let maintenance_by_kind = report
            .map_or(&self.finance_ledger.maintenance_by_kind, |report| {
                &report.maintenance_by_kind
            });
        let population_by_kind =
            report.map_or(&current_population, |report| &report.population_by_kind);
        let net = income.saturating_sub(maintenance);
        let period = finance_period_label(year, quarter);
        draw_rectangle(
            rect.x + 20.0,
            rect.y + 4.0,
            rect.w - 40.0,
            21.0,
            // Bitmap 500 uses #cccccc throughout its report face. Matching
            // it keeps this replacement heading from looking pasted on.
            PROMOTION_FACE,
        );
        draw_centered_text(&period, rect.x + rect.w * 0.5, rect.y + 20.0, 16, BLACK);
        draw_right_aligned_text(
            &format_currency(income),
            rect.x + 166.0,
            rect.y + 53.0,
            13,
            BLACK,
        );
        draw_right_aligned_text(
            &format_currency(maintenance),
            rect.x + 319.0,
            rect.y + 53.0,
            13,
            BLACK,
        );

        // The row labels are baked into bitmap 500 at a very small size.
        // Clear only their cells, preserving the original table geometry,
        // then redraw them alongside the live values at a readable size.
        draw_rectangle(
            rect.x + 20.0,
            rect.y + 80.0,
            65.0,
            130.0,
            FINANCE_TABLE_FACE,
        );
        draw_rectangle(
            rect.x + 191.0,
            rect.y + 80.0,
            75.0,
            130.0,
            FINANCE_TABLE_FACE,
        );
        for (index, (kind, label)) in [
            (FacilityKind::Office, "Office"),
            (FacilityKind::HotelSingle, "Single Room"),
            (FacilityKind::HotelTwin, "Twin Room"),
            (FacilityKind::HotelSuite, "Hotel Suite"),
            (FacilityKind::Shop, "Shops"),
            (FacilityKind::FastFood, "Fast Food"),
            (FacilityKind::Restaurant, "Restaurant"),
            (FacilityKind::PartyHall, "Party Hall"),
            (FacilityKind::Cinema, "Theater"),
            (FacilityKind::Condo, "Condo"),
        ]
        .into_iter()
        .enumerate()
        {
            let y = rect.y + 91.0 + index as f32 * 12.4;
            draw_text(label, rect.x + 24.0, y, 10.0, BLACK);
            draw_right_aligned_text(
                &population_by_kind
                    .get(&kind)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                rect.x + 116.0,
                y,
                10,
                BLACK,
            );
            draw_right_aligned_text(
                &format_number(income_by_kind.get(&kind).copied().unwrap_or(0)),
                rect.x + 177.0,
                y,
                10,
                BLACK,
            );
        }
        for (index, (kind, label)) in [
            (FacilityKind::Lobby, "Lobby"),
            (FacilityKind::Elevator, "Elevator"),
            (FacilityKind::ExpressElevator, "Exp Elevator"),
            (FacilityKind::ServiceElevator, "Ser Elevator"),
            (FacilityKind::Escalator, "Escalator"),
            (FacilityKind::Ramp, "Parking Ramp"),
            (FacilityKind::Recycling, "Recycling Center"),
            (FacilityKind::Metro, "Metro Station"),
            (FacilityKind::Housekeeping, "Housekeeping"),
            (FacilityKind::Security, "Security"),
        ]
        .into_iter()
        .enumerate()
        {
            let y = rect.y + 91.0 + index as f32 * 12.4;
            draw_fitted_text(label, rect.x + 195.0, y, 69.0, 10, BLACK);
            draw_right_aligned_text(
                &format_number(maintenance_by_kind.get(&kind).copied().unwrap_or(0)),
                rect.x + 319.0,
                y,
                10,
                BLACK,
            );
        }
        for (value, y) in [
            (format_currency(net), 243.0),
            (format_currency(other_income), 259.0),
            (
                format_currency(construction.saturating_abs().saturating_neg()),
                275.0,
            ),
            (format_currency(starting_balance), 291.0),
            (format_currency(ending_balance), 309.0),
        ] {
            draw_right_aligned_text(&value, rect.x + 319.0, rect.y + y, 13, BLACK);
        }
        draw_rectangle(
            rect.x + 20.0,
            rect.y + 214.0,
            rect.w - 40.0,
            13.0,
            PROMOTION_FACE,
        );
        draw_centered_text(
            "(Items with no income or expenses are not displayed)",
            rect.x + rect.w * 0.5,
            rect.y + 224.0,
            10,
            BLACK,
        );
        let ok = finance_ok_rect();
        draw_classic_button(ok, false);
        draw_centered_text("OK", ok.x + ok.w * 0.5, ok.y + 17.0, 14, BLACK);
    }

    fn handle_elevator_panel_click(&mut self, point: Vec2) {
        let Some(mut panel) = self.elevator_panel else {
            return;
        };
        let rect = elevator_panel_rect();
        if !rect.contains(point) {
            self.close_elevator_controls();
            return;
        }
        if Rect::new(rect.x + 58.0, rect.y + 29.0, 70.0, 24.0).contains(point) {
            panel.weekend = false;
        } else if Rect::new(rect.x + 132.0, rect.y + 29.0, 70.0, 24.0).contains(point) {
            panel.weekend = true;
        } else if let Some(period) =
            (0..6).find(|index| elevator_period_rect(rect, *index).contains(point))
        {
            panel.period = period;
        } else if Rect::new(rect.x + rect.w - 66.0, rect.y + rect.h - 32.0, 54.0, 22.0)
            .contains(point)
        {
            self.close_elevator_controls();
            return;
        } else if Rect::new(rect.x + 12.0, rect.y + rect.h - 32.0, 82.0, 22.0).contains(point) {
            panel.simulate = !panel.simulate;
            self.status = if panel.simulate {
                "Elevator traffic simulation view enabled"
            } else {
                "Elevator traffic simulation view disabled"
            }
            .to_owned();
        } else if let Some(shaft) = self.traffic.elevator_mut(panel.shaft_id) {
            let schedule_index = panel.period + if panel.weekend { 6 } else { 0 };
            let schedule = &mut shaft.schedules[schedule_index];
            if Rect::new(rect.x + 56.0, rect.y + 87.0, 148.0, 23.0).contains(point) {
                schedule.mode = match schedule.mode {
                    ElevatorMode::Local => ElevatorMode::ExpressToTop,
                    ElevatorMode::ExpressToTop => ElevatorMode::ExpressToBottom,
                    ElevatorMode::ExpressToBottom => ElevatorMode::Local,
                };
            } else if Rect::new(rect.x + 18.0, rect.y + 137.0, 15.0, 22.0).contains(point) {
                schedule.waiting_car_response = schedule.waiting_car_response.saturating_sub(1);
            } else if Rect::new(rect.x + 33.0, rect.y + 137.0, 15.0, 22.0).contains(point) {
                schedule.waiting_car_response = (schedule.waiting_car_response + 1).min(30);
            } else if Rect::new(rect.x + 18.0, rect.y + 195.0, 15.0, 22.0).contains(point) {
                schedule.departure_seconds = schedule.departure_seconds.saturating_sub(5);
            } else if Rect::new(rect.x + 33.0, rect.y + 195.0, 15.0, 22.0).contains(point) {
                schedule.departure_seconds = (schedule.departure_seconds + 5).min(60);
            } else if Rect::new(rect.x + 190.0, rect.y + 262.0, 34.0, 21.0).contains(point) {
                shaft.visible = true;
            } else if Rect::new(rect.x + 226.0, rect.y + 262.0, 34.0, 21.0).contains(point) {
                shaft.visible = false;
            } else {
                let grid = Rect::new(rect.x + 12.0, rect.y + 235.0, 170.0, 184.0);
                if point.x >= grid.x + 50.0 && point.x < grid.x + 74.0 && point.y >= grid.y + 18.0 {
                    let row = ((point.y - grid.y - 18.0) / 13.0).floor() as usize;
                    let mut floors = shaft.served_floors.clone();
                    floors.sort_by_key(|floor| {
                        std::cmp::Reverse(simtower_core::floor_ordinal(*floor))
                    });
                    if let (Some(car), Some(floor)) = (shaft.cars.first_mut(), floors.get(row)) {
                        car.home_floor = *floor;
                    }
                }
            }
        }
        self.elevator_panel = Some(panel);
    }

    fn draw_topbar(&self) {
        draw_rectangle(0.0, 0.0, ui_screen_width(), TOPBAR_HEIGHT, CLASSIC_FACE);
        draw_line(
            0.0,
            TOPBAR_HEIGHT - 1.0,
            ui_screen_width(),
            TOPBAR_HEIGHT - 1.0,
            2.0,
            CLASSIC_DARK,
        );
        let menu_button = game_menu_button_rect();
        draw_classic_button(menu_button, self.game_menu_open);
        draw_text("Menu", menu_button.x + 8.0, 22.0, 15.0, BLACK);
        let mode_button = mode_menu_button_rect();
        draw_classic_button(mode_button, self.mode_menu_open);
        draw_text(
            self.view_mode.label(),
            mode_button.x + 8.0,
            22.0,
            15.0,
            BLACK,
        );
        let arrow_x = mode_button.x + mode_button.w - 13.0;
        draw_triangle(
            vec2(arrow_x, 14.0),
            vec2(arrow_x + 8.0, 14.0),
            vec2(arrow_x + 4.0, 19.0),
            BLACK,
        );
        let sound_button = sound_menu_button_rect();
        draw_classic_button(sound_button, false);
        draw_sound_icon(sound_button, self.assets.sounds.is_muted());
        let rating = self.star_rating();
        if rating >= 6 {
            let row = rating_row_rect();
            let source_width = self.assets.tower_logo.width();
            let source_height = self.assets.tower_logo.height();
            let scale = (row.h / source_height).min(row.w / source_width);
            let width = source_width * scale;
            let height = source_height * scale;
            draw_texture_ex(
                &self.assets.tower_logo,
                row.x + (row.w - width) * 0.5,
                row.y + (row.h - height) * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(width, height)),
                    ..Default::default()
                },
            );
        } else {
            for star in 0..5 {
                let rect = rating_star_rect(star);
                draw_scalable_star(rect, star < usize::from(rating));
            }
        }
        let star_row = rating_row_rect();
        let controls_right = star_row.x + star_row.w;
        let clock = self.tower.clock;
        let time = format!(
            "Day {} ({})   {:02}:{:02}",
            clock.day,
            weekday_name(clock.day),
            clock.minute_of_day / 60,
            clock.minute_of_day % 60
        );
        let time_width = measure_text(&time, None, 17, 1.0).width;
        let time_x = topbar_clock_x(ui_screen_width(), time_width, controls_right);
        draw_text(&time, time_x, 23.0, 17.0, BLACK);
        let money = format!(
            "Fund {}    Pop {}",
            format_currency(self.tower.cash()),
            self.population()
        );
        let money_width = measure_text(&money, None, 17, 1.0).width;
        let money_x = (ui_screen_width() - money_width - 12.0).max(time_x + time_width + 18.0);
        draw_text(&money, money_x, 23.0, 17.0, BLACK);
        draw_rectangle(
            6.0,
            35.0,
            ui_screen_width() - 12.0,
            22.0,
            Color::from_rgba(235, 235, 235, 255),
        );
        draw_rectangle_lines(
            6.0,
            35.0,
            ui_screen_width() - 12.0,
            22.0,
            1.0,
            CLASSIC_SHADOW,
        );
        let mode = match self.view_mode {
            ViewMode::Evaluation => "Evaluation".to_owned(),
            ViewMode::Pricing => "Rent".to_owned(),
            ViewMode::Hotel => "Hotel status".to_owned(),
            ViewMode::Edit => match self.tool {
                ToolMode::Floor => "Build: Floor".to_owned(),
                ToolMode::Build(kind) => format!("Build: {}", kind.spec().name),
                ToolMode::Inspect => "Inspect".to_owned(),
                ToolMode::Demolish => "Bulldoze".to_owned(),
            },
        };
        draw_text(
            format!("{mode}  |  {}", self.status),
            12.0,
            51.0,
            14.0,
            BLACK,
        );

        let mouse = ui_mouse_position();
        if rating < 6
            && let Some(star) = (0..5).find(|star| rating_star_rect(*star).contains(mouse))
        {
            self.draw_star_tooltip(star + 1, rating_star_rect(star).x, 31.0);
        }
    }

    fn draw_game_menu(&self) {
        let menu = game_menu_rect();
        draw_classic_panel(menu);
        let mouse = ui_mouse_position();
        for (index, (action, label)) in GAME_MENU_ACTIONS.iter().enumerate() {
            let row = game_menu_entry_rect(index);
            let hovered = row.contains(mouse)
                || (*action == GameMenuAction::UxScale && self.ux_scale_menu_open);
            let checked =
                *action == GameMenuAction::AutomaticReports && self.automatic_finance_reports;
            let checkable = *action == GameMenuAction::AutomaticReports;
            let display_label = if *action == GameMenuAction::UxScale {
                format!("UX scale: {}", self.ux_scale.label())
            } else {
                (*label).to_owned()
            };
            if hovered {
                draw_rectangle(row.x, row.y, row.w, row.h, TITLE_BLUE);
            }
            if checked {
                draw_menu_checkmark(row, if hovered { WHITE } else { BLACK });
            }
            draw_text(
                &display_label,
                row.x + if checkable { 25.0 } else { 8.0 },
                row.y + 17.0,
                14.0,
                if hovered { WHITE } else { BLACK },
            );
            if *action == GameMenuAction::UxScale {
                let arrow_color = if hovered { WHITE } else { BLACK };
                let arrow_x = row.x + row.w - 12.0;
                let arrow_y = row.y + row.h * 0.5;
                draw_triangle(
                    vec2(arrow_x, arrow_y - 4.0),
                    vec2(arrow_x, arrow_y + 4.0),
                    vec2(arrow_x + 5.0, arrow_y),
                    arrow_color,
                );
            }
        }
    }

    fn draw_ux_scale_menu(&self) {
        let menu = ux_scale_menu_rect();
        draw_classic_panel(menu);
        let mouse = ui_mouse_position();
        for (index, scale) in UX_SCALE_CHOICES.iter().copied().enumerate() {
            let row = ux_scale_menu_entry_rect(index);
            let hovered = row.contains(mouse);
            let selected = self.ux_scale == scale;
            if hovered {
                draw_rectangle(row.x, row.y, row.w, row.h, TITLE_BLUE);
            }
            if selected {
                draw_menu_checkmark(row, if hovered { WHITE } else { BLACK });
            }
            draw_text(
                scale.label(),
                row.x + 25.0,
                row.y + 17.0,
                14.0,
                if hovered { WHITE } else { BLACK },
            );
        }
    }

    fn draw_mode_menu(&self) {
        let menu = mode_menu_rect();
        draw_classic_panel(menu);
        let mouse = ui_mouse_position();
        for (index, label) in MODE_MENU_LABELS.iter().enumerate() {
            let row = mode_menu_entry_rect(index);
            let mode = ViewMode::from_menu_index(index).expect("mode menu index is valid");
            let selected = self.view_mode == mode;
            let hovered = row.contains(mouse);
            if hovered || selected {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    if hovered { TITLE_BLUE } else { CLASSIC_SHADOW },
                );
            }
            if selected {
                draw_menu_checkmark(row, if hovered { WHITE } else { BLACK });
            }
            draw_text(
                label,
                row.x + 24.0,
                row.y + 17.0,
                14.0,
                if hovered { WHITE } else { BLACK },
            );
        }
    }

    fn draw_save_name_dialog(&self) {
        let Some(dialog) = &self.save_name_dialog else {
            return;
        };
        draw_rectangle(
            0.0,
            TOPBAR_HEIGHT,
            ui_screen_width(),
            ui_screen_height() - TOPBAR_HEIGHT,
            Color::from_rgba(0, 0, 0, 90),
        );
        let rect = save_name_dialog_rect();
        draw_classic_panel(rect);
        draw_rectangle(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, 24.0, TITLE_BLUE);
        draw_text("Save Tower", rect.x + 10.0, rect.y + 20.0, 15.0, WHITE);
        if let Some(path) = &dialog.overwrite_path {
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Tower.smtower");
            draw_text(
                format!("{filename} already exists."),
                rect.x + 20.0,
                rect.y + 58.0,
                15.0,
                BLACK,
            );
            draw_text(
                "Do you want to overwrite it?",
                rect.x + 20.0,
                rect.y + 84.0,
                15.0,
                BLACK,
            );
            let yes = save_dialog_save_rect();
            let no = save_dialog_cancel_rect();
            draw_classic_button(yes, false);
            draw_classic_button(no, false);
            draw_text("Yes", yes.x + 25.0, yes.y + 17.0, 14.0, BLACK);
            draw_text("No", no.x + 28.0, no.y + 17.0, 14.0, BLACK);
            return;
        }
        draw_text("Save-game name:", rect.x + 14.0, rect.y + 49.0, 14.0, BLACK);
        let input = Rect::new(rect.x + 14.0, rect.y + 57.0, rect.w - 28.0, 27.0);
        draw_rectangle(input.x, input.y, input.w, input.h, WHITE);
        draw_rectangle_lines(input.x, input.y, input.w, input.h, 2.0, CLASSIC_DARK);
        let max_width = input.w - 12.0;
        let mut visible_name = dialog.name.as_str();
        while measure_text(visible_name, None, 16, 1.0).width > max_width {
            let Some((index, _)) = visible_name.char_indices().nth(1) else {
                break;
            };
            visible_name = &visible_name[index..];
        }
        let visible_width = measure_text(visible_name, None, 16, 1.0).width;
        if dialog.replace_default_on_input && !visible_name.is_empty() {
            draw_rectangle(
                input.x + 4.0,
                input.y + 3.0,
                visible_width + 4.0,
                input.h - 6.0,
                TITLE_BLUE,
            );
        }
        draw_text(
            visible_name,
            input.x + 6.0,
            input.y + 19.0,
            16.0,
            if dialog.replace_default_on_input {
                WHITE
            } else {
                BLACK
            },
        );
        let caret_x = input.x + 6.0 + visible_width.min(max_width);
        if !dialog.replace_default_on_input {
            draw_line(caret_x, input.y + 5.0, caret_x, input.y + 22.0, 1.0, BLACK);
        }
        if let Some(error) = &dialog.error {
            draw_text(error, rect.x + 14.0, rect.y + 105.0, 12.0, RED);
        } else if let Ok(directory) = save_game_directory() {
            let folder = format!("Folder: {}", directory.display());
            draw_text(&folder, rect.x + 14.0, rect.y + 105.0, 11.0, DARKGRAY);
        }
        let save = save_dialog_save_rect();
        let cancel = save_dialog_cancel_rect();
        draw_classic_button(save, false);
        draw_classic_button(cancel, false);
        draw_text("Save", save.x + 21.0, save.y + 17.0, 14.0, BLACK);
        draw_text("Cancel", cancel.x + 15.0, cancel.y + 17.0, 14.0, BLACK);
    }

    fn draw_star_tooltip(&self, star: usize, x: f32, y: f32) {
        let (first, second) = match star {
            1 => ("1 Star: starting rating", None),
            2 => ("2 Stars: population 300", None),
            3 => ("3 Stars: population 1,000 + a Security office", None),
            4 => (
                "4 Stars: population 5,000 + 2 suites",
                Some("Medical and recycling facilities are also required"),
            ),
            5 => (
                "5 Stars: population 10,000 + a Metro station",
                Some("Medical and recycling demands must also be fulfilled"),
            ),
            _ => return,
        };
        let font_size = 13;
        let mut width = measure_text(first, None, font_size, 1.0).width;
        if let Some(second) = second {
            width = width.max(measure_text(second, None, font_size, 1.0).width);
        }
        width += 12.0;
        let height = if second.is_some() { 38.0 } else { 22.0 };
        let x = x.min(ui_screen_width() - width - 4.0).max(4.0);
        draw_rectangle(x, y, width, height, Color::from_rgba(255, 255, 220, 250));
        draw_rectangle_lines(x, y, width, height, 1.0, BLACK);
        draw_text(first, x + 6.0, y + 15.0, font_size as f32, BLACK);
        if let Some(second) = second {
            draw_text(second, x + 6.0, y + 31.0, font_size as f32, BLACK);
        }
    }

    fn draw_palette(&self) {
        let rect = palette_rect();
        draw_classic_panel(rect);
        draw_rectangle(
            rect.x + 3.0,
            rect.y + 3.0,
            rect.w - 6.0,
            PALETTE_TITLE_HEIGHT - 4.0,
            TITLE_BLUE,
        );
        draw_text("TOOLS", rect.x + 8.0, rect.y + 16.0, 13.0, WHITE);
        for (index, button) in PALETTE_BUTTONS.iter().enumerate() {
            let button_rect = palette_button_rect(index);
            let available = match button.action {
                PaletteAction::Menu(menu) => self.star_rating() >= menu.unlock_stars(),
                _ => true,
            };
            let selected = match button.action {
                PaletteAction::Speed => self.speed_menu_open,
                PaletteAction::Inspect => self.tool == ToolMode::Inspect,
                PaletteAction::Demolish => self.tool == ToolMode::Demolish,
                PaletteAction::Menu(menu) => self.open_menu == Some(menu),
            };
            draw_classic_button(button_rect, selected && available);
            match button.icon {
                ToolIcon::Pause => {
                    let paused = self.tower.clock.speed == SimulationSpeed::Paused;
                    draw_play_pause_glyph(
                        Rect::new(
                            button_rect.x + 7.0,
                            button_rect.y + 5.0,
                            button_rect.w - 14.0,
                            button_rect.h - 12.0,
                        ),
                        paused,
                        BLACK,
                    );
                    let speed = speed_label(self.tower.clock.speed);
                    let width = measure_text(speed, None, 11, 1.0).width;
                    draw_rectangle(
                        button_rect.x + button_rect.w - width - 5.0,
                        button_rect.y + button_rect.h - 13.0,
                        width + 4.0,
                        12.0,
                        Color::from_rgba(235, 235, 235, 225),
                    );
                    draw_text(
                        speed,
                        button_rect.x + button_rect.w - width - 3.0,
                        button_rect.y + button_rect.h - 3.0,
                        11.0,
                        BLACK,
                    );
                }
                ToolIcon::Pointer(index) => {
                    let icon_width = 64.0 / 3.0;
                    let texture = if selected {
                        &self.assets.pointer_tools_selected
                    } else {
                        &self.assets.pointer_tools
                    };
                    draw_cropped_icon(
                        texture,
                        Rect::new(
                            f32::from(index) * icon_width * UI_TEXTURE_SCALE,
                            0.0,
                            icon_width * UI_TEXTURE_SCALE,
                            21.0 * UI_TEXTURE_SCALE,
                        ),
                        button_rect,
                        WHITE,
                    );
                }
                ToolIcon::Build(column, row) => {
                    let texture = if !available {
                        &self.assets.tool_palette_disabled
                    } else if selected {
                        &self.assets.tool_palette_selected
                    } else {
                        &self.assets.tool_palette
                    };
                    draw_atlas_icon(texture, column, row, button_rect, WHITE);
                }
            }
        }
    }

    fn draw_submenu(&self, menu: BuildMenu) {
        let rect = submenu_rect(menu.entries().len());
        draw_classic_panel(rect);
        draw_rectangle(
            rect.x + 3.0,
            rect.y + 3.0,
            rect.w - 6.0,
            SUBMENU_HEADER_HEIGHT - 3.0,
            TITLE_BLUE,
        );
        draw_text(menu.title(), rect.x + 7.0, rect.y + 15.0, 12.0, WHITE);
        let mouse = ui_mouse_position();
        let rating = self.star_rating();
        for (index, entry) in menu.entries().iter().enumerate() {
            let row = submenu_entry_rect(index);
            let hovered = row.contains(mouse);
            if hovered {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    Color::from_rgba(33, 63, 130, 255),
                );
            }
            let icon_palette = if !entry.is_available(rating) {
                &self.assets.tool_palette_disabled
            } else if hovered {
                &self.assets.tool_palette_selected
            } else {
                &self.assets.tool_palette
            };
            draw_atlas_icon_with_padding(
                icon_palette,
                entry.atlas_column,
                entry.atlas_row,
                Rect::new(
                    row.x + 1.0,
                    row.y + 1.0,
                    SUBMENU_ICON_SIZE,
                    SUBMENU_ICON_SIZE,
                ),
                WHITE,
                SUBMENU_ICON_PADDING,
            );
            let color = if entry.is_available(rating) {
                if hovered { WHITE } else { BLACK }
            } else if hovered {
                LIGHTGRAY
            } else {
                GRAY
            };
            draw_text(entry.label, row.x + 30.0, row.y + 12.0, 13.0, color);
            let detail = if !entry.is_available(rating) {
                format!("Requires {} stars", entry.required_stars())
            } else if entry.builds_floor {
                format!("${FLOOR_CONSTRUCTION_COST} / slice")
            } else {
                entry.kind.map_or_else(
                    || entry.disabled_reason.unwrap_or("Unavailable").to_owned(),
                    build_price_label,
                )
            };
            draw_text(&detail, row.x + 30.0, row.y + 24.0, 10.0, color);
        }
    }

    fn draw_speed_menu(&self) {
        let menu = speed_menu_rect();
        draw_classic_panel(menu);
        for (index, speed) in SPEED_CHOICES.iter().copied().enumerate() {
            let row = speed_menu_entry_rect(index);
            let selected = self.tower.clock.speed == speed;
            if selected {
                draw_rectangle(row.x, row.y, row.w, row.h, TITLE_BLUE);
            }
            if selected {
                draw_menu_checkmark(row, WHITE);
            }
            draw_speed_choice_glyph(
                Rect::new(row.x + 25.0, row.y + 5.0, 14.0, 14.0),
                speed,
                if selected { WHITE } else { BLACK },
            );
            draw_text(
                speed_label(speed),
                row.x + 44.0,
                row.y + 17.0,
                14.0,
                if selected { WHITE } else { BLACK },
            );
        }
    }

    fn draw_ui_tooltip(&self, mouse: Vec2) {
        if self.elevator_panel.is_some()
            || self.game_menu_open
            || self.mode_menu_open
            || self.save_name_dialog.is_some()
        {
            return;
        }
        if let Some(index) = palette_button_at(mouse) {
            let button = PALETTE_BUTTONS[index];
            let available = match button.action {
                PaletteAction::Menu(menu) => self.star_rating() >= menu.unlock_stars(),
                _ => true,
            };
            let label = match button.action {
                PaletteAction::Speed => {
                    format!("Simulation speed: {}", speed_label(self.tower.clock.speed))
                }
                PaletteAction::Menu(menu) if !available => {
                    format!(
                        "{} — unlocks at {} stars",
                        button.label,
                        menu.unlock_stars()
                    )
                }
                _ => button.label.to_owned(),
            };
            self.draw_tooltip(&label, palette_rect().x, palette_rect().y - 23.0);
            return;
        }
        if self.speed_menu_open
            && let Some(speed) = speed_menu_choice_at(mouse)
        {
            self.draw_tooltip(
                &format!("Set simulation to {}", speed_label(speed)),
                speed_menu_rect().x,
                speed_menu_rect().y - 23.0,
            );
            return;
        }
        if let Some((menu, index)) = self.menu_entry_at(mouse) {
            let entry = menu.entries()[index];
            let rating = self.star_rating();
            let detail = if !entry.is_available(rating) {
                format!(
                    "{} — requires {} stars",
                    entry.label,
                    entry.required_stars()
                )
            } else if entry.builds_floor {
                format!("{} — ${FLOOR_CONSTRUCTION_COST} per slice", entry.label)
            } else if entry.kind == Some(FacilityKind::Lobby) {
                format!(
                    "{} — {}; {}",
                    entry.label,
                    build_price_label(FacilityKind::Lobby),
                    LOBBY_HEIGHT_HELP
                )
            } else {
                entry.kind.map_or_else(
                    || entry.label.to_owned(),
                    |kind| format!("{} — {}", entry.label, build_price_label(kind)),
                )
            };
            self.draw_tooltip(
                &detail,
                submenu_rect(menu.entries().len()).x,
                palette_rect().y - 23.0,
            );
        }
    }

    fn draw_tooltip(&self, text: &str, x: f32, y: f32) {
        let width = measure_text(text, None, 13, 1.0).width + 10.0;
        let x = x.min(ui_screen_width() - width - 4.0).max(4.0);
        let y = y.max(TOPBAR_HEIGHT + 1.0);
        draw_rectangle(x, y, width, 20.0, Color::from_rgba(255, 255, 220, 250));
        draw_rectangle_lines(x, y, width, 20.0, 1.0, BLACK);
        draw_text(text, x + 5.0, y + 14.0, 13.0, BLACK);
    }
}

fn person_walk_frame_weights(animation_seconds: f64, person_id: u64, paused: bool) -> (f32, f32) {
    // A deliberately modest cadence keeps the original two-cel sprites
    // readable. Rendering time is independent of game speed, so 10x affects
    // travel distance without turning this into a high-frequency flicker.
    let phase =
        ((animation_seconds * 2.2 + person_id as f64 * 0.37) * std::f64::consts::TAU).cos() as f32;
    let next_weight = phase.mul_add(-0.5, 0.5);
    if paused {
        if next_weight < 0.5 {
            (1.0, 0.0)
        } else {
            (0.0, 1.0)
        }
    } else {
        (1.0 - next_weight, next_weight)
    }
}

fn visual_animation_delta(frame_dt: f32, speed: SimulationSpeed) -> f32 {
    if speed == SimulationSpeed::Paused {
        0.0
    } else {
        frame_dt
    }
}

fn world_ground_y(view_anchor_y: f32, camera_floor: f32) -> f32 {
    view_anchor_y + camera_floor * FLOOR_HEIGHT
}

fn floor_top_y(ground_y: f32, floor: i16) -> f32 {
    if floor > 0 {
        ground_y - f32::from(floor) * FLOOR_HEIGHT
    } else {
        ground_y - f32::from(floor + 1) * FLOOR_HEIGHT
    }
}

fn floor_position_top_y(ground_y: f32, floor_position: f32) -> f32 {
    ground_y - (floor_position + 1.0) * FLOOR_HEIGHT
}

fn tenant_room_top_y(ground_y: f32, floor: i16) -> f32 {
    let story_top = floor_top_y(ground_y, floor);
    if floor > 0 {
        story_top + FLOOR_HEIGHT - ROOM_HEIGHT
    } else {
        story_top
    }
}

fn utility_strip_top_y(ground_y: f32, floor: i16) -> f32 {
    let story_top = floor_top_y(ground_y, floor);
    if floor > 0 {
        story_top
    } else {
        story_top + ROOM_HEIGHT
    }
}

fn interior_person_top_y(ground_y: f32, floor_position: f32) -> f32 {
    let story_top = floor_position_top_y(ground_y, floor_position);
    // On the lobby and above, the utility band occupies the top twelve pixels
    // and the tenant/corridor occupies the lower twenty-four. Basements retain
    // their original inverse arrangement.
    let room_top = if floor_position >= 0.0 {
        story_top + FLOOR_HEIGHT - ROOM_HEIGHT
    } else {
        story_top
    };
    room_top + ROOM_HEIGHT - PERSON_SPRITE_HEIGHT
}

fn floor_above(floor: i16) -> i16 {
    if floor == -1 { 1 } else { floor + 1 }
}

fn floor_below(floor: i16) -> i16 {
    if floor == 1 { -1 } else { floor - 1 }
}

fn world_floor_at_y(ground_y: f32, screen_y: f32) -> i16 {
    let height_above_ground = (ground_y - screen_y) / FLOOR_HEIGHT;
    if height_above_ground >= 0.0 {
        height_above_ground.ceil().max(1.0) as i16
    } else {
        height_above_ground.floor() as i16
    }
}

fn sky_tile_origin_x(camera_x: f32) -> f32 {
    -(camera_x * CELL_WIDTH).rem_euclid(SKY_TILE_WIDTH) - SKY_TILE_WIDTH
}

fn city_tile_layout(view_width: f32, camera_x: f32, tile_width: f32) -> Vec<(f32, bool)> {
    if view_width <= 0.0 || tile_width <= 0.0 {
        return Vec::new();
    }
    let scroll = camera_x * CELL_WIDTH * 0.18;
    let mut tile_index = (scroll / tile_width).floor() as i64;
    let mut x = -scroll.rem_euclid(tile_width);
    let mut layout = Vec::new();
    while x < view_width {
        layout.push((x, tile_index.rem_euclid(2) != 0));
        x += tile_width;
        tile_index += 1;
    }
    layout
}

fn dragged_columns(previous: u16, current: u16) -> Vec<u16> {
    if previous < current {
        (previous + 1..=current).collect()
    } else {
        (current..previous).rev().collect()
    }
}

struct FillPlan {
    columns: Vec<u16>,
    limiting_error: Option<PlacementError>,
}

struct TenantFillPlan {
    positions: Vec<GridPosition>,
    limiting_error: Option<PlacementError>,
}

fn floor_fill_plan(tower: &Tower, floor: i16) -> FillPlan {
    let mut preview = tower.clone();
    let mut columns = Vec::new();
    let mut limiting_error = None;
    for x in 0..tower.width() {
        match preview.place_floor(GridPosition { x, floor }) {
            Ok(()) => columns.push(x),
            Err(error) if placement_error_is_insufficient_funds(&error) => {
                limiting_error = Some(error);
                break;
            }
            Err(error @ PlacementError::FloorOutOfRange { .. }) => {
                limiting_error = Some(error);
                break;
            }
            Err(_) => {}
        }
    }
    FillPlan {
        columns,
        limiting_error,
    }
}

fn lobby_fill_plan(tower: &Tower, floor: i16, stories: u8) -> FillPlan {
    let mut preview = tower.clone();
    let mut columns = Vec::new();
    let mut limiting_error = None;
    for x in 0..tower.width() {
        match preview.place_lobby(GridPosition { x, floor }, stories) {
            Ok(_) => columns.push(x),
            Err(error) if placement_error_is_insufficient_funds(&error) => {
                limiting_error = Some(error);
                break;
            }
            Err(_) => {}
        }
    }
    FillPlan {
        columns,
        limiting_error,
    }
}

fn tenant_fill_plan(tower: &Tower, kind: FacilityKind, start: GridPosition) -> TenantFillPlan {
    if !kind.has_tenant_occupancy() {
        return TenantFillPlan {
            positions: Vec::new(),
            limiting_error: None,
        };
    }
    let mut preview = tower.clone();
    let mut positions = Vec::new();
    let mut x = start.x;
    let width = kind.spec().width;
    let limiting_error = loop {
        let position = GridPosition {
            x,
            floor: start.floor,
        };
        match preview.place(kind, position) {
            Ok(_) => positions.push(position),
            Err(error) => break Some(error),
        }
        let Some(next_x) = x.checked_add(width) else {
            break Some(PlacementError::OutsideTower);
        };
        if next_x >= tower.width() {
            break None;
        }
        x = next_x;
    };
    TenantFillPlan {
        positions,
        limiting_error,
    }
}

fn contiguous_column_runs(columns: &[u16]) -> Vec<(u16, u16)> {
    let Some(&first) = columns.first() else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut start = first;
    let mut previous = first;
    for &column in &columns[1..] {
        if column != previous + 1 {
            runs.push((start, previous + 1));
            start = column;
        }
        previous = column;
    }
    runs.push((start, previous + 1));
    runs
}

fn placement_error_is_insufficient_funds(error: &PlacementError) -> bool {
    matches!(error, PlacementError::InsufficientFunds { .. })
}

fn roof_crane_anchor(floors: &[GridPosition]) -> Option<GridPosition> {
    let highest_floor = floors
        .iter()
        .filter(|position| position.floor > 0)
        .map(|position| position.floor)
        .max()?;
    floors
        .iter()
        .filter(|position| position.floor == highest_floor)
        .min_by_key(|position| position.x)
        .copied()
}

fn facility_crowd_level(
    kind: FacilityKind,
    occupied: bool,
    visitors: usize,
    open: bool,
) -> FacilityCrowd {
    if !occupied {
        return FacilityCrowd::Empty;
    }
    if !open {
        return FacilityCrowd::Closed;
    }
    if visitors == 0 {
        return FacilityCrowd::Empty;
    }
    match kind {
        FacilityKind::Office if visitors >= 4 => FacilityCrowd::Heavy,
        FacilityKind::FastFood if visitors >= 35 => FacilityCrowd::Heavy,
        FacilityKind::Restaurant if visitors >= 70 => FacilityCrowd::Heavy,
        _ => FacilityCrowd::Light,
    }
}

const fn has_dedicated_patronage(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::FastFood
            | FacilityKind::Restaurant
            | FacilityKind::Shop
            | FacilityKind::Cinema
            | FacilityKind::PartyHall
    )
}

const fn has_independent_population(kind: FacilityKind) -> bool {
    matches!(kind, FacilityKind::Condo) || has_dedicated_patronage(kind)
}

fn facility_light_tint(kind: FacilityKind, crowd: FacilityCrowd) -> Color {
    if crowd == FacilityCrowd::Closed && matches!(kind, FacilityKind::Shop | FacilityKind::Medical)
    {
        Color::from_rgba(150, 154, 176, 255)
    } else {
        WHITE
    }
}

fn office_visible_worker_count(visitors: usize) -> usize {
    // Offices hold six workers in the original simulation. Render the actual
    // visitors rather than swapping the entire room scene at an occupancy threshold.
    visitors.min(6)
}

const fn person_sprite_sheet(destination: Option<FacilityKind>, person_id: u64) -> usize {
    match destination {
        Some(FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite) => 1,
        Some(FacilityKind::Medical | FacilityKind::Security) => 2,
        Some(FacilityKind::FastFood) => 3,
        Some(FacilityKind::Cathedral | FacilityKind::PartyHall) => 4,
        Some(FacilityKind::Shop | FacilityKind::Restaurant | FacilityKind::Cinema) => 5,
        Some(FacilityKind::Housekeeping | FacilityKind::Recycling) => 6,
        _ => (person_id as usize % 2) * 5,
    }
}

const fn traveler_sprite_sheet(
    role: PersonRole,
    home: Option<FacilityKind>,
    destination: Option<FacilityKind>,
    person_id: u64,
) -> usize {
    match role {
        PersonRole::Salesman | PersonRole::Homebody => 0,
        PersonRole::Woman | PersonRole::WomanWithChild => 5,
        PersonRole::Child => 6,
        PersonRole::Security => 2,
        // Sheet 1513 contains the original housekeeping/room-service
        // figures. Sheet 1518 is the child sheet and was previously selected
        // here by mistake.
        PersonRole::Housekeeper => 1,
        PersonRole::Visitor if matches!(home, Some(FacilityKind::Housekeeping)) => 1,
        PersonRole::Visitor => person_sprite_sheet(destination, person_id),
    }
}

const fn queue_sheet_for_mood(mood: PersonMood) -> usize {
    match mood {
        PersonMood::Calm => 0,
        PersonMood::Concerned => 1,
        PersonMood::Angry => 2,
    }
}

fn requested_lobby_stories() -> u8 {
    let control = is_key_down(KeyCode::LeftControl)
        || is_key_down(KeyCode::RightControl)
        || is_key_down(KeyCode::LeftAlt)
        || is_key_down(KeyCode::RightAlt)
        || is_key_down(KeyCode::LeftSuper)
        || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    lobby_stories_for_modifiers(control, shift)
}

const fn lobby_stories_for_modifiers(control_or_option: bool, shift: bool) -> u8 {
    if control_or_option && shift {
        3
    } else if control_or_option {
        2
    } else {
        1
    }
}

fn horizontal_span_is_visible(x: f32, width: f32, viewport_width: f32) -> bool {
    x + width >= 0.0 && x <= viewport_width
}

fn emergency_stair_screen_xs(start_x: u16, end_x: u16, camera_x: f32) -> [f32; 2] {
    [
        (f32::from(start_x) - camera_x) * CELL_WIDTH - EMERGENCY_STAIR_HALF_WIDTH,
        (f32::from(end_x) - camera_x) * CELL_WIDTH,
    ]
}

fn seasonal_event_for_date(date: &str) -> Option<SeasonalEventKind> {
    match date.trim() {
        "12-25" => Some(SeasonalEventKind::Santa),
        "10-31" => Some(SeasonalEventKind::Witch),
        _ => None,
    }
}

fn lobby_display_name(floor: i16, stories: u8) -> &'static str {
    if floor > 1 {
        "sky-lobby"
    } else if stories > 1 {
        "super-lobby"
    } else {
        "lobby"
    }
}

fn lobby_stories_for_position(tower: &Tower, position: GridPosition) -> u8 {
    lobby_stories_for_position_with_request(tower, position, requested_lobby_stories())
}

fn lobby_stories_for_position_with_request(
    tower: &Tower,
    position: GridPosition,
    requested_stories: u8,
) -> u8 {
    if position.floor != 1 || requested_stories <= 1 {
        return 1;
    }
    let existing_lobbies = tower
        .facilities()
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Lobby)
        .collect::<Vec<_>>();
    if existing_lobbies.is_empty()
        || (existing_lobbies
            .iter()
            .all(|facility| facility.position.floor == 1 && facility.stories == requested_stories)
            && existing_lobbies.iter().any(|facility| {
                facility.position.x + 1 == position.x || position.x + 1 == facility.position.x
            }))
    {
        requested_stories
    } else {
        1
    }
}

fn shift_modifier_down() -> bool {
    is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
}

fn seasonal_event_for_system_date() -> Option<SeasonalEventKind> {
    if let Ok(override_date) = std::env::var("OPENTOWER_DATE") {
        return seasonal_event_for_date(&override_date);
    }
    #[cfg(target_os = "windows")]
    let output = hidden_windows_command("powershell")
        .args(["-NoProfile", "-Command", "(Get-Date).ToString('MM-dd')"])
        .output()
        .ok()?;
    #[cfg(not(target_os = "windows"))]
    let output = Command::new("date").arg("+%m-%d").output().ok()?;
    seasonal_event_for_date(&String::from_utf8_lossy(&output.stdout))
}

const fn weather_is_raining(day: u32) -> bool {
    day.is_multiple_of(5) || day % 7 == 3
}

fn rain_palette_index(minute_of_day: u16) -> usize {
    ((minute_of_day as usize * 10) / (24 * 60)).min(9)
}

fn rain_tile_layout(
    view_width: f32,
    top: f32,
    bottom: f32,
    camera_x: f32,
    tile_width: f32,
    tile_height: f32,
) -> Vec<Vec2> {
    if view_width <= 0.0 || bottom <= top || tile_width <= 0.0 || tile_height <= 0.0 {
        return Vec::new();
    }
    let rows = ((bottom - top) / tile_height).ceil() as i32;
    let columns = (view_width / tile_width).ceil() as i32 + 2;
    let first_x = -(camera_x * CELL_WIDTH).rem_euclid(tile_width);
    let first_y = bottom - rows as f32 * tile_height;
    let mut origins = Vec::with_capacity((rows * columns) as usize);
    for row in 0..rows {
        for column in 0..columns {
            origins.push(vec2(
                first_x + column as f32 * tile_width,
                first_y + row as f32 * tile_height,
            ));
        }
    }
    origins
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorldLighting {
    night_alpha: u8,
    warm_alpha: u8,
}

fn world_lighting(minute_of_day: u16) -> WorldLighting {
    const NIGHT_ALPHA: f32 = 118.0;
    const WARM_ALPHA: f32 = 46.0;
    let minute = minute_of_day.min(24 * 60 - 1);
    let (night, warm) = match minute {
        0..=299 => (NIGHT_ALPHA, 0.0),
        300..=359 => {
            let progress = f32::from(minute - 300) / 60.0;
            (
                NIGHT_ALPHA * (1.0 - progress),
                WARM_ALPHA * (1.0 - (progress * 2.0 - 1.0).abs()),
            )
        }
        360..=1079 => (0.0, 0.0),
        1080..=1139 => {
            let progress = f32::from(minute - 1080) / 60.0;
            (
                NIGHT_ALPHA * progress,
                WARM_ALPHA * (1.0 - (progress * 2.0 - 1.0).abs()),
            )
        }
        _ => (NIGHT_ALPHA, 0.0),
    };
    WorldLighting {
        night_alpha: night.round() as u8,
        warm_alpha: warm.round() as u8,
    }
}

const fn night_sprite_active(minute_of_day: u16) -> bool {
    minute_of_day < 5 * 60 || minute_of_day >= 18 * 60
}

fn office_sprite_frame(minute_of_day: u16) -> usize {
    usize::from(night_sprite_active(minute_of_day))
}

fn hotel_sprite_frame(minute_of_day: u16, visitors: usize, dirty: bool, infested: bool) -> usize {
    // Reverse engineering the original 1193/1257/1321 resource families
    // confirms four day/night pairs: occupied, clean, dirty, infested.
    let state = if infested {
        3
    } else if dirty {
        2
    } else if visitors == 0 {
        1
    } else {
        0
    };
    state * 2 + usize::from(night_sprite_active(minute_of_day))
}

fn hotel_checkout_minute(facility_id: u64) -> u16 {
    const FIRST_CHECKOUT: u16 = 6 * 60 + 30;
    const LAST_CHECKOUT: u16 = 12 * 60;
    let window = u64::from(LAST_CHECKOUT - FIRST_CHECKOUT + 1);
    FIRST_CHECKOUT
        + (facility_id
            .wrapping_mul(97)
            .wrapping_add(facility_id.rotate_left(17))
            % window) as u16
}

const fn speed_label(speed: SimulationSpeed) -> &'static str {
    match speed {
        SimulationSpeed::Paused => "Pause",
        SimulationSpeed::Normal => "1x",
        SimulationSpeed::Fast => "2x",
        SimulationSpeed::Triple => "3x",
        SimulationSpeed::Quintuple => "5x",
        SimulationSpeed::Tenfold => "10x",
    }
}

const fn speed_keyboard_shortcut(key: KeyCode) -> Option<SimulationSpeed> {
    match key {
        KeyCode::GraveAccent => Some(SimulationSpeed::Paused),
        KeyCode::Key1 | KeyCode::Kp1 => Some(SimulationSpeed::Normal),
        KeyCode::Key2 | KeyCode::Kp2 => Some(SimulationSpeed::Fast),
        KeyCode::Key3 | KeyCode::Kp3 => Some(SimulationSpeed::Triple),
        KeyCode::Key4 | KeyCode::Kp4 => Some(SimulationSpeed::Quintuple),
        KeyCode::Key5 | KeyCode::Kp5 => Some(SimulationSpeed::Tenfold),
        _ => None,
    }
}

fn build_price_label(kind: FacilityKind) -> String {
    if is_elevator_kind(kind) {
        format!(
            "${} shaft / ${} car",
            kind.spec().construction_cost,
            elevator_car_cost(kind)
        )
    } else {
        format!("${}", kind.spec().construction_cost)
    }
}

const fn price_level_label(level: u8) -> &'static str {
    match level {
        0 => "Low",
        2 => "High",
        3 => "Maximum",
        _ => "Standard",
    }
}

fn stepped_price_level(current: u8, step: i8) -> u8 {
    ((current.min(3) as i8 + step).rem_euclid(4)) as u8
}

fn set_price_for_tenant_kind(tower: &mut Tower, kind: FacilityKind, price: u8) -> usize {
    let ids = tower
        .facilities()
        .iter()
        .filter(|facility| facility.kind == kind && facility.kind.has_tenant_occupancy())
        .map(|facility| facility.id)
        .collect::<Vec<_>>();
    for facility_id in &ids {
        tower.set_price_level(*facility_id, price);
    }
    ids.len()
}

fn control_modifier_down() -> bool {
    is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
}

const fn is_hotel_room(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite
    )
}

fn speed_menu_rect() -> Rect {
    let button = palette_button_rect(0);
    Rect::new(
        button.x + button.w + 4.0,
        button.y,
        100.0,
        4.0 + SPEED_CHOICES.len() as f32 * 25.0,
    )
}

fn speed_menu_entry_rect(index: usize) -> Rect {
    let menu = speed_menu_rect();
    Rect::new(
        menu.x + 2.0,
        menu.y + 2.0 + index as f32 * 25.0,
        menu.w - 4.0,
        25.0,
    )
}

fn speed_menu_choice_at(point: Vec2) -> Option<SimulationSpeed> {
    SPEED_CHOICES
        .iter()
        .copied()
        .enumerate()
        .find(|(index, _)| speed_menu_entry_rect(*index).contains(point))
        .map(|(_, speed)| speed)
}

fn fresh_tower() -> Tower {
    let mut tower = Tower::default();
    tower.seed_treasure_site(macroquad::rand::gen_range(0, i32::MAX) as u64);
    tower
}

fn weekday_name(day: u32) -> &'static str {
    const WEEKDAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    WEEKDAYS[day.saturating_sub(1) as usize % WEEKDAYS.len()]
}

fn quarter_closed_between(previous: u64, current: u64) -> Option<(u32, u8)> {
    if current <= previous {
        return None;
    }
    let first_day = previous / (24 * 60);
    let last_day = current / (24 * 60);
    for day in first_day..=last_day {
        if day == 0 || !day.is_multiple_of(3) {
            continue;
        }
        let close_minute = day * 24 * 60 + 5 * 60;
        if previous < close_minute && close_minute <= current {
            let period = day / 3 - 1;
            return Some(((period / 4) as u32 + 1, (period % 4) as u8 + 1));
        }
    }
    None
}

fn topbar_clock_x(viewport_width: f32, clock_width: f32, controls_right: f32) -> f32 {
    (viewport_width * 0.5 - clock_width * 0.5).max(controls_right + 12.0)
}

fn game_menu_button_rect() -> Rect {
    Rect::new(6.0, 6.0, 54.0, 23.0)
}

fn mode_menu_button_rect() -> Rect {
    Rect::new(
        game_menu_button_rect().x + game_menu_button_rect().w + 4.0,
        6.0,
        105.0,
        23.0,
    )
}

fn sound_menu_button_rect() -> Rect {
    let mode = mode_menu_button_rect();
    Rect::new(mode.x + mode.w + 4.0, 6.0, 32.0, 23.0)
}

fn rating_row_rect() -> Rect {
    let sound = sound_menu_button_rect();
    Rect::new(sound.x + sound.w + 4.0, 6.0, RATING_ROW_WIDTH, 23.0)
}

fn rating_star_rect(index: usize) -> Rect {
    let row = rating_row_rect();
    let total_width = RATING_STAR_WIDTH * 5.0 + RATING_STAR_GAP * 4.0;
    let start_x = row.x + (row.w - total_width) * 0.5;
    Rect::new(
        start_x + index as f32 * (RATING_STAR_WIDTH + RATING_STAR_GAP),
        row.y + (row.h - RATING_STAR_HEIGHT) * 0.5,
        RATING_STAR_WIDTH,
        RATING_STAR_HEIGHT,
    )
}

fn mode_menu_rect() -> Rect {
    let button = mode_menu_button_rect();
    Rect::new(button.x, 31.0, button.w, 102.0)
}

fn mode_menu_entry_rect(index: usize) -> Rect {
    let menu = mode_menu_rect();
    Rect::new(
        menu.x + 3.0,
        menu.y + 3.0 + index as f32 * 24.0,
        menu.w - 6.0,
        24.0,
    )
}

fn mode_menu_choice_at(point: Vec2) -> Option<ViewMode> {
    MODE_MENU_LABELS
        .iter()
        .enumerate()
        .find(|(index, _)| mode_menu_entry_rect(*index).contains(point))
        .and_then(|(index, _)| ViewMode::from_menu_index(index))
}

fn game_menu_rect() -> Rect {
    Rect::new(
        6.0,
        31.0,
        176.0,
        7.0 + GAME_MENU_ACTIONS.len() as f32 * 24.0,
    )
}

fn game_menu_entry_rect(index: usize) -> Rect {
    let menu = game_menu_rect();
    Rect::new(
        menu.x + 3.0,
        menu.y + 3.0 + index as f32 * 24.0,
        menu.w - 6.0,
        24.0,
    )
}

fn game_menu_action_at(point: Vec2) -> Option<GameMenuAction> {
    GAME_MENU_ACTIONS
        .iter()
        .enumerate()
        .find(|(index, _)| game_menu_entry_rect(*index).contains(point))
        .map(|(_, (action, _))| *action)
}

fn title_screen_transform(viewport: Vec2) -> (f32, Vec2) {
    let scale = (viewport.x / TITLE_SCREEN_WIDTH)
        .min(viewport.y / TITLE_SCREEN_HEIGHT)
        .max(0.01);
    let size = vec2(TITLE_SCREEN_WIDTH, TITLE_SCREEN_HEIGHT) * scale;
    (scale, (viewport - size) * 0.5)
}

fn title_screen_rect(viewport: Vec2, source: Rect) -> Rect {
    let (scale, origin) = title_screen_transform(viewport);
    Rect::new(
        origin.x + source.x * scale,
        origin.y + source.y * scale,
        source.w * scale,
        source.h * scale,
    )
}

fn welcome_dialog_rect(viewport: Vec2) -> Rect {
    // Measured from the original 640x480 Windows title screen. Keeping this
    // tied to the storefront canvas preserves its placement over the doors.
    title_screen_rect(viewport, Rect::new(190.0, 182.0, 264.0, 119.0))
}

fn welcome_button_rect(viewport: Vec2, index: usize) -> Rect {
    let source = match index {
        0 => Rect::new(207.0, 192.0, 230.0, 27.0),
        1 => Rect::new(207.0, 228.0, 230.0, 27.0),
        _ => Rect::new(281.0, 264.0, 82.0, 27.0),
    };
    title_screen_rect(viewport, source)
}

fn welcome_action_index_at(viewport: Vec2, point: Vec2) -> Option<usize> {
    WELCOME_ACTIONS
        .iter()
        .enumerate()
        .find(|(index, _)| welcome_button_rect(viewport, *index).contains(point))
        .map(|(index, _)| index)
}

fn welcome_action_at(viewport: Vec2, point: Vec2) -> Option<WelcomeAction> {
    welcome_action_index_at(viewport, point).map(|index| WELCOME_ACTIONS[index].0)
}

fn ux_scale_menu_rect() -> Rect {
    let parent = game_menu_entry_rect(4);
    Rect::new(
        parent.x + parent.w - 1.0,
        parent.y,
        106.0,
        7.0 + UX_SCALE_CHOICES.len() as f32 * 24.0,
    )
}

fn ux_scale_menu_entry_rect(index: usize) -> Rect {
    let menu = ux_scale_menu_rect();
    Rect::new(
        menu.x + 3.0,
        menu.y + 3.0 + index as f32 * 24.0,
        menu.w - 6.0,
        24.0,
    )
}

fn ux_scale_menu_choice_at(point: Vec2) -> Option<UxScale> {
    UX_SCALE_CHOICES
        .iter()
        .copied()
        .enumerate()
        .find(|(index, _)| ux_scale_menu_entry_rect(*index).contains(point))
        .map(|(_, scale)| scale)
}

fn active_ux_scale() -> UxScale {
    UxScale::from_quarters(ACTIVE_UX_SCALE.load(Ordering::Relaxed))
}

fn set_active_ux_scale(scale: UxScale) {
    ACTIVE_UX_SCALE.store(scale as u8, Ordering::Relaxed);
}

fn physical_to_ui(point: Vec2, scale: UxScale) -> Vec2 {
    point / scale.factor()
}

fn ui_mouse_position() -> Vec2 {
    let (x, y) = mouse_position();
    physical_to_ui(vec2(x, y), active_ux_scale())
}

fn ui_screen_width() -> f32 {
    screen_width() / active_ux_scale().factor()
}

fn ui_screen_height() -> f32 {
    screen_height() / active_ux_scale().factor()
}

fn ui_text_rasterization(font_size: f32, scale: f32) -> (u16, f32) {
    let logical_size = font_size.max(1.0);
    let raster_size = (logical_size * scale.max(1.0))
        .round()
        .clamp(1.0, u16::MAX as f32) as u16;
    (raster_size, logical_size / f32::from(raster_size))
}

/// Macroquad's ordinary text helper rasterizes glyphs at their logical size.
/// Enlarging those cached glyph quads with the UX camera loses most of the
/// thin strokes on high-DPI displays. Rasterize at the final UX density and
/// counter-scale the quad so its logical measurements remain unchanged.
fn draw_text(
    text: impl AsRef<str>,
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
) -> TextDimensions {
    if !UI_CAMERA_ACTIVE.load(Ordering::Relaxed) {
        return macroquad::text::draw_text(text, x, y, font_size, color);
    }
    let (raster_size, font_scale) = ui_text_rasterization(font_size, active_ux_scale().factor());
    macroquad::text::draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: raster_size,
            font_scale,
            color,
            ..Default::default()
        },
    )
}

fn measure_text(
    text: &str,
    font: Option<&Font>,
    font_size: u16,
    font_scale: f32,
) -> TextDimensions {
    if !UI_CAMERA_ACTIVE.load(Ordering::Relaxed) {
        return macroquad::text::measure_text(text, font, font_size, font_scale);
    }
    let (raster_size, density_scale) =
        ui_text_rasterization(f32::from(font_size), active_ux_scale().factor());
    macroquad::text::measure_text(text, font, raster_size, font_scale * density_scale)
}

fn begin_ui_camera(scale: UxScale) {
    set_active_ux_scale(scale);
    UI_CAMERA_ACTIVE.store(true, Ordering::Relaxed);
    let width = ui_screen_width();
    let height = ui_screen_height();
    // Camera2D's onscreen matrix applies its own Y inversion. A positive Y
    // zoom therefore preserves the same top-left origin as Macroquad's
    // default drawing coordinates while scaling only this overlay pass.
    set_camera(&Camera2D {
        target: vec2(width * 0.5, height * 0.5),
        zoom: vec2(2.0 / width, 2.0 / height),
        ..Default::default()
    });
}

fn save_name_dialog_rect() -> Rect {
    Rect::new(
        ui_screen_width() * 0.5 - 190.0,
        ui_screen_height() * 0.5 - 80.0,
        380.0,
        160.0,
    )
}

fn save_dialog_save_rect() -> Rect {
    let dialog = save_name_dialog_rect();
    Rect::new(
        dialog.x + dialog.w - 176.0,
        dialog.y + dialog.h - 34.0,
        76.0,
        23.0,
    )
}

fn save_dialog_cancel_rect() -> Rect {
    let dialog = save_name_dialog_rect();
    Rect::new(
        dialog.x + dialog.w - 92.0,
        dialog.y + dialog.h - 34.0,
        76.0,
        23.0,
    )
}

fn save_game_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let home = env::var_os("USERPROFILE").or_else(|| {
        let drive = env::var_os("HOMEDRIVE")?;
        let path = env::var_os("HOMEPATH")?;
        let mut combined = PathBuf::from(drive);
        combined.push(path);
        Some(combined.into_os_string())
    });
    #[cfg(not(target_os = "windows"))]
    let home = env::var_os("HOME");
    home.map(PathBuf::from)
        .map(|path| path.join("SimTower"))
        .ok_or_else(|| "Could not determine the current user's home folder".to_owned())
}

fn ux_scale_preference_path() -> Result<PathBuf, String> {
    Ok(save_game_directory()?.join(UX_SCALE_SETTINGS_FILE))
}

fn load_ux_scale_preference() -> UxScale {
    ux_scale_preference_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|value| UxScale::from_preference(&value))
        .unwrap_or(UxScale::One)
}

fn save_ux_scale_preference(scale: UxScale) -> Result<(), String> {
    let path = ux_scale_preference_path()?;
    let directory = path
        .parent()
        .ok_or_else(|| "UX scale preference path does not have a parent folder".to_owned())?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create settings folder: {error}"))?;
    fs::write(&path, scale.preference_value())
        .map_err(|error| format!("could not save UX scale preference: {error}"))
}

fn sanitized_save_filename(name: &str) -> Result<String, String> {
    let mut sanitized = String::new();
    for character in name.trim().chars().take(64) {
        if character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        {
            sanitized.push('_');
        } else {
            sanitized.push(character);
        }
    }
    let sanitized = sanitized.trim_matches([' ', '.']).trim();
    if sanitized.is_empty() {
        return Err("Please enter a save-game name".to_owned());
    }
    if sanitized.to_ascii_lowercase().ends_with(".smtower") {
        Ok(sanitized.to_owned())
    } else {
        Ok(format!("{sanitized}.{SAVE_EXTENSION}"))
    }
}

fn save_game_path(name: &str) -> Result<PathBuf, String> {
    Ok(save_game_directory()?.join(sanitized_save_filename(name)?))
}

fn save_default_name_for_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_SAVE_NAME)
        .to_owned()
}

fn discard_queued_text_input() {
    while get_char_pressed().is_some() {}
}

fn key_for_text_character(character: char) -> Option<KeyCode> {
    const LETTER_KEYS: [KeyCode; 26] = [
        KeyCode::A,
        KeyCode::B,
        KeyCode::C,
        KeyCode::D,
        KeyCode::E,
        KeyCode::F,
        KeyCode::G,
        KeyCode::H,
        KeyCode::I,
        KeyCode::J,
        KeyCode::K,
        KeyCode::L,
        KeyCode::M,
        KeyCode::N,
        KeyCode::O,
        KeyCode::P,
        KeyCode::Q,
        KeyCode::R,
        KeyCode::S,
        KeyCode::T,
        KeyCode::U,
        KeyCode::V,
        KeyCode::W,
        KeyCode::X,
        KeyCode::Y,
        KeyCode::Z,
    ];
    const NUMBER_KEYS: [KeyCode; 10] = [
        KeyCode::Key0,
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
    ];
    if character.is_ascii_alphabetic() {
        let index = character.to_ascii_lowercase() as usize - 'a' as usize;
        return Some(LETTER_KEYS[index]);
    }
    if character.is_ascii_digit() {
        return Some(NUMBER_KEYS[character as usize - '0' as usize]);
    }
    match character {
        ' ' => Some(KeyCode::Space),
        '-' | '_' => Some(KeyCode::Minus),
        '=' | '+' => Some(KeyCode::Equal),
        '[' | '{' => Some(KeyCode::LeftBracket),
        ']' | '}' => Some(KeyCode::RightBracket),
        '\\' | '|' => Some(KeyCode::Backslash),
        ';' | ':' => Some(KeyCode::Semicolon),
        '\'' | '"' => Some(KeyCode::Apostrophe),
        ',' | '<' => Some(KeyCode::Comma),
        '.' | '>' => Some(KeyCode::Period),
        '/' | '?' => Some(KeyCode::Slash),
        '`' | '~' => Some(KeyCode::GraveAccent),
        _ => None,
    }
}

fn prepared_save_game_directory() -> Result<PathBuf, String> {
    let directory = save_game_directory()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
    Ok(directory)
}

#[cfg(target_os = "macos")]
fn choose_save_game_file() -> Result<Option<PathBuf>, String> {
    let directory = prepared_save_game_directory()?;
    let escaped = directory
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = format!(
        "POSIX path of (choose file with prompt \"Load SimTower save game\" default location POSIX file \"{escaped}\")"
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|error| format!("could not launch the macOS file picker: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

#[cfg(target_os = "windows")]
fn choose_save_game_file() -> Result<Option<PathBuf>, String> {
    let directory = prepared_save_game_directory()?
        .to_string_lossy()
        .replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title='Load SimTower save game'; $d.InitialDirectory='{directory}'; $d.Filter='SimTower saves (*.smtower)|*.smtower|All files (*.*)|*.*'; if($d.ShowDialog() -eq 'OK'){{$d.FileName}}"
    );
    let output = hidden_windows_command("powershell.exe")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .output()
        .map_err(|error| format!("could not launch the Windows file picker: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

#[cfg(target_os = "linux")]
fn choose_save_game_file() -> Result<Option<PathBuf>, String> {
    let directory = prepared_save_game_directory()?;
    let initial = format!("{}/", directory.display());
    let output = Command::new("zenity")
        .args([
            "--file-selection",
            "--title=Load SimTower save game",
            "--file-filter=SimTower saves | *.smtower",
            "--file-filter=All files | *",
            "--filename",
            &initial,
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(_) => Command::new("kdialog")
            .args([
                "--getopenfilename",
                &initial,
                "*.smtower|SimTower saves",
                "--title",
                "Load SimTower save game",
            ])
            .output()
            .map_err(|error| format!("neither zenity nor kdialog could be started: {error}"))?,
    };
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn choose_save_game_file() -> Result<Option<PathBuf>, String> {
    Err("native file selection is not implemented for this platform".to_owned())
}

fn palette_rect() -> Rect {
    let rows = PALETTE_BUTTONS.len().div_ceil(PALETTE_COLUMNS);
    Rect::new(
        PALETTE_X,
        PALETTE_Y,
        8.0 + PALETTE_COLUMNS as f32 * (PALETTE_BUTTON_SIZE + PALETTE_GAP),
        PALETTE_TITLE_HEIGHT + 7.0 + rows as f32 * (PALETTE_BUTTON_SIZE + PALETTE_GAP),
    )
}

fn palette_button_rect(index: usize) -> Rect {
    let column = index % PALETTE_COLUMNS;
    let row = index / PALETTE_COLUMNS;
    Rect::new(
        PALETTE_X + 4.0 + column as f32 * (PALETTE_BUTTON_SIZE + PALETTE_GAP),
        PALETTE_Y + PALETTE_TITLE_HEIGHT + 3.0 + row as f32 * (PALETTE_BUTTON_SIZE + PALETTE_GAP),
        PALETTE_BUTTON_SIZE,
        PALETTE_BUTTON_SIZE,
    )
}

fn palette_button_at(point: Vec2) -> Option<usize> {
    PALETTE_BUTTONS
        .iter()
        .enumerate()
        .find(|(index, _)| palette_button_rect(*index).contains(point))
        .map(|(index, _)| index)
}

fn submenu_rect(entry_count: usize) -> Rect {
    let palette = palette_rect();
    Rect::new(
        palette.x + palette.w + 5.0,
        palette.y + PALETTE_TITLE_HEIGHT + 2.0,
        SUBMENU_WIDTH,
        SUBMENU_HEADER_HEIGHT + 5.0 + entry_count as f32 * SUBMENU_ROW_HEIGHT,
    )
}

fn submenu_entry_rect(index: usize) -> Rect {
    let menu = submenu_rect(0);
    Rect::new(
        menu.x + 3.0,
        menu.y + SUBMENU_HEADER_HEIGHT + 2.0 + index as f32 * SUBMENU_ROW_HEIGHT,
        menu.w - 6.0,
        SUBMENU_ROW_HEIGHT,
    )
}

fn format_number(value: i64) -> String {
    let negative = value < 0;
    let digits = value.saturating_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn format_currency(value: i64) -> String {
    let magnitude = value.saturating_abs();
    if magnitude >= 1_000_000 {
        let millions = magnitude as f64 / 1_000_000.0;
        return if value < 0 {
            format!("-${millions:.1}M")
        } else {
            format!("${millions:.1}M")
        };
    }
    if value < 0 {
        format!("-${}", format_number(magnitude))
    } else {
        format!("${}", format_number(value))
    }
}

fn finance_period_label(year: u32, quarter: u8) -> String {
    // The built-in pixel-like font lacks the bullet glyph. An ASCII-only gap
    // keeps the two halves balanced without rendering a fallback rectangle.
    format!("Year {year}     Quarter {quarter}")
}

fn draw_centered_text(text: &str, center_x: f32, baseline_y: f32, font_size: u16, color: Color) {
    let width = measure_text(text, None, font_size, 1.0).width;
    draw_text(
        text,
        (center_x - width * 0.5).round(),
        baseline_y.round(),
        f32::from(font_size),
        color,
    );
}

fn draw_right_aligned_text(
    text: &str,
    right_x: f32,
    baseline_y: f32,
    font_size: u16,
    color: Color,
) {
    let width = measure_text(text, None, font_size, 1.0).width;
    draw_text(
        text,
        (right_x - width).round(),
        baseline_y.round(),
        f32::from(font_size),
        color,
    );
}

fn draw_fitted_text(
    text: &str,
    x: f32,
    baseline_y: f32,
    maximum_width: f32,
    preferred_size: u16,
    color: Color,
) {
    let font_size = (8..=preferred_size)
        .rev()
        .find(|size| measure_text(text, None, *size, 1.0).width <= maximum_width)
        .unwrap_or(8);
    draw_text(
        text,
        x.round(),
        baseline_y.round(),
        f32::from(font_size),
        color,
    );
}

fn draw_classic_panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, CLASSIC_FACE);
    draw_line(rect.x, rect.y, rect.x + rect.w, rect.y, 2.0, CLASSIC_LIGHT);
    draw_line(rect.x, rect.y, rect.x, rect.y + rect.h, 2.0, CLASSIC_LIGHT);
    draw_line(
        rect.x,
        rect.y + rect.h,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        CLASSIC_DARK,
    );
    draw_line(
        rect.x + rect.w,
        rect.y,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        CLASSIC_DARK,
    );
}

fn draw_recessed_field(rect: Rect) {
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::from_rgba(238, 238, 238, 255),
    );
    draw_line(rect.x, rect.y, rect.x + rect.w, rect.y, 2.0, CLASSIC_DARK);
    draw_line(rect.x, rect.y, rect.x, rect.y + rect.h, 2.0, CLASSIC_DARK);
    draw_line(
        rect.x,
        rect.y + rect.h,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        CLASSIC_LIGHT,
    );
    draw_line(
        rect.x + rect.w,
        rect.y,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        CLASSIC_LIGHT,
    );
}

fn draw_tenant_people_meter(texture: &Texture2D, rect: Rect, people_icons: u32) {
    let Some((source, destination)) =
        tenant_people_strip_layout(rect, people_icons, texture.width())
    else {
        return;
    };
    draw_texture_ex(
        texture,
        destination.x,
        destination.y,
        WHITE,
        DrawTextureParams {
            source: Some(source),
            dest_size: Some(vec2(destination.w, destination.h)),
            ..Default::default()
        },
    );
}

fn draw_condo_family_meter(people: &[Texture2D; 7], rect: Rect) {
    // The original condo panel depicts the household as individual black
    // person cels, including the shorter child figure. Reuse the walking
    // sheets themselves and tint them black so the panel and tower always
    // agree about which roles make up the family.
    let sheets = [0_usize, 5, 6];
    let size = vec2(10.0, 30.0);
    for (index, sheet) in sheets.into_iter().enumerate() {
        draw_texture_ex(
            &people[sheet],
            rect.x + 8.0 + index as f32 * 13.0,
            (rect.y + (rect.h - size.y) * 0.5).round(),
            BLACK,
            DrawTextureParams {
                source: Some(Rect::new(0.0, 0.0, 8.0, PERSON_SPRITE_HEIGHT)),
                dest_size: Some(size),
                ..Default::default()
            },
        );
    }
}

fn draw_person_status_meter(rect: Rect, fraction: f32, color: Color, segmented: bool) {
    draw_recessed_field(rect);
    let inner = Rect::new(rect.x + 3.0, rect.y + 3.0, rect.w - 6.0, rect.h - 6.0);
    let filled_width = (inner.w * fraction.clamp(0.0, 1.0)).round();
    if filled_width > 0.0 {
        draw_rectangle(inner.x, inner.y, filled_width, inner.h, color);
    }
    if segmented {
        for segment in 1..3 {
            let x = (inner.x + inner.w * segment as f32 / 3.0).round();
            draw_line(x, inner.y, x, inner.y + inner.h, 1.0, CLASSIC_DARK);
        }
    }
}

fn tenant_people_strip_layout(
    rect: Rect,
    people_icons: u32,
    texture_width: f32,
) -> Option<(Rect, Rect)> {
    let count = people_icons.min(30);
    if count == 0 || texture_width <= 0.0 {
        return None;
    }
    let source = Rect::new(0.0, 0.0, (count as f32 * 16.0).min(texture_width), 36.0);
    let scale = (30.0 / source.h).min((rect.w - 16.0).max(1.0) / source.w);
    let size = vec2(source.w * scale, source.h * scale);
    let destination = Rect::new(
        rect.x + 8.0,
        (rect.y + (rect.h - size.y) * 0.5).round(),
        size.x,
        size.y,
    );
    Some((source, destination))
}

fn draw_menu_checkmark(row: Rect, color: Color) {
    let x = row.x + 7.0;
    let y = row.y + 12.0;
    draw_line(x, y, x + 4.0, y + 4.0, 2.0, color);
    draw_line(x + 4.0, y + 4.0, x + 12.0, y - 5.0, 2.0, color);
}

fn star_points(rect: Rect) -> [Vec2; 10] {
    let center = rect.center();
    let outer = rect.h * 0.5;
    let inner = outer * 0.43;
    let x_scale = rect.w / rect.h;
    std::array::from_fn(|index| {
        let radius = if index % 2 == 0 { outer } else { inner };
        let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
        vec2(
            center.x + angle.cos() * radius * x_scale,
            center.y + angle.sin() * radius,
        )
    })
}

fn draw_scalable_star(rect: Rect, earned: bool) {
    let colors = if earned {
        [
            Color::from_rgba(255, 255, 70, 255),
            Color::from_rgba(242, 218, 0, 255),
        ]
    } else {
        [
            Color::from_rgba(164, 164, 164, 255),
            Color::from_rgba(126, 126, 126, 255),
        ]
    };
    let outline = if earned {
        Color::from_rgba(145, 132, 0, 255)
    } else {
        Color::from_rgba(94, 94, 94, 255)
    };
    draw_star_with_palette(rect, colors, outline);
}

fn draw_star_with_palette(rect: Rect, colors: [Color; 2], outline: Color) {
    let points = star_points(rect);
    let center = rect.center();
    for index in 0..points.len() {
        draw_triangle(
            center,
            points[index],
            points[(index + 1) % points.len()],
            colors[index % 2],
        );
    }
    for index in 0..points.len() {
        draw_line(
            points[index].x,
            points[index].y,
            points[(index + 1) % points.len()].x,
            points[(index + 1) % points.len()].y,
            0.8,
            outline,
        );
    }
}

fn draw_promotion_award(rect: Rect) {
    let center = rect.center();
    let orbit = rect.w.min(rect.h) * 0.40;
    for index in 0..12 {
        let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::TAU / 12.0;
        let size = if index % 3 == 0 { 15.0 } else { 11.0 };
        let small = Rect::new(
            center.x + angle.cos() * orbit - size * 0.5,
            center.y + angle.sin() * orbit - size * 0.5,
            size,
            size,
        );
        if index % 3 == 0 {
            draw_star_with_palette(
                small,
                [WHITE, Color::from_rgba(225, 225, 225, 255)],
                Color::from_rgba(170, 170, 170, 255),
            );
        } else {
            draw_star_with_palette(
                small,
                [
                    Color::from_rgba(185, 215, 215, 255),
                    Color::from_rgba(135, 180, 180, 255),
                ],
                Color::from_rgba(110, 150, 150, 255),
            );
        }
    }
    let large_size = rect.w.min(rect.h) * 0.58;
    draw_scalable_star(
        Rect::new(
            center.x - large_size * 0.5,
            center.y - large_size * 0.5,
            large_size,
            large_size,
        ),
        true,
    );
}

fn draw_play_pause_glyph(rect: Rect, show_play: bool, color: Color) {
    if show_play {
        draw_triangle(
            vec2(rect.x + rect.w * 0.24, rect.y + rect.h * 0.12),
            vec2(rect.x + rect.w * 0.24, rect.y + rect.h * 0.88),
            vec2(rect.x + rect.w * 0.82, rect.y + rect.h * 0.5),
            color,
        );
    } else {
        let bar_width = (rect.w * 0.23).max(2.0);
        draw_rectangle(
            rect.x + rect.w * 0.22,
            rect.y + rect.h * 0.14,
            bar_width,
            rect.h * 0.72,
            color,
        );
        draw_rectangle(
            rect.x + rect.w * 0.58,
            rect.y + rect.h * 0.14,
            bar_width,
            rect.h * 0.72,
            color,
        );
    }
}

fn draw_speed_choice_glyph(rect: Rect, speed: SimulationSpeed, color: Color) {
    if speed == SimulationSpeed::Paused {
        draw_play_pause_glyph(rect, false, color);
        return;
    }
    let triangle_width = rect.w * 0.58;
    draw_triangle(
        vec2(rect.x, rect.y + rect.h * 0.12),
        vec2(rect.x, rect.y + rect.h * 0.88),
        vec2(rect.x + triangle_width, rect.y + rect.h * 0.5),
        color,
    );
    let marks = match speed {
        SimulationSpeed::Normal => 0,
        SimulationSpeed::Fast => 1,
        SimulationSpeed::Triple => 2,
        SimulationSpeed::Quintuple => 3,
        SimulationSpeed::Tenfold => 4,
        SimulationSpeed::Paused => 0,
    };
    for index in 0..marks {
        let x = rect.x + triangle_width + 1.5 + index as f32 * 1.8;
        draw_line(
            x,
            rect.y + rect.h * 0.28,
            x,
            rect.y + rect.h * 0.72,
            1.0,
            color,
        );
    }
}

#[derive(Clone, Copy)]
struct SpeakerIconGeometry {
    body: Rect,
    cone: [Vec2; 3],
}

fn speaker_icon_geometry(rect: Rect) -> SpeakerIconGeometry {
    let x = (rect.x + 4.0).round();
    let center_y = (rect.y + rect.h * 0.5).round();
    SpeakerIconGeometry {
        body: Rect::new(x, center_y - 3.0, 7.0, 6.0),
        // Extend the body beneath the cone and attach its point at the vertical
        // center. The solid overlap prevents a visible seam at native pixel scale.
        cone: [
            vec2(x + 3.0, center_y),
            vec2(x + 11.0, center_y - 7.0),
            vec2(x + 11.0, center_y + 7.0),
        ],
    }
}

fn draw_sound_icon(rect: Rect, muted: bool) {
    let geometry = speaker_icon_geometry(rect);
    let x = geometry.body.x;
    let center_y = geometry.body.y + geometry.body.h * 0.5;
    let ink = Color::from_rgba(24, 24, 24, 255);
    draw_rectangle(
        geometry.body.x,
        geometry.body.y,
        geometry.body.w,
        geometry.body.h,
        ink,
    );
    draw_triangle(geometry.cone[0], geometry.cone[1], geometry.cone[2], ink);
    if muted {
        draw_line(
            x + 1.0,
            center_y - 8.0,
            x + 23.0,
            center_y + 8.0,
            2.0,
            Color::from_rgba(210, 25, 45, 255),
        );
    } else {
        // Two open arcs read as sound waves at native pixel scale. A single
        // chevron resembles a play arrow, which made the prior icon unclear.
        draw_line(x + 14.0, center_y - 4.0, x + 16.0, center_y - 2.0, 1.0, ink);
        draw_line(x + 16.0, center_y - 2.0, x + 16.0, center_y + 2.0, 1.0, ink);
        draw_line(x + 16.0, center_y + 2.0, x + 14.0, center_y + 4.0, 1.0, ink);
        draw_line(x + 18.0, center_y - 6.0, x + 21.0, center_y - 3.0, 1.0, ink);
        draw_line(x + 21.0, center_y - 3.0, x + 21.0, center_y + 3.0, 1.0, ink);
        draw_line(x + 21.0, center_y + 3.0, x + 18.0, center_y + 6.0, 1.0, ink);
    }
}

fn draw_classic_button(rect: Rect, pressed: bool) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, CLASSIC_FACE);
    let (top_left, bottom_right) = if pressed {
        (CLASSIC_DARK, CLASSIC_LIGHT)
    } else {
        (CLASSIC_LIGHT, CLASSIC_DARK)
    };
    draw_line(rect.x, rect.y, rect.x + rect.w, rect.y, 2.0, top_left);
    draw_line(rect.x, rect.y, rect.x, rect.y + rect.h, 2.0, top_left);
    draw_line(
        rect.x,
        rect.y + rect.h,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        bottom_right,
    );
    draw_line(
        rect.x + rect.w,
        rect.y,
        rect.x + rect.w,
        rect.y + rect.h,
        2.0,
        bottom_right,
    );
}

fn draw_title_screen_panel(rect: Rect, scale: f32) {
    let edge = scale.max(1.0);
    let shadow = 4.0 * scale;
    draw_rectangle(
        rect.x + shadow,
        rect.y + shadow,
        rect.w,
        rect.h,
        Color::from_rgba(70, 70, 70, 190),
    );
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, CLASSIC_FACE);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, edge, CLASSIC_DARK);
    draw_line(
        rect.x + edge,
        rect.y + edge,
        rect.x + rect.w - edge,
        rect.y + edge,
        edge,
        CLASSIC_LIGHT,
    );
    draw_line(
        rect.x + edge,
        rect.y + edge,
        rect.x + edge,
        rect.y + rect.h - edge,
        edge,
        CLASSIC_LIGHT,
    );
    draw_line(
        rect.x + edge,
        rect.y + rect.h - edge,
        rect.x + rect.w - edge,
        rect.y + rect.h - edge,
        edge,
        CLASSIC_SHADOW,
    );
    draw_line(
        rect.x + rect.w - edge,
        rect.y + edge,
        rect.x + rect.w - edge,
        rect.y + rect.h - edge,
        edge,
        CLASSIC_SHADOW,
    );
}

fn draw_title_screen_button(rect: Rect, scale: f32, pressed: bool, focused: bool) {
    let edge = scale.max(1.0);
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, CLASSIC_FACE);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, edge, CLASSIC_DARK);
    let (top_left, bottom_right) = if pressed {
        (CLASSIC_SHADOW, CLASSIC_LIGHT)
    } else {
        (CLASSIC_LIGHT, CLASSIC_SHADOW)
    };
    draw_line(
        rect.x + edge,
        rect.y + edge,
        rect.x + rect.w - edge,
        rect.y + edge,
        edge,
        top_left,
    );
    draw_line(
        rect.x + edge,
        rect.y + edge,
        rect.x + edge,
        rect.y + rect.h - edge,
        edge,
        top_left,
    );
    draw_line(
        rect.x + edge,
        rect.y + rect.h - edge,
        rect.x + rect.w - edge,
        rect.y + rect.h - edge,
        edge,
        bottom_right,
    );
    draw_line(
        rect.x + rect.w - edge,
        rect.y + edge,
        rect.x + rect.w - edge,
        rect.y + rect.h - edge,
        edge,
        bottom_right,
    );
    if focused {
        draw_title_screen_focus_rect(rect, scale);
    }
}

fn draw_title_screen_focus_rect(rect: Rect, scale: f32) {
    let dot = scale.max(1.0);
    let step = dot * 2.0;
    let inset = 4.0 * scale;
    let left = rect.x + inset;
    let right = rect.x + rect.w - inset;
    let top = rect.y + inset;
    let bottom = rect.y + rect.h - inset;
    let mut x = left;
    while x <= right - dot {
        draw_rectangle(x, top, dot, dot, CLASSIC_DARK);
        draw_rectangle(x, bottom - dot, dot, dot, CLASSIC_DARK);
        x += step;
    }
    let mut y = top + step;
    while y <= bottom - dot - step {
        draw_rectangle(left, y, dot, dot, CLASSIC_DARK);
        draw_rectangle(right - dot, y, dot, dot, CLASSIC_DARK);
        y += step;
    }
}

fn elevator_panel_rect() -> Rect {
    let width = 280.0;
    let height = 465.0_f32.min(ui_screen_height() - TOPBAR_HEIGHT - 20.0);
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        TOPBAR_HEIGHT + (ui_screen_height() - TOPBAR_HEIGHT - height) * 0.5,
        width,
        height,
    )
}

fn finance_panel_rect() -> Rect {
    let width = 343.0;
    let height = 364.0;
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn tenant_info_rect() -> Rect {
    let width = 510.0_f32.min(ui_screen_width() - 20.0);
    let height = 340.0_f32.min(ui_screen_height() - TOPBAR_HEIGHT - 16.0);
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn tenant_info_rename_rect() -> Rect {
    let panel = tenant_info_rect();
    Rect::new(panel.x + 14.0, panel.y + panel.h - 40.0, 112.0, 27.0)
}

fn tenant_info_ok_rect() -> Rect {
    let panel = tenant_info_rect();
    Rect::new(
        panel.x + panel.w - 126.0,
        panel.y + panel.h - 40.0,
        112.0,
        27.0,
    )
}

fn person_info_rect() -> Rect {
    let width = 340.0_f32.min(ui_screen_width() - 20.0);
    let height = 218.0_f32.min(ui_screen_height() - TOPBAR_HEIGHT - 16.0);
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn person_info_rename_rect() -> Rect {
    let panel = person_info_rect();
    Rect::new(panel.x + 28.0, panel.y + panel.h - 43.0, 90.0, 27.0)
}

fn person_info_ok_rect() -> Rect {
    let panel = person_info_rect();
    Rect::new(
        panel.x + panel.w - 118.0,
        panel.y + panel.h - 43.0,
        90.0,
        27.0,
    )
}

fn finance_ok_rect() -> Rect {
    let panel = finance_panel_rect();
    Rect::new(panel.x + 130.0, panel.y + 326.0, 94.0, 23.0)
}

fn promotion_dialog_rect() -> Rect {
    let width = 390.0;
    let height = 166.0;
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn overview_panel_rect() -> Rect {
    Rect::new(
        10.0,
        TOPBAR_HEIGHT + 10.0,
        OVERVIEW_PANEL_WIDTH,
        OVERVIEW_PANEL_HEIGHT.min(ui_screen_height() - TOPBAR_HEIGHT - 20.0),
    )
}

fn overview_tab_rect(index: usize) -> Rect {
    let panel = overview_panel_rect();
    let width = (panel.w - 8.0) / MODE_MENU_LABELS.len() as f32;
    Rect::new(
        panel.x + 4.0 + index as f32 * width,
        panel.y + 25.0,
        width,
        23.0,
    )
}

fn overview_map_rect() -> Rect {
    let panel = overview_panel_rect();
    Rect::new(
        panel.x + 7.0,
        panel.y + 51.0,
        panel.w - 14.0,
        panel.h - 78.0,
    )
}

fn promotion_ok_rect() -> Rect {
    let panel = promotion_dialog_rect();
    // Offset one pixel left to compensate for the classic button's dark
    // right-hand shadow, keeping its visual mass centered in the panel.
    Rect::new(
        panel.x + (panel.w - 96.0) * 0.5 - 1.0,
        panel.y + 128.0,
        96.0,
        26.0,
    )
}

fn vip_dialog_rect() -> Rect {
    let width = 540.0;
    let height = 170.0;
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn vip_dialog_ok_rect() -> Rect {
    let panel = vip_dialog_rect();
    Rect::new(
        panel.x + (panel.w - 96.0) * 0.5,
        panel.y + 132.0,
        96.0,
        26.0,
    )
}

fn fire_response_dialog_rect() -> Rect {
    let width = 540.0;
    let height = 210.0;
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn fire_helicopter_button_rect() -> Rect {
    let panel = fire_response_dialog_rect();
    Rect::new(panel.x + 116.0, panel.y + 160.0, 190.0, 30.0)
}

fn fire_security_button_rect() -> Rect {
    let panel = fire_response_dialog_rect();
    Rect::new(panel.x + 322.0, panel.y + 160.0, 190.0, 30.0)
}

fn bomb_response_dialog_rect() -> Rect {
    let width = 570.0;
    let height = 220.0;
    Rect::new(
        (ui_screen_width() - width) * 0.5,
        ((ui_screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
    )
}

fn bomb_pay_button_rect() -> Rect {
    let panel = bomb_response_dialog_rect();
    Rect::new(panel.x + 116.0, panel.y + 172.0, 190.0, 30.0)
}

fn bomb_search_button_rect() -> Rect {
    let panel = bomb_response_dialog_rect();
    Rect::new(panel.x + 322.0, panel.y + 172.0, 220.0, 30.0)
}

const fn promotion_rating_label(star_rating: u8) -> &'static str {
    match star_rating {
        2 => "Two Star Rating!",
        3 => "Three Star Rating!",
        4 => "Four Star Rating!",
        5 => "Five Star Rating!",
        6 => "TOWER Rating!",
        _ => "New Star Rating!",
    }
}

fn elevator_period_rect(panel: Rect, index: usize) -> Rect {
    Rect::new(
        panel.x + 12.0 + index as f32 * 42.0,
        panel.y + 57.0,
        39.0,
        26.0,
    )
}

fn draw_classic_group(rect: Rect, label: &str) {
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, CLASSIC_SHADOW);
    draw_rectangle(
        rect.x + 7.0,
        rect.y - 2.0,
        measure_text(label, None, 11, 1.0).width + 6.0,
        12.0,
        CLASSIC_FACE,
    );
    draw_text(label, rect.x + 10.0, rect.y + 8.0, 11.0, BLACK);
}

fn draw_spinner(x: f32, y: f32, value: u8) {
    let down = Rect::new(x, y, 15.0, 22.0);
    let up = Rect::new(x + 15.0, y, 15.0, 22.0);
    draw_classic_button(down, false);
    draw_classic_button(up, false);
    draw_text("-", down.x + 5.0, down.y + 15.0, 13.0, BLACK);
    draw_text("+", up.x + 3.0, up.y + 15.0, 13.0, BLACK);
    draw_rectangle(x + 32.0, y, 28.0, 22.0, WHITE);
    draw_rectangle_lines(x + 32.0, y, 28.0, 22.0, 1.0, BLACK);
    draw_text(value.to_string(), x + 38.0, y + 16.0, 13.0, BLACK);
}

fn format_floor(floor: i16) -> String {
    if floor < 0 {
        format!("B{}", floor.unsigned_abs())
    } else {
        floor.to_string()
    }
}

fn person_role(person: &Person, tower: &Tower) -> &'static str {
    let explicit = match person.role {
        PersonRole::Visitor => None,
        PersonRole::Salesman => Some("Salesman"),
        PersonRole::Woman => Some("Woman"),
        PersonRole::Child => Some("Child"),
        PersonRole::Security => Some("Security"),
        PersonRole::WomanWithChild => Some("Woman with Kid"),
        PersonRole::Homebody => Some("Homebody"),
        PersonRole::Housekeeper => Some("Housekeeper"),
    };
    if let Some(role) = explicit {
        return role;
    }
    let home_kind = tower
        .facilities()
        .iter()
        .find(|facility| facility.id == person.home_facility_id)
        .map(|facility| facility.kind);
    let destination_kind = tower
        .facilities()
        .iter()
        .find(|facility| facility.id == person.destination_facility_id)
        .map(|facility| facility.kind);
    person_role_for_kinds(home_kind, destination_kind)
}

const fn person_role_for_kinds(
    home_kind: Option<FacilityKind>,
    destination_kind: Option<FacilityKind>,
) -> &'static str {
    if matches!(home_kind, Some(FacilityKind::Housekeeping)) {
        return "Housekeeper";
    }
    if matches!(home_kind, Some(FacilityKind::Security)) {
        return "Security Guard";
    }
    match destination_kind {
        Some(FacilityKind::Office) => "Salesman",
        Some(FacilityKind::Condo) => "Resident",
        Some(FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite) => {
            "Hotel Guest"
        }
        Some(FacilityKind::Restaurant) => "Diner",
        Some(FacilityKind::FastFood | FacilityKind::Shop) => "Customer",
        Some(FacilityKind::Cinema) => "Moviegoer",
        Some(FacilityKind::PartyHall) => "Party Guest",
        Some(FacilityKind::Medical) => "Patient",
        Some(FacilityKind::Cathedral) => "Visitor",
        _ => "Visitor",
    }
}

fn person_trip_labels(person: &Person, tower: &Tower) -> (String, String) {
    if person.is_returning_home() {
        (
            person_place_label(person.destination_facility_id, tower),
            person_place_label(person.home_facility_id, tower),
        )
    } else {
        (
            person_place_label(person.home_facility_id, tower),
            person_place_label(person.destination_facility_id, tower),
        )
    }
}

fn person_place_label(facility_id: u64, tower: &Tower) -> String {
    if facility_id == 0 {
        return "Outside".to_owned();
    }
    tower
        .facilities()
        .iter()
        .find(|facility| facility.id == facility_id)
        .map_or_else(
            || "Outside".to_owned(),
            |facility| {
                format!(
                    "{}, Floor {}",
                    facility.kind.spec().name,
                    format_floor(facility.position.floor)
                )
            },
        )
}

fn elevator_compound_label_backplate(x: f32, y: f32, shaft_width: f32) -> Rect {
    Rect::new(x + (shaft_width - 32.0) * 0.5, y, 32.0, 36.0)
}

fn person_stress_fraction(person: &Person) -> f32 {
    person_stress_for(person.mood, person.wait_seconds)
}

fn person_stress_for(mood: PersonMood, wait_seconds: f32) -> f32 {
    let wait_stress = (wait_seconds / 30.0).clamp(0.0, 1.0);
    match mood {
        PersonMood::Calm => wait_stress.min(0.32),
        PersonMood::Concerned => wait_stress.max(0.5),
        PersonMood::Angry => wait_stress.max(0.82),
    }
}

fn person_evaluation_fraction(person: &Person) -> f32 {
    (1.0 - person_stress_fraction(person) * 0.72).clamp(0.2, 1.0)
}

fn bomb_search_floor_order(tower: &Tower, security_floor: i16) -> Vec<i16> {
    let floors = tower
        .floors()
        .iter()
        .map(|position| position.floor)
        .collect::<Vec<_>>();
    let mut security_floors = tower
        .facilities()
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Security)
        .map(|facility| facility.position.floor)
        .collect::<Vec<_>>();
    if security_floors.is_empty() {
        security_floors.push(security_floor);
    }
    bomb_search_floor_order_for_origins(floors, security_floors)
}

fn bomb_search_floor_order_for_origins(
    mut floors: Vec<i16>,
    mut security_floors: Vec<i16>,
) -> Vec<i16> {
    floors.sort_unstable();
    floors.dedup();
    security_floors.sort_unstable();
    let mut ordered = Vec::with_capacity(floors.len());
    while !floors.is_empty() {
        for _ in 0..BOMB_GUARDS_PER_SECURITY_OFFICE {
            for security_floor in &security_floors {
                let origin = simtower_core::floor_ordinal(*security_floor);
                let Some((index, _)) = floors.iter().enumerate().min_by_key(|(_, floor)| {
                    let ordinal = simtower_core::floor_ordinal(**floor);
                    (
                        (ordinal - origin).unsigned_abs(),
                        std::cmp::Reverse(ordinal),
                    )
                }) else {
                    return ordered;
                };
                ordered.push(floors.remove(index));
            }
        }
    }
    ordered
}

fn bomb_security_personnel_count(tower: &Tower) -> usize {
    tower
        .facilities()
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Security)
        .count()
        .saturating_mul(BOMB_GUARDS_PER_SECURITY_OFFICE)
        .max(1)
}

fn bomb_search_completed_floor_count(elapsed: f32, personnel: usize, floor_count: usize) -> usize {
    let completed_waves = (elapsed / BOMB_SEARCH_SECONDS_PER_FLOOR) as usize;
    completed_waves
        .saturating_mul(personnel.max(1))
        .min(floor_count)
}

struct TenantInfoDetails {
    fields: [(&'static str, String); 2],
    progress_value: u32,
    progress_max: u32,
    people_icons: u32,
}

fn tenant_info_details(
    facility: &Facility,
    patronage: u32,
    current_inside: u32,
    previous_profit: i64,
) -> TenantInfoDetails {
    let capacity = facility.kind.population_capacity().max(1);
    let current_inside = current_inside.min(capacity);
    match facility.kind {
        FacilityKind::Restaurant
        | FacilityKind::FastFood
        | FacilityKind::Shop
        | FacilityKind::Cinema
        | FacilityKind::PartyHall => TenantInfoDetails {
            fields: [
                ("Today's Patronage", patronage.to_string()),
                ("Yesterday's Profit", format_currency(previous_profit)),
            ],
            progress_value: patronage,
            progress_max: capacity,
            people_icons: proportional_people_icons(patronage, capacity),
        },
        FacilityKind::Office => TenantInfoDetails {
            fields: [
                ("Occupancy", format!("{current_inside} / {capacity}")),
                (
                    "Rent",
                    format_currency(tenant_rate(facility.kind, facility.price_level)),
                ),
            ],
            progress_value: current_inside,
            progress_max: capacity,
            people_icons: current_inside,
        },
        FacilityKind::Condo => TenantInfoDetails {
            fields: [
                (
                    "Status",
                    if facility.is_occupied() {
                        "Occupied"
                    } else {
                        "For Sale"
                    }
                    .to_owned(),
                ),
                (
                    "Price",
                    format_currency(tenant_rate(facility.kind, facility.price_level)),
                ),
            ],
            progress_value: u32::from(facility.is_occupied()),
            progress_max: 1,
            people_icons: if facility.is_occupied() { capacity } else { 0 },
        },
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => {
            TenantInfoDetails {
                fields: [
                    ("Now on board", format!("{current_inside} / {capacity}")),
                    (
                        "Rate",
                        format_currency(tenant_rate(facility.kind, facility.price_level)),
                    ),
                ],
                progress_value: current_inside,
                progress_max: capacity,
                people_icons: current_inside,
            }
        }
        _ => TenantInfoDetails {
            fields: [
                ("Status", "Occupied".to_owned()),
                ("Value", format_currency(previous_profit)),
            ],
            progress_value: current_inside,
            progress_max: capacity,
            people_icons: current_inside,
        },
    }
}

fn proportional_people_icons(value: u32, maximum: u32) -> u32 {
    (((value.min(maximum) as f32 / maximum.max(1) as f32) * 30.0).ceil() as u32).min(30)
}

fn tenant_rate(kind: FacilityKind, price_level: u8) -> i64 {
    let index = usize::from(price_level.min(3));
    match kind {
        FacilityKind::Office => [5_000, 10_000, 15_000, 20_000][index],
        FacilityKind::Condo => [100_000, 150_000, 200_000, 250_000][index],
        FacilityKind::HotelSingle => [1_000, 2_000, 3_000, 4_000][index],
        FacilityKind::HotelTwin => [1_500, 3_000, 4_500, 6_000][index],
        FacilityKind::HotelSuite => [3_000, 6_000, 9_000, 12_000][index],
        FacilityKind::Shop => [7_500, 15_000, 22_500, 30_000][index],
        _ => 0,
    }
}

fn tenant_evaluation(
    facility: &Facility,
    reachable: bool,
    patronage: u32,
    current_inside: u32,
) -> &'static str {
    if facility.is_burned() {
        return "Burned Area";
    }
    if facility.is_hotel_infested() {
        return "Cockroach infestation - bulldoze room";
    }
    if facility.is_hotel_dirty() {
        return "Waiting for housekeeping";
    }
    if !facility.is_occupied() {
        return if facility.kind == FacilityKind::Condo {
            "For Sale"
        } else {
            "For Rent"
        };
    }
    if !reachable {
        return "No transportation connected";
    }
    if matches!(
        facility.kind,
        FacilityKind::Restaurant
            | FacilityKind::FastFood
            | FacilityKind::Shop
            | FacilityKind::Cinema
            | FacilityKind::PartyHall
    ) {
        let capacity = facility.kind.population_capacity().max(1);
        return match patronage.saturating_mul(100) / capacity {
            75.. => "Business is very good!",
            45..=74 => "Business is good",
            20..=44 => "Business is average",
            _ => "Very few customers",
        };
    }
    if current_inside > 0 || facility.kind == FacilityKind::Condo {
        "Transportation access is good"
    } else {
        "Waiting for occupants"
    }
}

fn default_tenant_name(kind: FacilityKind, variant: Option<usize>) -> &'static str {
    const RESTAURANTS: [&str; 5] = [
        "English Pub",
        "French Restaurant",
        "Chinese Restaurant",
        "Sushi Bar",
        "Steak House",
    ];
    const FAST_FOOD: [&str; 5] = [
        "Japanese Soba",
        "Chinese Cafe",
        "Hamburger Stand",
        "Ice Cream",
        "Coffee Shop",
    ];
    const SHOPS: [&str; 11] = [
        "Men's Clothing",
        "Pet Store",
        "Flower Shop",
        "Book Store",
        "Drug Store",
        "Boutique",
        "Electronics",
        "Bank",
        "Hair Salon",
        "Post Office",
        "Sports Gear",
    ];
    let index = variant.unwrap_or(0);
    match kind {
        FacilityKind::Restaurant => RESTAURANTS[index % RESTAURANTS.len()],
        FacilityKind::FastFood => FAST_FOOD[index % FAST_FOOD.len()],
        FacilityKind::Shop => SHOPS[index % SHOPS.len()],
        FacilityKind::Office => "Office",
        FacilityKind::Condo => "Condominium",
        FacilityKind::HotelSingle => "Single Room",
        FacilityKind::HotelTwin => "Twin Room",
        FacilityKind::HotelSuite => "Hotel Suite",
        FacilityKind::Cinema => "Movie Theater",
        FacilityKind::PartyHall => "Party Hall",
        _ => kind.spec().name,
    }
}

fn with_alpha(mut color: Color, alpha: f32) -> Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

const fn elevator_display_alpha(visible: bool) -> f32 {
    if visible { 1.0 } else { 0.22 }
}

const fn elevator_infrastructure_alpha(_visible: bool) -> f32 {
    1.0
}

const fn hidden_elevator_outline_color() -> Color {
    Color::new(0.0, 0.0, 0.0, 1.0)
}

const fn elevator_number_texture_indices(default_waiting_floor: bool) -> (usize, usize, usize) {
    if default_waiting_floor {
        (3, 4, 5)
    } else {
        (0, 1, 2)
    }
}

fn is_vertical_transport(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::Stairs
            | FacilityKind::Escalator
            | FacilityKind::Elevator
            | FacilityKind::ServiceElevator
            | FacilityKind::ExpressElevator
    )
}

const fn is_elevator_kind(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::Elevator | FacilityKind::ServiceElevator | FacilityKind::ExpressElevator
    )
}

fn draw_atlas_icon(texture: &Texture2D, column: u8, row: u8, destination: Rect, tint: Color) {
    draw_atlas_icon_with_padding(texture, column, row, destination, tint, 3.0);
}

fn draw_atlas_icon_with_padding(
    texture: &Texture2D,
    column: u8,
    row: u8,
    destination: Rect,
    tint: Color,
    padding: f32,
) {
    draw_texture_ex(
        texture,
        destination.x + padding,
        destination.y + padding,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(
                destination.w - padding * 2.0,
                destination.h - padding * 2.0,
            )),
            source: Some(Rect::new(
                f32::from(column) * 32.0 * UI_TEXTURE_SCALE,
                f32::from(row) * 32.0 * UI_TEXTURE_SCALE,
                32.0 * UI_TEXTURE_SCALE,
                32.0 * UI_TEXTURE_SCALE,
            )),
            ..Default::default()
        },
    );
}

fn draw_cropped_icon(texture: &Texture2D, source: Rect, destination: Rect, tint: Color) {
    draw_texture_ex(
        texture,
        destination.x + 3.0,
        destination.y + 3.0,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(destination.w - 6.0, destination.h - 6.0)),
            source: Some(source),
            ..Default::default()
        },
    );
}

fn sprite_pixel_is_opaque(sprite: &Sprite, source: Rect, destination: Rect, point: Vec2) -> bool {
    let Some((source_x, source_y)) = scaled_source_pixel(source, destination, point) else {
        return false;
    };
    sprite.alpha.is_opaque(source_x, source_y)
}

fn scaled_source_pixel(source: Rect, destination: Rect, point: Vec2) -> Option<(usize, usize)> {
    if destination.w <= 0.0 || destination.h <= 0.0 || !destination.contains(point) {
        return None;
    }
    let u = ((point.x - destination.x) / destination.w).clamp(0.0, 0.999_999);
    let v = ((point.y - destination.y) / destination.h).clamp(0.0, 0.999_999);
    Some((
        (source.x + u * source.w).floor().max(0.0) as usize,
        (source.y + v * source.h).floor().max(0.0) as usize,
    ))
}

struct Sprite {
    resource_id: u16,
    texture: Texture2D,
    source: Rect,
    alpha: AlphaMask,
}

struct AlphaMask {
    width: usize,
    height: usize,
    opaque: Vec<bool>,
}

impl AlphaMask {
    fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Self, String> {
        let width = usize::try_from(width).map_err(|_| "sprite is too wide".to_owned())?;
        let height = usize::try_from(height).map_err(|_| "sprite is too tall".to_owned())?;
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| "sprite dimensions overflow".to_owned())?;
        if rgba.len() != pixel_count * 4 {
            return Err("sprite alpha data has the wrong length".to_owned());
        }
        Ok(Self {
            width,
            height,
            opaque: rgba.chunks_exact(4).map(|pixel| pixel[3] != 0).collect(),
        })
    }

    fn is_opaque(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height && self.opaque[y * self.width + x]
    }
}

struct OriginalAssets {
    sky: Texture2D,
    ground: Texture2D,
    intro_store: Texture2D,
    intro_title: Texture2D,
    intro_maxis: Texture2D,
    rain: [Texture2D; 10],
    clouds: [Texture2D; 4],
    city: Texture2D,
    tool_palette: Texture2D,
    tool_palette_selected: Texture2D,
    tool_palette_disabled: Texture2D,
    pointer_tools: Texture2D,
    pointer_tools_selected: Texture2D,
    elevator_cursor: Texture2D,
    magnifier_cursor: Texture2D,
    scaffolding: Texture2D,
    floor_strip: Texture2D,
    emergency_stairs: Texture2D,
    roof_crane: Texture2D,
    lobby_awning: Texture2D,
    santa: Texture2D,
    treasure: Texture2D,
    people: [Texture2D; 7],
    people_silhouette: [Texture2D; 2],
    queue_people: [Texture2D; 4],
    elevator_cars: Texture2D,
    elevator_shaft: Texture2D,
    elevator_numbers: [Texture2D; 6],
    tower_logo: Texture2D,
    finance_dialog: Texture2D,
    fire_large: [Texture2D; 4],
    fire_small: Texture2D,
    fire_helicopter: Texture2D,
    fire_alert: Texture2D,
    fire_aftermath: Texture2D,
    terrorist_portrait: Texture2D,
    fire_dispatch: Texture2D,
    vip_arrival: Texture2D,
    recycling_fill: [Texture2D; 5],
    recycling_truck: Texture2D,
    facilities: FacilitySprites,
    sounds: NativeSounds,
}

struct FacilitySprites {
    lobby_first_story: [Sprite; 3],
    lobby_second_story: [Sprite; 3],
    lobby_third_story: [Sprite; 3],
    office_vacant: Sprite,
    office: [Sprite; 6],
    condo_states: [Sprite; 15],
    hotel_single_vacant: [Sprite; 2],
    hotel_single_occupied: [Sprite; 2],
    hotel_twin_vacant: [Sprite; 4],
    hotel_twin_occupied: [Sprite; 4],
    hotel_suite_vacant: [Sprite; 2],
    hotel_suite_occupied: [Sprite; 2],
    restaurant_vacant: [Sprite; 5],
    restaurant_occupied: [Sprite; 5],
    fast_food_vacant: [Sprite; 5],
    fast_food_occupied: [Sprite; 5],
    shop_vacant: [Sprite; 11],
    #[allow(dead_code)] // retained as the catalog's pre-cropped middle-state views
    shop_occupied: [Sprite; 11],
    cinema_vacant: Sprite,
    #[allow(dead_code)] // retained for resource-audit parity; frames are selected dynamically
    cinema_occupied: Sprite,
    party_hall_vacant: Sprite,
    #[allow(dead_code)] // retained for resource-audit parity; frames are selected dynamically
    party_hall_occupied: Sprite,
    metro: [Sprite; 2],
    parking: [Sprite; 2],
    medical: [Sprite; 3],
    security: Sprite,
    recycling: Sprite,
    stairs: Sprite,
    escalator: Sprite,
    elevator: Sprite,
    service_elevator: Sprite,
    express_elevator: Sprite,
    housekeeping: Sprite,
    ramp: [Sprite; 3],
    cathedral: Sprite,
}

impl FacilitySprites {
    fn lobby_layer(
        &self,
        story: usize,
        stories: u8,
        floor: i16,
        star_rating: u8,
    ) -> (&Sprite, f32) {
        let style = lobby_style_index(star_rating);
        let (row, source_x) = lobby_artwork_source(story, stories, floor);
        let sprite = match row {
            0 => &self.lobby_first_story[style],
            1 => &self.lobby_second_story[style],
            _ => &self.lobby_third_story[style],
        };
        (sprite, source_x)
    }
}

fn lobby_artwork_source(story: usize, stories: u8, floor: i16) -> (usize, f32) {
    if floor > 1 {
        // The second 328-pixel chunk is the dedicated blue-window sky lobby.
        (0, LOBBY_CHUNK_WIDTH)
    } else if stories <= 1 {
        (0, 0.0)
    } else if story == 0 {
        // A grand lobby's bottom row is the third chunk of the first sheet.
        (0, LOBBY_CHUNK_WIDTH * 2.0)
    } else if story + 1 >= usize::from(stories) {
        // Both two- and three-story lobbies use the dedicated top-row sheet.
        (2, 0.0)
    } else {
        // Only a three-story lobby has a middle row.
        (1, LOBBY_CHUNK_WIDTH)
    }
}

#[cfg(test)]
fn lobby_artwork_resource_bounds(row: usize) -> f32 {
    match row {
        0 => 992.0,
        1 => 664.0,
        _ => 336.0,
    }
}

#[cfg(test)]
fn lobby_artwork_source_fits(row: usize, source_x: f32) -> bool {
    source_x + LOBBY_FACADE_OFFSET + LOBBY_FACADE_WIDTH <= lobby_artwork_resource_bounds(row)
}

#[cfg(test)]
fn assert_lobby_artwork_source(story: usize, stories: u8, floor: i16, expected: (usize, f32)) {
    let actual = lobby_artwork_source(story, stories, floor);
    assert_eq!(actual, expected);
    assert!(lobby_artwork_source_fits(actual.0, actual.1));
}

impl OriginalAssets {
    fn load() -> Result<Self, String> {
        Ok(Self {
            sky: texture_from_bmp(SKY_BMP)?,
            ground: texture_from_bmp(GROUND_BMP)?,
            intro_store: texture_from_bmp(INTRO_STORE_BMP)?,
            intro_title: texture_from_bmp(INTRO_TITLE_BMP)?,
            intro_maxis: texture_from_bmp(INTRO_MAXIS_BMP)?,
            rain: RAIN_BMPS
                .map(texture_from_bmp)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "rain texture count mismatch".to_owned())?,
            clouds: CLOUD_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "cloud texture count mismatch".to_owned())?,
            city: texture_from_bmp_with_color_transparency(CITY_BMP, [138, 212, 255])?,
            tool_palette: texture_from_bmp_for_ui(PALETTE_BMP)?,
            tool_palette_selected: texture_from_bmp_for_ui(PALETTE_SELECTED_BMP)?,
            tool_palette_disabled: texture_from_bmp_for_ui(PALETTE_DISABLED_BMP)?,
            pointer_tools: texture_from_bmp_for_ui(POINTER_TOOLS_BMP)?,
            pointer_tools_selected: texture_from_bmp_for_ui(POINTER_TOOLS_SELECTED_BMP)?,
            elevator_cursor: texture_from_hand_cursor(POINTER_TOOLS_BMP)?,
            magnifier_cursor: texture_from_magnifier_cursor(POINTER_TOOLS_BMP)?,
            scaffolding: texture_from_bmp(SCAFFOLD_BMP)?,
            floor_strip: texture_from_bmp(FLOOR_STRIP_BMP)?,
            emergency_stairs: texture_from_bmp_with_white_transparency(EMERGENCY_STAIRS_BMP)?,
            roof_crane: texture_from_bmp_with_white_transparency(ROOF_CRANE_BMP)?,
            lobby_awning: texture_from_bmp_with_white_transparency(LOBBY_AWNING_BMP)?,
            santa: texture_from_bmp_with_white_transparency(SANTA_BMP)?,
            treasure: texture_from_bmp(TREASURE_BMP)?,
            people: PEOPLE_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "people texture count mismatch".to_owned())?,
            people_silhouette: PEOPLE_SILHOUETTE_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "silhouette texture count mismatch".to_owned())?,
            queue_people: [
                texture_from_queue_bmp(QUEUE_PEOPLE_BMPS[0], None)?,
                texture_from_queue_bmp(QUEUE_PEOPLE_BMPS[1], Some(QUEUE_CONCERNED_PINK))?,
                texture_from_queue_bmp(QUEUE_PEOPLE_BMPS[2], None)?,
                texture_from_queue_bmp(QUEUE_PEOPLE_BMPS[3], None)?,
            ],
            elevator_cars: texture_from_bmp_with_white_transparency(ELEVATOR_CARS_BMP)?,
            elevator_shaft: texture_from_bmp_with_white_transparency(ELEVATOR_SHAFT_BMP)?,
            elevator_numbers: [
                texture_from_bmp(ELEVATOR_NUMBER_BMPS[0])?,
                texture_from_elevator_glyph_bmp(ELEVATOR_NUMBER_BMPS[1])?,
                texture_from_elevator_glyph_bmp(ELEVATOR_NUMBER_BMPS[2])?,
                texture_from_bmp(ELEVATOR_NUMBER_BMPS[3])?,
                texture_from_elevator_glyph_bmp(ELEVATOR_NUMBER_BMPS[4])?,
                texture_from_elevator_glyph_bmp(ELEVATOR_NUMBER_BMPS[5])?,
            ],
            tower_logo: texture_from_bmp_with_white_transparency(TOWER_LOGO_BMP)?,
            finance_dialog: texture_from_bmp_for_ui(FINANCE_DIALOG_BMP)?,
            fire_large: FIRE_LARGE_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "fire texture count mismatch".to_owned())?,
            fire_small: texture_from_bmp_with_white_transparency(FIRE_SMALL_BMP)?,
            fire_helicopter: texture_from_bmp_with_white_transparency(FIRE_HELICOPTER_BMP)?,
            fire_alert: texture_from_bmp_with_white_transparency(FIRE_ALERT_BMP)?,
            fire_aftermath: texture_from_bmp(FIRE_AFTERMATH_BMP)?,
            terrorist_portrait: texture_from_bmp_with_white_transparency(TERRORIST_PORTRAIT_BMP)?,
            fire_dispatch: texture_from_bmp_with_white_transparency(FIRE_DISPATCH_BMP)?,
            vip_arrival: texture_from_bmp_with_white_transparency(VIP_ARRIVAL_BMP)?,
            recycling_fill: [
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[1])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[2])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[3])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[4])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[5])?,
            ],
            recycling_truck: texture_from_bmp_with_white_transparency(RECYCLING_TRUCK_BMP)?,
            facilities: FacilitySprites {
                lobby_first_story: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2536,
                        LOBBY_FIRST_STORY_BMPS[0],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2537,
                        LOBBY_FIRST_STORY_BMPS[1],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2538,
                        LOBBY_FIRST_STORY_BMPS[2],
                    )?,
                ],
                lobby_second_story: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2600,
                        LOBBY_SECOND_STORY_BMPS[0],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2601,
                        LOBBY_SECOND_STORY_BMPS[1],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2602,
                        LOBBY_SECOND_STORY_BMPS[2],
                    )?,
                ],
                lobby_third_story: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2664,
                        LOBBY_THIRD_STORY_BMPS[0],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2665,
                        LOBBY_THIRD_STORY_BMPS[1],
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2666,
                        LOBBY_THIRD_STORY_BMPS[2],
                    )?,
                ],
                office_vacant: facility_sprite_from_bmp_with_id(
                    FacilityKind::Office,
                    1451,
                    OFFICE_VACANT_BMP,
                )?,
                office: [
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[0],
                        0.0,
                        OFFICE_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[1],
                        144.0,
                        OFFICE_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[2],
                        0.0,
                        OFFICE_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[3],
                        144.0,
                        OFFICE_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[4],
                        0.0,
                        OFFICE_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Office,
                        OFFICE_VARIANT_IDS[5],
                        144.0,
                        OFFICE_3_BMP,
                    )?,
                ],
                condo_states: facility_sprite_array(FacilityKind::Condo, 1576, CONDO_STATE_BMPS)?,
                hotel_single_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSingle,
                        HOTEL_SINGLE_VARIANT_IDS[0],
                        HOTEL_SINGLE_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSingle,
                        HOTEL_SINGLE_VARIANT_IDS[1],
                        HOTEL_SINGLE_2_BMP,
                    )?,
                ],
                hotel_single_occupied: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSingle,
                        HOTEL_SINGLE_VARIANT_IDS[0],
                        HOTEL_SINGLE_OCCUPIED_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSingle,
                        HOTEL_SINGLE_VARIANT_IDS[1],
                        HOTEL_SINGLE_OCCUPIED_2_BMP,
                    )?,
                ],
                hotel_twin_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[0],
                        HOTEL_TWIN_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[1],
                        HOTEL_TWIN_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[2],
                        HOTEL_TWIN_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[3],
                        HOTEL_TWIN_4_BMP,
                    )?,
                ],
                hotel_twin_occupied: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[0],
                        HOTEL_TWIN_OCCUPIED_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[1],
                        HOTEL_TWIN_OCCUPIED_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[2],
                        HOTEL_TWIN_OCCUPIED_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelTwin,
                        HOTEL_TWIN_VARIANT_IDS[3],
                        HOTEL_TWIN_OCCUPIED_4_BMP,
                    )?,
                ],
                hotel_suite_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSuite,
                        HOTEL_SUITE_VARIANT_IDS[0],
                        HOTEL_SUITE_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSuite,
                        HOTEL_SUITE_VARIANT_IDS[1],
                        HOTEL_SUITE_2_BMP,
                    )?,
                ],
                hotel_suite_occupied: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSuite,
                        HOTEL_SUITE_VARIANT_IDS[0],
                        HOTEL_SUITE_OCCUPIED_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::HotelSuite,
                        HOTEL_SUITE_VARIANT_IDS[1],
                        HOTEL_SUITE_OCCUPIED_2_BMP,
                    )?,
                ],
                restaurant_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[0],
                        RESTAURANT_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[1],
                        RESTAURANT_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[2],
                        RESTAURANT_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[3],
                        RESTAURANT_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[4],
                        RESTAURANT_5_BMP,
                    )?,
                ],
                restaurant_occupied: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[0],
                        RESTAURANT_OCCUPIED_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[1],
                        RESTAURANT_OCCUPIED_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[2],
                        RESTAURANT_OCCUPIED_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[3],
                        RESTAURANT_OCCUPIED_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Restaurant,
                        RESTAURANT_VARIANT_IDS[4],
                        RESTAURANT_OCCUPIED_5_BMP,
                    )?,
                ],
                fast_food_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[0],
                        FAST_FOOD_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[1],
                        FAST_FOOD_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[2],
                        FAST_FOOD_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[3],
                        FAST_FOOD_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[4],
                        FAST_FOOD_5_BMP,
                    )?,
                ],
                fast_food_occupied: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[0],
                        FAST_FOOD_OCCUPIED_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[1],
                        FAST_FOOD_OCCUPIED_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[2],
                        FAST_FOOD_OCCUPIED_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[3],
                        FAST_FOOD_OCCUPIED_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::FastFood,
                        FAST_FOOD_VARIANT_IDS[4],
                        FAST_FOOD_OCCUPIED_5_BMP,
                    )?,
                ],
                shop_vacant: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[0],
                        SHOP_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[1],
                        SHOP_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[2],
                        SHOP_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[3],
                        SHOP_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[4],
                        SHOP_5_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[5],
                        SHOP_6_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[6],
                        SHOP_7_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[7],
                        SHOP_8_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[8],
                        SHOP_9_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[9],
                        SHOP_10_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[10],
                        SHOP_11_BMP,
                    )?,
                ],
                shop_occupied: [
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[0],
                        96.0,
                        SHOP_1_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[1],
                        96.0,
                        SHOP_2_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[2],
                        96.0,
                        SHOP_3_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[3],
                        96.0,
                        SHOP_4_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[4],
                        96.0,
                        SHOP_5_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[5],
                        96.0,
                        SHOP_6_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[6],
                        96.0,
                        SHOP_7_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[7],
                        96.0,
                        SHOP_8_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[8],
                        96.0,
                        SHOP_9_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[9],
                        96.0,
                        SHOP_10_BMP,
                    )?,
                    facility_sprite_from_bmp_with_source(
                        FacilityKind::Shop,
                        SHOP_VARIANT_IDS[10],
                        96.0,
                        SHOP_11_BMP,
                    )?,
                ],
                cinema_vacant: facility_sprite_from_vertical_bmps_with_source(
                    FacilityKind::Cinema,
                    &[CINEMA_UPPER_BMP, CINEMA_LOWER_BMP],
                    false,
                    192.0,
                )?,
                cinema_occupied: facility_sprite_from_vertical_bmps_with_source(
                    FacilityKind::Cinema,
                    &[CINEMA_UPPER_BMP, CINEMA_LOWER_BMP],
                    false,
                    384.0,
                )?,
                party_hall_vacant: facility_sprite_from_vertical_bmps_with_source(
                    FacilityKind::PartyHall,
                    &[PARTY_HALL_UPPER_BMP, PARTY_HALL_LOWER_BMP],
                    false,
                    192.0,
                )?,
                party_hall_occupied: facility_sprite_from_vertical_bmps_with_source(
                    FacilityKind::PartyHall,
                    &[PARTY_HALL_UPPER_BMP, PARTY_HALL_LOWER_BMP],
                    false,
                    384.0,
                )?,
                metro: [
                    facility_sprite_from_vertical_bmps(
                        FacilityKind::Metro,
                        &[METRO_UPPER_BMP, METRO_MIDDLE_BMP, METRO_LOWER_BMP],
                        false,
                    )?,
                    facility_sprite_from_vertical_bmps(
                        FacilityKind::Metro,
                        &[
                            METRO_OCCUPIED_UPPER_BMP,
                            METRO_OCCUPIED_MIDDLE_BMP,
                            METRO_OCCUPIED_LOWER_BMP,
                        ],
                        false,
                    )?,
                ],
                parking: [
                    facility_sprite_from_bmp(FacilityKind::Parking, PARKING_BMP)?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Parking,
                        1705,
                        PARKING_OCCUPIED_BMP,
                    )?,
                ],
                medical: [
                    facility_sprite_from_bmp(FacilityKind::Medical, MEDICAL_BMP)?,
                    facility_sprite_from_bmp_with_id(FacilityKind::Medical, 1833, MEDICAL_2_BMP)?,
                    facility_sprite_from_bmp_with_id(FacilityKind::Medical, 1834, MEDICAL_3_BMP)?,
                ],
                security: facility_sprite_from_bmp(FacilityKind::Security, SECURITY_BMP)?,
                recycling: facility_sprite_from_bmp(FacilityKind::Recycling, RECYCLING_BMP)?,
                stairs: facility_sprite_from_vertical_bmps(
                    FacilityKind::Stairs,
                    &[STAIRS_UPPER_BMP, STAIRS_LOWER_BMP],
                    true,
                )?,
                escalator: facility_sprite_from_vertical_bmps(
                    FacilityKind::Escalator,
                    &[ESCALATOR_UPPER_BMP, ESCALATOR_LOWER_BMP],
                    true,
                )?,
                elevator: facility_sprite_from_bmp(FacilityKind::Elevator, ELEVATOR_BMP)?,
                service_elevator: facility_sprite_from_bmp(
                    FacilityKind::ServiceElevator,
                    SERVICE_ELEVATOR_BMP,
                )?,
                express_elevator: facility_sprite_from_bmp(
                    FacilityKind::ExpressElevator,
                    EXPRESS_ELEVATOR_BMP,
                )?,
                housekeeping: facility_sprite_from_bmp(
                    FacilityKind::Housekeeping,
                    HOUSEKEEPING_BMP,
                )?,
                ramp: [
                    facility_sprite_from_bmp(FacilityKind::Ramp, RAMP_BMP)?,
                    facility_sprite_from_bmp_with_id(FacilityKind::Ramp, 3817, RAMP_2_BMP)?,
                    facility_sprite_from_bmp_with_id(FacilityKind::Ramp, 3818, RAMP_3_BMP)?,
                ],
                cathedral: facility_sprite_from_vertical_bmps(
                    FacilityKind::Cathedral,
                    &[
                        CATHEDRAL_1_BMP,
                        CATHEDRAL_2_BMP,
                        CATHEDRAL_3_BMP,
                        CATHEDRAL_4_BMP,
                        CATHEDRAL_5_BMP,
                    ],
                    false,
                )?,
            },
            sounds: NativeSounds::install()?,
        })
    }

    fn facility_sprite(&self, kind: FacilityKind) -> &Sprite {
        let sprite = match kind {
            FacilityKind::Lobby => &self.facilities.lobby_first_story[0],
            FacilityKind::Office => &self.facilities.office[0],
            FacilityKind::Condo => &self.facilities.condo_states[3],
            FacilityKind::HotelSingle => &self.facilities.hotel_single_vacant[0],
            FacilityKind::HotelTwin => &self.facilities.hotel_twin_vacant[0],
            FacilityKind::HotelSuite => &self.facilities.hotel_suite_vacant[0],
            FacilityKind::Restaurant => &self.facilities.restaurant_vacant[0],
            FacilityKind::FastFood => &self.facilities.fast_food_vacant[0],
            FacilityKind::Shop => &self.facilities.shop_vacant[0],
            FacilityKind::Cinema => &self.facilities.cinema_vacant,
            FacilityKind::PartyHall => &self.facilities.party_hall_vacant,
            FacilityKind::Metro => &self.facilities.metro[0],
            FacilityKind::Parking => &self.facilities.parking[0],
            FacilityKind::Medical => &self.facilities.medical[0],
            FacilityKind::Security => &self.facilities.security,
            FacilityKind::Recycling => &self.facilities.recycling,
            FacilityKind::Stairs => &self.facilities.stairs,
            FacilityKind::Escalator => &self.facilities.escalator,
            FacilityKind::Elevator => &self.facilities.elevator,
            FacilityKind::ServiceElevator => &self.facilities.service_elevator,
            FacilityKind::ExpressElevator => &self.facilities.express_elevator,
            FacilityKind::Housekeeping => &self.facilities.housekeeping,
            FacilityKind::Ramp => &self.facilities.ramp[0],
            FacilityKind::Cathedral => &self.facilities.cathedral,
        };
        debug_assert_eq!(sprite.resource_id, facility_resource_ids(kind).bitmap);
        sprite
    }

    fn facility_sprite_for(
        &self,
        facility: &Facility,
        crowd: FacilityCrowd,
        tenant_variant: Option<usize>,
        minute_of_day: u16,
        visual_load: usize,
    ) -> (&Sprite, Rect) {
        if facility.kind == FacilityKind::Office {
            if !facility.is_occupied() {
                let sprite = &self.facilities.office_vacant;
                let mut source = sprite.source;
                if night_sprite_active(minute_of_day) {
                    source.x += source.w;
                }
                return (sprite, source);
            }
            let style = tenant_variant
                .unwrap_or_else(|| facility_variant_index(facility, self.facilities.office.len()))
                % self.facilities.office.len();
            let sprite = &self.facilities.office[style];
            let frame = office_sprite_frame(minute_of_day);
            let mut source = sprite.source;
            source.x = frame as f32 * source.w;
            return (sprite, source);
        }
        if facility.kind == FacilityKind::Restaurant {
            let style = tenant_variant.unwrap_or_else(|| {
                facility_variant_index(facility, self.facilities.restaurant_vacant.len())
            }) % self.facilities.restaurant_vacant.len();
            return match crowd {
                FacilityCrowd::Empty => {
                    let sprite = &self.facilities.restaurant_vacant[style];
                    (sprite, sprite.source)
                }
                FacilityCrowd::Light => {
                    let sprite = &self.facilities.restaurant_vacant[style];
                    let mut source = sprite.source;
                    source.x += source.w;
                    (sprite, source)
                }
                FacilityCrowd::Heavy => {
                    let sprite = &self.facilities.restaurant_occupied[style];
                    (sprite, sprite.source)
                }
                FacilityCrowd::Closed => {
                    let sprite = &self.facilities.restaurant_occupied[style];
                    let mut source = sprite.source;
                    source.x += source.w;
                    (sprite, source)
                }
            };
        }

        if facility.kind == FacilityKind::FastFood {
            let style = tenant_variant.unwrap_or_else(|| {
                facility_variant_index(facility, self.facilities.fast_food_vacant.len())
            }) % self.facilities.fast_food_vacant.len();
            return match crowd {
                FacilityCrowd::Empty => {
                    let sprite = &self.facilities.fast_food_vacant[style];
                    (sprite, sprite.source)
                }
                FacilityCrowd::Light => {
                    let sprite = &self.facilities.fast_food_vacant[style];
                    let mut source = sprite.source;
                    source.x += source.w;
                    (sprite, source)
                }
                FacilityCrowd::Heavy => {
                    let sprite = &self.facilities.fast_food_occupied[style];
                    (sprite, sprite.source)
                }
                FacilityCrowd::Closed => {
                    let sprite = &self.facilities.fast_food_occupied[style];
                    let mut source = sprite.source;
                    source.x += source.w;
                    (sprite, source)
                }
            };
        }

        if facility.kind == FacilityKind::Condo {
            let style = tenant_variant.unwrap_or_else(|| facility_variant_index(facility, 3)) % 3;
            let night = night_sprite_active(minute_of_day);
            let state = if !facility.is_occupied() {
                if night { 4 } else { 3 }
            } else if night {
                2
            } else if visual_load > 0 {
                1
            } else {
                0
            };
            let sprite = &self.facilities.condo_states[style * 5 + state];
            return (sprite, sprite.source);
        }

        if matches!(
            facility.kind,
            FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite
        ) {
            let (vacant, occupied): (&[Sprite], &[Sprite]) = match facility.kind {
                FacilityKind::HotelSingle => (
                    &self.facilities.hotel_single_vacant,
                    &self.facilities.hotel_single_occupied,
                ),
                FacilityKind::HotelTwin => (
                    &self.facilities.hotel_twin_vacant,
                    &self.facilities.hotel_twin_occupied,
                ),
                FacilityKind::HotelSuite => (
                    &self.facilities.hotel_suite_vacant,
                    &self.facilities.hotel_suite_occupied,
                ),
                _ => unreachable!(),
            };
            let style = tenant_variant
                .unwrap_or_else(|| facility_variant_index(facility, vacant.len()))
                % vacant.len();
            if !facility.is_occupied()
                && !facility.is_hotel_dirty()
                && !facility.is_hotel_infested()
            {
                let sprite = &vacant[style];
                return (sprite, sprite.source);
            }
            let sprite = &occupied[style];
            let mut source = sprite.source;
            let frame = hotel_sprite_frame(
                minute_of_day,
                visual_load,
                facility.is_hotel_dirty(),
                facility.is_hotel_infested(),
            );
            source.x += frame as f32 * source.w;
            return (sprite, source);
        }

        if facility.kind == FacilityKind::Shop {
            let variants = &self.facilities.shop_vacant;
            let index = tenant_variant
                .unwrap_or_else(|| facility_variant_index(facility, variants.len()))
                % variants.len();
            let sprite = &variants[index];
            let frame = match crowd {
                FacilityCrowd::Empty | FacilityCrowd::Closed => 0,
                FacilityCrowd::Light => 1,
                FacilityCrowd::Heavy => 2,
            };
            let mut source = sprite.source;
            source.x += frame as f32 * source.w;
            return (sprite, source);
        }

        if facility.kind == FacilityKind::Cinema {
            let sprite = &self.facilities.cinema_vacant;
            let frame = match crowd {
                FacilityCrowd::Closed => 0,
                FacilityCrowd::Empty => 1,
                FacilityCrowd::Light => 2,
                FacilityCrowd::Heavy => 3,
            };
            let mut source = sprite.source;
            source.x = frame as f32 * source.w;
            return (sprite, source);
        }

        if facility.kind == FacilityKind::PartyHall {
            let sprite = &self.facilities.party_hall_vacant;
            let frame = match crowd {
                FacilityCrowd::Closed => 0,
                FacilityCrowd::Empty | FacilityCrowd::Light => 1,
                FacilityCrowd::Heavy => 2,
            };
            let mut source = sprite.source;
            source.x = frame as f32 * source.w;
            return (sprite, source);
        }

        if facility.kind == FacilityKind::Metro {
            let sprite = &self.facilities.metro[usize::from(visual_load > 0)];
            return (sprite, sprite.source);
        }

        if facility.kind == FacilityKind::Parking {
            let sprite = &self.facilities.parking[usize::from(visual_load != 0)];
            let mut source = sprite.source;
            if visual_load != 0 {
                let frame = if visual_load == 14 {
                    0
                } else {
                    visual_load.min(13)
                };
                source.x = frame as f32 * source.w;
            }
            return (sprite, source);
        }

        if facility.kind == FacilityKind::Medical {
            let sprite = &self.facilities.medical[visual_load.min(2)];
            return (sprite, sprite.source);
        }

        if facility.kind == FacilityKind::Ramp {
            let sprite = &self.facilities.ramp[visual_load.min(2)];
            return (sprite, sprite.source);
        }

        if facility.kind == FacilityKind::Cathedral {
            let sprite = &self.facilities.cathedral;
            let mut source = sprite.source;
            if visual_load > 0 {
                source.x += source.w;
            }
            return (sprite, source);
        }

        // Tenant leasing and visible population are distinct. A connected
        // space may be leased after its move-in delay, but populated artwork
        // appears only after a person has actually reached it.
        let occupied = !matches!(crowd, FacilityCrowd::Closed | FacilityCrowd::Empty);
        let variants: Option<&[Sprite]> = match facility.kind {
            FacilityKind::Office => unreachable!("office density handled above"),
            FacilityKind::Condo
            | FacilityKind::HotelSingle
            | FacilityKind::HotelTwin
            | FacilityKind::HotelSuite
            | FacilityKind::Shop
            | FacilityKind::Cinema
            | FacilityKind::PartyHall
            | FacilityKind::Metro
            | FacilityKind::Parking
            | FacilityKind::Medical
            | FacilityKind::Ramp
            | FacilityKind::Cathedral => unreachable!("dynamic state handled above"),
            FacilityKind::Restaurant if occupied => Some(&self.facilities.restaurant_occupied),
            FacilityKind::Restaurant => Some(&self.facilities.restaurant_vacant),
            FacilityKind::FastFood if occupied => Some(&self.facilities.fast_food_occupied),
            FacilityKind::FastFood => Some(&self.facilities.fast_food_vacant),
            FacilityKind::Lobby
            | FacilityKind::Security
            | FacilityKind::Recycling
            | FacilityKind::Stairs
            | FacilityKind::Escalator
            | FacilityKind::Elevator
            | FacilityKind::ServiceElevator
            | FacilityKind::ExpressElevator
            | FacilityKind::Housekeeping => None,
        };
        let sprite = variants.map_or_else(
            || self.facility_sprite(facility.kind),
            |variants| {
                let index = facility_variant_index(facility, variants.len());
                &variants[index]
            },
        );
        (sprite, sprite.source)
    }

    fn play_construction(&self, kind: FacilityKind) {
        self.sounds.play_construction(kind);
    }

    fn play_floor_construction(&self) {
        self.sounds.play_general_construction();
    }

    fn play_demolition(&self) {
        self.sounds.play_demolition();
    }

    fn play_no_money(&self) {
        self.sounds.play_no_money();
    }
}

struct NativeSounds {
    opening_theme: PathBuf,
    opening_theme_process: RefCell<Option<Child>>,
    construction: PathBuf,
    lobby_segment: PathBuf,
    demolition: PathBuf,
    no_money: PathBuf,
    payment: PathBuf,
    fire_alert: PathBuf,
    fire_response: PathBuf,
    bomb_threat: PathBuf,
    bomb_armed: PathBuf,
    bomb_explosion: PathBuf,
    elevator_move: PathBuf,
    elevator_open: [PathBuf; 2],
    escalator_move: PathBuf,
    crowd: PathBuf,
    parking_arrival: PathBuf,
    parking_departure: PathBuf,
    muted: Cell<bool>,
}

impl NativeSounds {
    fn install() -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("opentower-audio-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let opening_theme = directory.join("opening-theme-20000.wav");
        let construction = directory.join(format!("construction-{CONSTRUCTION_SOUND_ID}.wav"));
        let lobby_segment = directory.join(format!("lobby-segment-{LOBBY_SEGMENT_SOUND_ID}.wav"));
        let demolition = directory.join(format!("demolition-{DEMOLITION_SOUND_ID}.wav"));
        let no_money = directory.join(format!("no-money-{NO_MONEY_SOUND_ID}.wav"));
        let payment = directory.join(format!("payment-{PAYMENT_SOUND_ID}.wav"));
        let fire_alert = directory.join(format!("fire-alert-{FIRE_ALERT_SOUND_ID}.wav"));
        let fire_response = directory.join(format!("fire-response-{FIRE_RESPONSE_SOUND_ID}.wav"));
        let bomb_threat = directory.join("bomb-threat-10003.wav");
        let bomb_armed = directory.join("bomb-armed-10000.wav");
        let bomb_explosion = directory.join("bomb-explosion-10004.wav");
        let elevator_move = directory.join(format!("elevator-move-{ELEVATOR_MOVE_SOUND_ID}.wav"));
        let elevator_open =
            ELEVATOR_OPEN_SOUND_IDS.map(|id| directory.join(format!("elevator-open-{id}.wav")));
        let escalator_move = directory.join("escalator-move.wav");
        let crowd = directory.join(format!("crowd-{CROWD_SOUND_ID}.wav"));
        let parking_arrival = directory.join("parking-arrival-1704.wav");
        let parking_departure = directory.join("parking-departure-1705.wav");
        std::fs::write(&opening_theme, OPENING_THEME_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&construction, CONSTRUCTION_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&lobby_segment, LOBBY_SEGMENT_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&demolition, DEMOLITION_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&no_money, NO_MONEY_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&payment, PAYMENT_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&fire_alert, FIRE_ALERT_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&fire_response, FIRE_RESPONSE_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&bomb_threat, BOMB_THREAT_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&bomb_armed, BOMB_ARMED_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&bomb_explosion, BOMB_EXPLOSION_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&elevator_move, ELEVATOR_MOVE_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        for (path, bytes) in elevator_open
            .iter()
            .zip([ELEVATOR_OPEN_1_WAV, ELEVATOR_OPEN_2_WAV])
        {
            std::fs::write(path, bytes.bytes()?).map_err(|error| error.to_string())?;
        }
        // SimTower has no separate escalator WAVE resource. Its elevator motor
        // recording is the closest original mechanical cue, so use a quieter
        // copy and only trigger it when a rider steps onto an escalator.
        let escalator_bytes = attenuate_unsigned_pcm8_wav(ELEVATOR_MOVE_WAV.bytes()?, 0.28);
        std::fs::write(&escalator_move, escalator_bytes).map_err(|error| error.to_string())?;
        std::fs::write(&crowd, CROWD_1_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&parking_arrival, PARKING_ARRIVAL_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&parking_departure, PARKING_DEPARTURE_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            opening_theme,
            opening_theme_process: RefCell::new(None),
            construction,
            lobby_segment,
            demolition,
            no_money,
            payment,
            fire_alert,
            fire_response,
            bomb_threat,
            bomb_armed,
            bomb_explosion,
            elevator_move,
            elevator_open,
            escalator_move,
            crowd,
            parking_arrival,
            parking_departure,
            muted: Cell::new(false),
        })
    }

    fn is_muted(&self) -> bool {
        self.muted.get()
    }

    fn set_muted(&self, muted: bool) {
        self.muted.set(muted);
        if muted {
            self.stop_opening_theme();
        }
    }

    fn toggle_muted(&self) -> bool {
        let muted = !self.muted.get();
        self.muted.set(muted);
        if muted {
            self.stop_opening_theme();
        }
        muted
    }

    fn play_opening_theme(&self) {
        self.stop_opening_theme();
        if self.is_muted() {
            return;
        }
        *self.opening_theme_process.borrow_mut() = spawn_native_sound(&self.opening_theme);
    }

    fn stop_opening_theme(&self) {
        if let Some(mut child) = self.opening_theme_process.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn play_construction(&self, kind: FacilityKind) {
        if self.is_muted() {
            return;
        }
        self.play_general_construction();
        if kind == FacilityKind::Lobby {
            play_native_sound_async(self.lobby_segment.clone());
        }
    }

    fn play_general_construction(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.construction.clone());
    }

    fn play_demolition(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.demolition.clone());
    }

    fn play_no_money(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.no_money.clone());
    }

    fn play_payment(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.payment.clone());
    }

    fn play_fire_alert(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.fire_alert.clone());
    }

    fn play_fire_response(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.fire_response.clone());
    }

    fn play_bomb_threat(&self) {
        if !self.is_muted() {
            play_native_sound_async(self.bomb_threat.clone());
        }
    }

    fn play_bomb_armed(&self) {
        if !self.is_muted() {
            play_native_sound_async(self.bomb_armed.clone());
        }
    }

    fn play_bomb_explosion(&self) {
        if !self.is_muted() {
            play_native_sound_async(self.bomb_explosion.clone());
        }
    }

    fn play_elevator_move(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.elevator_move.clone());
    }

    fn play_elevator_open(&self, variant: usize) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.elevator_open[variant % self.elevator_open.len()].clone());
    }

    fn play_escalator_move(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.escalator_move.clone());
    }

    fn play_crowd(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.crowd.clone());
    }

    fn play_parking_arrival(&self) {
        if !self.is_muted() {
            play_native_sound_async(self.parking_arrival.clone());
        }
    }

    fn play_parking_departure(&self) {
        if !self.is_muted() {
            play_native_sound_async(self.parking_departure.clone());
        }
    }
}

impl Drop for NativeSounds {
    fn drop(&mut self) {
        self.stop_opening_theme();
    }
}

fn play_native_sound_async(path: PathBuf) {
    let _ = std::thread::spawn(move || play_native_sound(path));
}

fn attenuate_unsigned_pcm8_wav(bytes: &[u8], gain: f32) -> Vec<u8> {
    let mut quiet = bytes.to_vec();
    let mut cursor = 12_usize;
    while cursor + 8 <= quiet.len() {
        let chunk_size = u32::from_le_bytes([
            quiet[cursor + 4],
            quiet[cursor + 5],
            quiet[cursor + 6],
            quiet[cursor + 7],
        ]) as usize;
        let data_start = cursor + 8;
        let data_end = data_start.saturating_add(chunk_size).min(quiet.len());
        if &quiet[cursor..cursor + 4] == b"data" {
            for sample in &mut quiet[data_start..data_end] {
                let centered = f32::from(*sample) - 128.0;
                *sample = (128.0 + centered * gain.clamp(0.0, 1.0))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            break;
        }
        cursor = data_end + (chunk_size & 1);
    }
    quiet
}

#[cfg(target_os = "macos")]
fn spawn_native_sound(path: &Path) -> Option<Child> {
    Command::new("afplay").arg(path).spawn().ok()
}

#[cfg(target_os = "windows")]
fn spawn_native_sound(path: &Path) -> Option<Child> {
    let escaped = path.to_string_lossy().replace('\'', "''");
    let script = format!("(New-Object System.Media.SoundPlayer '{escaped}').PlaySync()");
    hidden_windows_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .spawn()
        .ok()
}

#[cfg(target_os = "linux")]
fn spawn_native_sound(path: &Path) -> Option<Child> {
    Command::new("paplay")
        .arg(path)
        .spawn()
        .or_else(|_| Command::new("aplay").arg(path).spawn());
    child.ok()
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn spawn_native_sound(_path: &Path) -> Option<Child> {
    None
}

fn play_native_sound(path: PathBuf) {
    if let Some(mut child) = spawn_native_sound(&path) {
        let _ = child.wait();
    }
}

fn facility_sprite_from_bmp(kind: FacilityKind, asset: AssetRef) -> Result<Sprite, String> {
    let resources = facility_resource_ids(kind);
    facility_sprite_from_bmp_with_id(kind, resources.bitmap, asset)
}

fn facility_sprite_from_bmp_with_id(
    kind: FacilityKind,
    resource_id: u16,
    asset: AssetRef,
) -> Result<Sprite, String> {
    facility_sprite_from_bmp_with_source(kind, resource_id, 0.0, asset)
}

fn facility_sprite_from_bmp_with_source(
    kind: FacilityKind,
    resource_id: u16,
    source_x: f32,
    asset: AssetRef,
) -> Result<Sprite, String> {
    let resources = facility_resource_ids(kind);
    let bytes = asset.bytes()?;
    let image = DibImage::decode_bmp(bytes).map_err(|error| error.to_string())?;
    let alpha = AlphaMask::from_rgba(image.width(), image.height(), image.rgba())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, image.rgba());
    texture.set_filter(FilterMode::Nearest);
    Ok(Sprite {
        resource_id,
        texture,
        source: Rect::new(
            source_x,
            0.0,
            f32::from(resources.source_width),
            f32::from(resources.source_height),
        ),
        alpha,
    })
}

fn facility_sprite_array<const N: usize>(
    kind: FacilityKind,
    first_resource_id: u16,
    sources: [AssetRef; N],
) -> Result<[Sprite; N], String> {
    let sprites = sources
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| {
            facility_sprite_from_bmp_with_id(kind, first_resource_id + index as u16, bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    sprites
        .try_into()
        .map_err(|_| "facility sprite array size mismatch".to_owned())
}

fn facility_variant_index(facility: &Facility, variant_count: usize) -> usize {
    debug_assert!(variant_count > 0);
    // SplitMix64 turns the persistent facility identity and placement into a
    // well-distributed, deterministic choice. Tenants keep their look between
    // frames (and future save/load) while successive placements do not simply
    // march through each resource list in order.
    let floor_bits = u64::from(u16::from_ne_bytes(facility.position.floor.to_ne_bytes()));
    let mut value = facility.id
        ^ (u64::from(facility.position.x) << 32)
        ^ (floor_bits << 16)
        ^ (u64::from(facility_resource_ids(facility.kind).bitmap) << 48)
        ^ 0x9e37_79b9_7f4a_7c15;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) % variant_count as u64) as usize
}

fn connected_parking_spaces(facilities: &[Facility]) -> HashSet<u64> {
    let mut valid_ramps = HashSet::<(u16, i16)>::new();
    let mut ramps = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Ramp && facility.position.floor < 0)
        .collect::<Vec<_>>();
    // Validate from B1 downward. A lower ramp only works when every level
    // above it is present in the same ramp column.
    ramps.sort_by_key(|ramp| std::cmp::Reverse(ramp.position.floor));
    for ramp in &ramps {
        let valid = if ramp.position.floor == -1 {
            facilities.iter().any(|facility| {
                facility.kind == FacilityKind::Lobby
                    && facility.position.floor == 1
                    && horizontal_ranges_touch(
                        ramp.position.x,
                        ramp.end_x(),
                        facility.position.x,
                        facility.end_x(),
                    )
            })
        } else {
            valid_ramps.contains(&(ramp.position.x, ramp.position.floor + 1))
        };
        if valid {
            valid_ramps.insert((ramp.position.x, ramp.position.floor));
        }
    }

    let parking = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Parking)
        .collect::<Vec<_>>();
    let mut connected = HashSet::new();
    for ramp in ramps
        .into_iter()
        .filter(|ramp| valid_ramps.contains(&(ramp.position.x, ramp.position.floor)))
    {
        let mut frontier = vec![(ramp.position.x, ramp.end_x())];
        loop {
            let mut changed = false;
            for space in parking.iter().copied() {
                if space.position.floor != ramp.position.floor || connected.contains(&space.id) {
                    continue;
                }
                if frontier.iter().any(|&(start, end)| {
                    horizontal_ranges_touch(start, end, space.position.x, space.end_x())
                }) {
                    connected.insert(space.id);
                    frontier.push((space.position.x, space.end_x()));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    connected
}

const fn horizontal_ranges_touch(
    first_start: u16,
    first_end: u16,
    second_start: u16,
    second_end: u16,
) -> bool {
    first_start <= second_end && second_start <= first_end
}

fn occupied_parking_spaces(
    tower: &Tower,
    connected: &HashSet<u64>,
    minute_of_day: u16,
) -> HashSet<u64> {
    let commuter_demand = tower
        .facilities()
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Office && facility.is_occupied())
        .map(|facility| facility.kind.population_capacity() as usize)
        .sum::<usize>()
        .div_ceil(8);
    let overnight_demand = tower
        .facilities()
        .iter()
        .filter(|facility| {
            facility.is_occupied()
                && matches!(
                    facility.kind,
                    FacilityKind::Condo
                        | FacilityKind::HotelSingle
                        | FacilityKind::HotelTwin
                        | FacilityKind::HotelSuite
                )
        })
        .map(|facility| facility.kind.population_capacity() as usize)
        .sum::<usize>()
        .div_ceil(8);
    let mut spaces = tower
        .facilities()
        .iter()
        .filter(|facility| {
            facility.kind == FacilityKind::Parking && connected.contains(&facility.id)
        })
        .collect::<Vec<_>>();
    spaces.sort_by_key(|facility| facility.id);
    spaces
        .into_iter()
        .enumerate()
        .filter_map(|(rank, facility)| {
            parking_space_is_occupied(
                rank,
                commuter_demand,
                overnight_demand,
                facility.id,
                minute_of_day,
            )
            .then_some(facility.id)
        })
        .collect()
}

fn parking_space_is_occupied(
    rank: usize,
    commuter_demand: usize,
    overnight_demand: usize,
    facility_id: u64,
    minute_of_day: u16,
) -> bool {
    let commuter_arrival = 7 * 60 + parking_jitter(facility_id, 17, 120);
    let commuter_departure = 17 * 60 + parking_jitter(facility_id, 41, 150);
    let overnight_arrival = 16 * 60 + parking_jitter(facility_id, 73, 300);
    let overnight_departure = 6 * 60 + parking_jitter(facility_id, 101, 240);
    let commuter_present =
        rank < commuter_demand && (commuter_arrival..commuter_departure).contains(&minute_of_day);
    let overnight_present = rank < overnight_demand
        && (minute_of_day >= overnight_arrival || minute_of_day < overnight_departure);
    commuter_present || overnight_present
}

fn parking_jitter(facility_id: u64, salt: u64, span: u16) -> u16 {
    let mut value = facility_id ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) % u64::from(span)) as u16
}

fn parking_car_frame(facility: &Facility) -> usize {
    // Frame zero of bitmap 1705 is the disconnected red X; the remaining
    // thirteen frames are parked-car variants.
    1 + facility_variant_index(facility, 13)
}

fn emergency_events_unlocked(star_rating: u8) -> bool {
    star_rating >= FacilityKind::Security.unlock_stars()
}

fn apply_emergency_speed(clock: &mut Clock) -> SimulationSpeed {
    // All emergency entry points (fire, terrorist, and future incidents) use
    // this shared control so a high simulation rate cannot hide the response.
    let resume_speed = clock.speed;
    clock.speed = SimulationSpeed::Normal;
    resume_speed
}

fn restore_emergency_speed(clock: &mut Clock, resume_speed: SimulationSpeed) {
    clock.speed = resume_speed;
}

fn nearest_security_fire_response<'a>(
    target: &Facility,
    security_offices: impl Iterator<Item = &'a Facility>,
    floors: &[GridPosition],
    tower_width: u16,
) -> Option<(u64, f32)> {
    let entry_x = security_team_start_x(target, floors, tower_width);
    let action_x = (f32::from(target.end_x()) - 2.0).max(f32::from(target.position.x));
    let sweep_distance = (entry_x - action_x).max(0.0);
    security_offices
        .map(|security| {
            let floor_distance = (security.position.floor - target.position.floor).unsigned_abs();
            (
                security.id,
                8.0 + f32::from(floor_distance) * 3.0 + sweep_distance * 0.05,
            )
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn security_team_start_x(target: &Facility, floors: &[GridPosition], tower_width: u16) -> f32 {
    let (left, right) = floor_extent_for(target.position.floor, floors).unwrap_or((0, tower_width));
    f32::from(right.saturating_sub(2).max(left))
}

fn floor_extent_for(floor: i16, floors: &[GridPosition]) -> Option<(u16, u16)> {
    let left = floors
        .iter()
        .filter(|position| position.floor == floor)
        .map(|position| position.x)
        .min()?;
    let right = floors
        .iter()
        .filter(|position| position.floor == floor)
        .map(|position| position.x.saturating_add(1))
        .max()?;
    Some((left, right))
}

fn fire_response_timeout_remaining(elapsed: f32) -> u32 {
    (FIRE_RESPONSE_DECISION_SECONDS - elapsed).max(0.0).ceil() as u32
}

fn lobby_style_index(star_rating: u8) -> usize {
    match star_rating {
        0..=2 => 0,
        3 => 1,
        _ => 2,
    }
}

fn emergency_guard_source_x(arrived: bool, world_x: f32) -> f32 {
    if arrived {
        112.0
    } else if world_x.floor() as i32 & 1 == 0 {
        80.0
    } else {
        96.0
    }
}

fn fire_strip_segments(
    x: f32,
    y: f32,
    width: f32,
    tile_width: f32,
    source_x: f32,
    source_y: f32,
) -> Vec<(Rect, Rect)> {
    let mut segments = Vec::new();
    let mut offset = 0.0;
    while offset < width {
        let segment_width = (width - offset).min(tile_width);
        segments.push((
            Rect::new(x + offset, y, segment_width, FLOOR_HEIGHT),
            Rect::new(source_x, source_y, segment_width, FLOOR_HEIGHT),
        ));
        offset += segment_width;
    }
    segments
}

fn burned_unit_frame(x: f32, y: f32, width: f32, source_size: Vec2) -> (Rect, Rect) {
    debug_assert!(width <= source_size.x);
    (
        Rect::new(x, y, width, ROOM_HEIGHT),
        Rect::new(0.0, 0.0, width.min(source_size.x), source_size.y),
    )
}

fn facility_sprite_from_vertical_bmps(
    kind: FacilityKind,
    parts: &[AssetRef],
    white_is_transparent: bool,
) -> Result<Sprite, String> {
    facility_sprite_from_vertical_bmps_with_source(kind, parts, white_is_transparent, 0.0)
}

fn facility_sprite_from_vertical_bmps_with_source(
    kind: FacilityKind,
    parts: &[AssetRef],
    white_is_transparent: bool,
    source_x: f32,
) -> Result<Sprite, String> {
    let resources = facility_resource_ids(kind);
    let (texture, alpha) = texture_and_alpha_from_vertical_bmps(parts, white_is_transparent)?;
    Ok(Sprite {
        resource_id: resources.bitmap,
        texture,
        source: Rect::new(
            source_x,
            0.0,
            f32::from(resources.source_width),
            f32::from(resources.source_height),
        ),
        alpha,
    })
}

fn texture_and_alpha_from_vertical_bmps(
    parts: &[AssetRef],
    white_is_transparent: bool,
) -> Result<(Texture2D, AlphaMask), String> {
    let images = parts
        .iter()
        .map(|asset| DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(first) = images.first() else {
        return Err("vertical sprite has no source images".to_owned());
    };
    let width = first.width();
    if images.iter().any(|image| image.width() != width) {
        return Err("vertical sprite parts have different widths".to_owned());
    }
    let height = images.iter().try_fold(0_u32, |total, image| {
        total
            .checked_add(image.height())
            .ok_or_else(|| "vertical sprite height overflow".to_owned())
    })?;
    let row_bytes = usize::try_from(width)
        .map_err(|_| "vertical sprite is too wide".to_owned())?
        .checked_mul(4)
        .ok_or_else(|| "vertical sprite row overflow".to_owned())?;
    let mut rgba = Vec::with_capacity(
        row_bytes
            .checked_mul(
                usize::try_from(height).map_err(|_| "vertical sprite is too tall".to_owned())?,
            )
            .ok_or_else(|| "vertical sprite size overflow".to_owned())?,
    );
    for image in &images {
        rgba.extend_from_slice(image.rgba());
    }
    if white_is_transparent {
        for pixel in rgba.chunks_exact_mut(4) {
            if pixel[0] > 248 && pixel[1] > 248 && pixel[2] > 248 {
                pixel[3] = 0;
            }
        }
    }
    let alpha = AlphaMask::from_rgba(width, height, &rgba)?;
    let width = u16::try_from(width).map_err(|_| "vertical sprite is too wide".to_owned())?;
    let height = u16::try_from(height).map_err(|_| "vertical sprite is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok((texture, alpha))
}

fn texture_from_bmp_with_white_transparency(asset: AssetRef) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let mut rgba = image.rgba().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel[0] > 248 && pixel[1] > 248 && pixel[2] > 248 {
            pixel[3] = 0;
        }
    }
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn texture_from_bmp_with_color_transparency(
    asset: AssetRef,
    transparent_rgb: [u8; 3],
) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let mut rgba = image.rgba().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel[..3] == transparent_rgb {
            pixel[3] = 0;
        }
    }
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn texture_from_elevator_glyph_bmp(asset: AssetRef) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let mut rgba = image.rgba().to_vec();
    apply_elevator_glyph_transparency(&mut rgba);
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn apply_elevator_glyph_transparency(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        // Compound labels are overlays on the already-rendered shaft. The
        // atlas background and panel lines occupy the dark half of its gray
        // palette; the glyph ink begins at level 63.
        if pixel[0].max(pixel[1]).max(pixel[2]) < 63 {
            pixel[3] = 0;
        }
    }
}

#[cfg(test)]
fn apply_star_transparency(rgba: &mut [u8], earned: bool) {
    for pixel in rgba.chunks_exact_mut(4) {
        let is_star = if earned {
            pixel[0] != pixel[1] || pixel[1] != pixel[2]
        } else {
            pixel[..3] == [140, 140, 140]
        };
        if !is_star {
            pixel[3] = 0;
        }
    }
}

fn texture_from_queue_bmp(
    asset: AssetRef,
    replacement_color: Option<[u8; 3]>,
) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let mut rgba = image.rgba().to_vec();
    apply_queue_sprite_palette(&mut rgba, replacement_color);
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn apply_queue_sprite_palette(rgba: &mut [u8], replacement_color: Option<[u8; 3]>) {
    for pixel in rgba.chunks_exact_mut(4) {
        let maximum = pixel[0].max(pixel[1]).max(pixel[2]);
        let minimum = pixel[0].min(pixel[1]).min(pixel[2]);
        if minimum >= 220 && maximum - minimum <= 4 {
            pixel[3] = 0;
        } else if let Some(color) = replacement_color {
            pixel[..3].copy_from_slice(&color);
        }
    }
}

fn texture_from_hand_cursor(asset: AssetRef) -> Result<Texture2D, String> {
    texture_from_pointer_cursor(asset, 21, "hand")
}

fn texture_from_magnifier_cursor(asset: AssetRef) -> Result<Texture2D, String> {
    texture_from_pointer_cursor(asset, 42, "magnifier")
}

fn texture_from_pointer_cursor(
    asset: AssetRef,
    source_x: usize,
    cursor_name: &str,
) -> Result<Texture2D, String> {
    const CURSOR_WIDTH: usize = 22;
    const CURSOR_HEIGHT: usize = 21;
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    if image.width() < (source_x + CURSOR_WIDTH) as u32 || image.height() < CURSOR_HEIGHT as u32 {
        return Err(format!(
            "pointer bitmap is too small for the {cursor_name} cursor"
        ));
    }
    let source_width = image.width() as usize;
    let mut rgba = vec![0_u8; CURSOR_WIDTH * CURSOR_HEIGHT * 4];
    for y in 0..CURSOR_HEIGHT {
        for x in 0..CURSOR_WIDTH {
            let source = (y * source_width + source_x + x) * 4;
            let target = (y * CURSOR_WIDTH + x) * 4;
            rgba[target..target + 4].copy_from_slice(&image.rgba()[source..source + 4]);
            let pixel = &mut rgba[target..target + 4];
            let maximum = pixel[0].max(pixel[1]).max(pixel[2]);
            let minimum = pixel[0].min(pixel[1]).min(pixel[2]);
            if maximum - minimum <= 4 && maximum > 32 {
                pixel[3] = 0;
            }
        }
    }
    let (width, height, rgba) = scale_ui_rgba(CURSOR_WIDTH, CURSOR_HEIGHT, &rgba);
    let width = u16::try_from(width).map_err(|_| "cursor is too wide".to_owned())?;
    let height = u16::try_from(height).map_err(|_| "cursor is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Linear);
    Ok(texture)
}

fn scale2x_rgba(width: usize, height: usize, rgba: &[u8]) -> (usize, usize, Vec<u8>) {
    debug_assert_eq!(rgba.len(), width * height * 4);
    let output_width = width * 2;
    let output_height = height * 2;
    let mut output = vec![0_u8; output_width * output_height * 4];
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let offset = (y * width + x) * 4;
        rgba[offset..offset + 4].try_into().expect("RGBA pixel")
    };
    for y in 0..height {
        for x in 0..width {
            let center = pixel(x, y);
            let above = pixel(x, y.saturating_sub(1));
            let below = pixel(x, (y + 1).min(height - 1));
            let left = pixel(x.saturating_sub(1), y);
            let right = pixel((x + 1).min(width - 1), y);
            let scaled = if above != below && left != right {
                [
                    if left == above { left } else { center },
                    if above == right { right } else { center },
                    if left == below { left } else { center },
                    if below == right { right } else { center },
                ]
            } else {
                [center; 4]
            };
            for (index, color) in scaled.into_iter().enumerate() {
                let target_x = x * 2 + index % 2;
                let target_y = y * 2 + index / 2;
                let target = (target_y * output_width + target_x) * 4;
                output[target..target + 4].copy_from_slice(&color);
            }
        }
    }
    (output_width, output_height, output)
}

fn scale_ui_rgba(width: usize, height: usize, rgba: &[u8]) -> (usize, usize, Vec<u8>) {
    let (width, height, rgba) = scale2x_rgba(width, height, rgba);
    scale2x_rgba(width, height, &rgba)
}

fn texture_from_bmp(asset: AssetRef) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, image.rgba());
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn texture_from_bmp_for_ui(asset: AssetRef) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let (width, height, rgba) = scale_ui_rgba(
        image.width() as usize,
        image.height() as usize,
        image.rgba(),
    );
    let width = u16::try_from(width).map_err(|_| "UI bitmap is too wide".to_owned())?;
    let height = u16::try_from(height).map_err(|_| "UI bitmap is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    // UI bitmaps pass through the independent 1x/1.25x/1.5x/2x camera. Linear
    // sampling prevents fractional-scale cells from alternating between thick
    // and thin source pixels; tower/world textures remain nearest-neighbor.
    texture.set_filter(FilterMode::Linear);
    Ok(texture)
}

#[cfg(target_os = "macos")]
fn maximize_window() {
    use objc_rs::runtime::Object;
    use objc_rs::{Encode, Encoding, class, msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsPoint {
        x: f64,
        y: f64,
    }

    unsafe impl Encode for NsPoint {
        fn encode() -> Encoding {
            let encoding = format!(
                "{{CGPoint={}{}}}",
                f64::encode().as_str(),
                f64::encode().as_str()
            );
            unsafe { Encoding::from_str(&encoding) }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsSize {
        width: f64,
        height: f64,
    }

    unsafe impl Encode for NsSize {
        fn encode() -> Encoding {
            let encoding = format!(
                "{{CGSize={}{}}}",
                f64::encode().as_str(),
                f64::encode().as_str()
            );
            unsafe { Encoding::from_str(&encoding) }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsRect {
        origin: NsPoint,
        size: NsSize,
    }

    unsafe impl Encode for NsRect {
        fn encode() -> Encoding {
            let encoding = format!(
                "{{CGRect={}{}}}",
                NsPoint::encode().as_str(),
                NsSize::encode().as_str()
            );
            unsafe { Encoding::from_str(&encoding) }
        }
    }

    // Resize to the usable screen bounds without animation. AppKit's `zoom:`
    // action can synchronously trigger another draw while Macroquad is already
    // drawing, which aborts a normally launched .app bundle.
    unsafe {
        let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let window: *mut Object = msg_send![application, mainWindow];
        if !window.is_null() {
            let screen: *mut Object = msg_send![window, screen];
            if !screen.is_null() {
                let visible_frame: NsRect = msg_send![screen, visibleFrame];
                let _: () = msg_send![window, setFrame:visible_frame display:false animate:false];
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn maximize_window() {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetActiveWindow() -> *mut c_void;
        fn ShowWindow(window: *mut c_void, command: i32) -> i32;
    }

    const SW_MAXIMIZE: i32 = 3;
    unsafe {
        let window = GetActiveWindow();
        if !window.is_null() {
            let _ = ShowWindow(window, SW_MAXIMIZE);
        }
    }
}

#[cfg(target_os = "linux")]
fn maximize_window() {
    if Command::new("wmctrl")
        .args([
            "-r",
            "OpenTower - SimTower compatibility project",
            "-b",
            "add,maximized_vert,maximized_horz",
        ])
        .spawn()
        .is_err()
    {
        request_new_screen_size(1920.0, 1080.0);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn maximize_window() {
    request_new_screen_size(1920.0, 1080.0);
}

#[macroquad::main(window_conf)]
async fn main() {
    initialize_resources().expect(
        "OpenTower could not load the original SimTower resources; select a valid SimTower.exe",
    );
    let assets = OriginalAssets::load().expect("the original SimTower resources should decode");
    let mut app = App::new(assets);
    set_fullscreen(false);
    app.draw();
    next_frame().await;
    maximize_window();
    loop {
        app.update();
        app.draw();
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_implemented_facility_is_reachable_from_a_submenu() {
        for expected in FacilityKind::ALL {
            let found = [
                BuildMenu::Structure,
                BuildMenu::Tenants,
                BuildMenu::FoodRetail,
                BuildMenu::Transport,
                BuildMenu::Services,
                BuildMenu::Entertainment,
            ]
            .iter()
            .flat_map(|menu| menu.entries())
            .any(|entry| entry.kind == Some(expected));
            assert!(found, "missing submenu entry for {expected:?}");
        }
    }

    #[test]
    fn structural_floor_is_an_enabled_structure_tool() {
        let floor = BuildMenu::Structure
            .entries()
            .into_iter()
            .find(|entry| entry.label == "Floor")
            .expect("the structure menu should expose floor construction");
        assert!(floor.builds_floor);
        assert!(floor.is_available(1));
        assert!(floor.disabled_reason.is_none());
    }

    #[test]
    fn facility_unlocks_match_the_original_star_table() {
        for kind in [
            FacilityKind::Lobby,
            FacilityKind::Office,
            FacilityKind::Condo,
            FacilityKind::FastFood,
            FacilityKind::Stairs,
            FacilityKind::Elevator,
        ] {
            assert_eq!(kind.unlock_stars(), 1, "{kind:?}");
        }
        for kind in [
            FacilityKind::HotelSingle,
            FacilityKind::Security,
            FacilityKind::ServiceElevator,
            FacilityKind::Housekeeping,
        ] {
            assert_eq!(kind.unlock_stars(), 2, "{kind:?}");
        }
        for kind in [
            FacilityKind::HotelTwin,
            FacilityKind::HotelSuite,
            FacilityKind::Restaurant,
            FacilityKind::Shop,
            FacilityKind::Cinema,
            FacilityKind::PartyHall,
            FacilityKind::Parking,
            FacilityKind::Medical,
            FacilityKind::Recycling,
            FacilityKind::Escalator,
            FacilityKind::ExpressElevator,
            FacilityKind::Ramp,
        ] {
            assert_eq!(kind.unlock_stars(), 3, "{kind:?}");
        }
        assert_eq!(FacilityKind::Metro.unlock_stars(), 4);
        assert_eq!(FacilityKind::Cathedral.unlock_stars(), 5);
    }

    #[test]
    fn promotion_dialog_uses_the_original_rating_language() {
        assert_eq!(promotion_rating_label(2), "Two Star Rating!");
        assert_eq!(promotion_rating_label(3), "Three Star Rating!");
        assert_eq!(promotion_rating_label(4), "Four Star Rating!");
        assert_eq!(promotion_rating_label(5), "Five Star Rating!");
        assert_eq!(promotion_rating_label(6), "TOWER Rating!");
    }

    #[test]
    fn bomb_search_expands_outward_from_the_security_floor() {
        let mut tower = Tower::new(8, 1_000_000);
        for floor in [1, 2, 3, 4, 5, 6] {
            tower.place_floor(GridPosition { x: 0, floor }).unwrap();
        }
        assert_eq!(bomb_search_floor_order(&tower, 4), vec![4, 5, 3, 6, 2, 1]);
        assert_eq!(BOMB_GUARDS_PER_SECURITY_OFFICE, 6);
        assert_eq!(bomb_search_completed_floor_count(0.0, 3, 6), 0);
        assert_eq!(bomb_search_completed_floor_count(2.49, 3, 6), 0);
        assert_eq!(bomb_search_completed_floor_count(2.5, 3, 6), 3);
        assert_eq!(bomb_search_completed_floor_count(5.0, 3, 6), 6);
        assert_eq!(bomb_search_completed_floor_count(7.5, 3, 6), 6);
    }

    #[test]
    fn bomb_search_distributes_guards_from_every_security_office() {
        assert_eq!(
            bomb_search_floor_order_for_origins((1..=8).collect(), vec![2, 7]),
            vec![2, 7, 3, 8, 1, 6, 4, 5]
        );
    }

    #[test]
    fn bomb_dialog_uses_the_same_automatic_dispatch_timeout_as_fire() {
        assert_eq!(fire_response_timeout_remaining(0.0), 15);
        assert_eq!(fire_response_timeout_remaining(14.1), 1);
        assert_eq!(fire_response_timeout_remaining(15.0), 0);
    }

    #[test]
    fn overview_map_floor_scale_covers_the_complete_supported_tower_height() {
        let count =
            simtower_core::floor_ordinal(MAX_FLOOR) - simtower_core::floor_ordinal(MIN_FLOOR) + 1;
        assert_eq!(count, 109);
    }

    #[test]
    fn emergencies_remain_disabled_until_security_is_unlocked() {
        let security_stars = FacilityKind::Security.unlock_stars();
        assert!(!emergency_events_unlocked(security_stars - 1));
        assert!(emergency_events_unlocked(security_stars));
        assert!(emergency_events_unlocked(5));
    }

    #[test]
    fn emergency_speed_round_trip_restores_the_original_rate() {
        for starting_speed in [
            SimulationSpeed::Paused,
            SimulationSpeed::Fast,
            SimulationSpeed::Triple,
            SimulationSpeed::Quintuple,
            SimulationSpeed::Tenfold,
        ] {
            let mut clock = Clock::default();
            clock.speed = starting_speed;
            let resume_speed = apply_emergency_speed(&mut clock);
            assert_eq!(clock.speed, SimulationSpeed::Normal);
            assert_eq!(resume_speed, starting_speed);
            restore_emergency_speed(&mut clock, resume_speed);
            assert_eq!(clock.speed, starting_speed);
        }
    }

    #[test]
    fn nearby_security_saves_a_unit_while_a_distant_team_arrives_too_late() {
        let target = Facility {
            id: 1,
            kind: FacilityKind::Shop,
            position: GridPosition { x: 40, floor: 8 },
            occupancy: simtower_core::FacilityOccupancy::Occupied,
            price_level: 1,
            stories: 1,
        };
        let nearby = Facility {
            id: 2,
            kind: FacilityKind::Security,
            position: GridPosition { x: 44, floor: 7 },
            occupancy: simtower_core::FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories: 1,
        };
        let distant = Facility {
            position: GridPosition { x: 180, floor: 1 },
            ..nearby.clone()
        };
        let floors = (0..96)
            .map(|x| GridPosition { x, floor: 8 })
            .collect::<Vec<_>>();
        let (_, nearby_seconds) =
            nearest_security_fire_response(&target, std::iter::once(&nearby), &floors, 256)
                .unwrap();
        let (_, distant_seconds) =
            nearest_security_fire_response(&target, std::iter::once(&distant), &floors, 256)
                .unwrap();
        assert!(nearby_seconds <= FIRE_QUICK_RESPONSE_SECONDS);
        assert!(distant_seconds > FIRE_QUICK_RESPONSE_SECONDS);
    }

    #[test]
    fn fire_dialog_counts_down_before_automatic_security_dispatch() {
        assert_eq!(fire_response_timeout_remaining(0.0), 15);
        assert_eq!(fire_response_timeout_remaining(0.1), 15);
        assert_eq!(fire_response_timeout_remaining(14.1), 1);
        assert_eq!(fire_response_timeout_remaining(15.0), 0);
        assert_eq!(fire_response_timeout_remaining(20.0), 0);
    }

    #[test]
    fn security_team_uses_original_emergency_guard_cels() {
        assert_eq!(emergency_guard_source_x(false, 20.0), 80.0);
        assert_eq!(emergency_guard_source_x(false, 21.0), 96.0);
        assert_eq!(emergency_guard_source_x(true, 20.0), 112.0);
    }

    #[test]
    fn lobby_style_tracks_the_original_rating_tiers() {
        assert_eq!(lobby_style_index(1), 0);
        assert_eq!(lobby_style_index(2), 0);
        assert_eq!(lobby_style_index(3), 1);
        assert_eq!(lobby_style_index(4), 2);
        assert_eq!(lobby_style_index(5), 2);
    }

    #[test]
    fn sky_lobbies_use_the_dedicated_windowed_chunk() {
        assert_lobby_artwork_source(0, 1, 15, (0, LOBBY_CHUNK_WIDTH));
        assert_lobby_artwork_source(0, 1, 30, (0, LOBBY_CHUNK_WIDTH));
    }

    #[test]
    fn ground_and_grand_lobbies_use_their_original_rows() {
        assert_lobby_artwork_source(0, 1, 1, (0, 0.0));

        assert_lobby_artwork_source(0, 2, 1, (0, LOBBY_CHUNK_WIDTH * 2.0));
        assert_lobby_artwork_source(1, 2, 1, (2, 0.0));

        assert_lobby_artwork_source(0, 3, 1, (0, LOBBY_CHUNK_WIDTH * 2.0));
        assert_lobby_artwork_source(1, 3, 1, (1, LOBBY_CHUNK_WIDTH));
        assert_lobby_artwork_source(2, 3, 1, (2, 0.0));
    }

    #[test]
    fn original_lobby_modifiers_select_two_and_three_story_artwork() {
        assert_eq!(lobby_stories_for_modifiers(false, false), 1);
        assert_eq!(lobby_stories_for_modifiers(true, false), 2);
        assert_eq!(lobby_stories_for_modifiers(true, true), 3);
        assert_eq!(lobby_stories_for_modifiers(false, true), 1);

        let mut tower = Tower::new(16, 1_000_000);
        for floor in 1..=3 {
            for x in 0..4 {
                tower.place_floor(GridPosition { x, floor }).unwrap();
            }
        }
        assert_eq!(
            lobby_stories_for_position_with_request(&tower, GridPosition { x: 0, floor: 1 }, 3),
            3
        );
        tower
            .place_lobby(GridPosition { x: 0, floor: 1 }, 3)
            .unwrap();
        assert_eq!(
            lobby_stories_for_position_with_request(&tower, GridPosition { x: 1, floor: 1 }, 3),
            3
        );
        assert_eq!(
            lobby_stories_for_position_with_request(&tower, GridPosition { x: 3, floor: 1 }, 3),
            1
        );
    }

    #[test]
    fn security_team_enters_from_the_original_right_emergency_stair() {
        let target = Facility {
            id: 1,
            kind: FacilityKind::Shop,
            position: GridPosition { x: 10, floor: 6 },
            occupancy: simtower_core::FacilityOccupancy::Occupied,
            price_level: 1,
            stories: 1,
        };
        let floors = (4..80)
            .map(|x| GridPosition { x, floor: 6 })
            .collect::<Vec<_>>();
        assert_eq!(security_team_start_x(&target, &floors, 256), 78.0);

        // Office x does not change the entry side; guards use the exterior stair.
        assert_eq!(security_team_start_x(&target, &floors, 256), 78.0);
    }

    #[test]
    fn fire_strip_starts_at_the_unit_and_covers_its_exact_width() {
        let segments = fire_strip_segments(120.0, 244.0, 192.0, 96.0, 0.0, 0.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].0, Rect::new(120.0, 244.0, 96.0, FLOOR_HEIGHT));
        assert_eq!(segments[1].0, Rect::new(216.0, 244.0, 96.0, FLOOR_HEIGHT));
        assert_eq!(
            segments.last().unwrap().0.x + segments.last().unwrap().0.w,
            312.0
        );

        let cropped = fire_strip_segments(80.0, 100.0, 72.0, 96.0, 192.0, 0.0);
        assert_eq!(
            cropped,
            vec![(
                Rect::new(80.0, 100.0, 72.0, FLOOR_HEIGHT),
                Rect::new(192.0, 0.0, 72.0, FLOOR_HEIGHT),
            )]
        );
    }

    #[test]
    fn burned_tenants_crop_the_original_wide_ruin_strip_to_their_footprint() {
        let artwork = DibImage::decode_bmp(FIRE_AFTERMATH_BMP.bytes().unwrap()).unwrap();
        assert_eq!((artwork.width(), artwork.height()), (248, 24));

        let single_room = burned_unit_frame(120.0, 244.0, 32.0, vec2(248.0, 24.0));
        assert_eq!(
            single_room,
            (
                Rect::new(120.0, 244.0, 32.0, ROOM_HEIGHT),
                Rect::new(0.0, 0.0, 32.0, 24.0),
            )
        );

        let cinema = burned_unit_frame(40.0, 100.0, 248.0, vec2(248.0, 24.0));
        assert_eq!(
            cinema,
            (
                Rect::new(40.0, 100.0, 248.0, ROOM_HEIGHT),
                Rect::new(0.0, 0.0, 248.0, 24.0),
            )
        );
    }

    #[test]
    fn top_level_toolbox_matches_the_original_three_by_three_layout() {
        assert_eq!(PALETTE_COLUMNS, 3);
        assert_eq!(PALETTE_BUTTONS.len(), 9);
        assert_eq!(
            PALETTE_BUTTONS.map(|button| button.label),
            [
                "Speed",
                "Inspect",
                "Bulldoze",
                "Structure",
                "Tenants",
                "Food & retail",
                "Transport",
                "Services",
                "Entertainment",
            ]
        );
        assert_eq!(BuildMenu::Structure.unlock_stars(), 1);
        assert_eq!(BuildMenu::Tenants.unlock_stars(), 1);
        assert_eq!(BuildMenu::FoodRetail.unlock_stars(), 1);
        assert_eq!(BuildMenu::Transport.unlock_stars(), 1);
        assert_eq!(BuildMenu::Services.unlock_stars(), 2);
        assert_eq!(BuildMenu::Entertainment.unlock_stars(), 3);
    }

    #[test]
    fn speed_menu_exposes_pause_and_five_running_rates() {
        assert_eq!(
            SPEED_CHOICES.map(speed_label),
            ["1x", "2x", "3x", "5x", "10x", "Pause"]
        );
        for (index, speed) in SPEED_CHOICES.iter().copied().enumerate() {
            assert_eq!(
                speed_menu_choice_at(speed_menu_entry_rect(index).center()),
                Some(speed)
            );
        }
    }

    #[test]
    fn keyboard_number_row_selects_speed_tiers_in_order() {
        assert_eq!(
            speed_keyboard_shortcut(KeyCode::GraveAccent),
            Some(SimulationSpeed::Paused)
        );
        for (key, expected) in [
            (KeyCode::Key1, SimulationSpeed::Normal),
            (KeyCode::Key2, SimulationSpeed::Fast),
            (KeyCode::Key3, SimulationSpeed::Triple),
            (KeyCode::Key4, SimulationSpeed::Quintuple),
            (KeyCode::Key5, SimulationSpeed::Tenfold),
            (KeyCode::Kp1, SimulationSpeed::Normal),
            (KeyCode::Kp5, SimulationSpeed::Tenfold),
        ] {
            assert_eq!(speed_keyboard_shortcut(key), Some(expected));
        }
        assert_eq!(speed_keyboard_shortcut(KeyCode::Key6), None);
    }

    #[test]
    fn elevator_shaft_labels_match_original_floor_designations() {
        assert_eq!(format_floor(1), "1");
        assert_eq!(format_floor(15), "15");
        assert_eq!(format_floor(-1), "B1");
        assert_eq!(format_floor(-8), "B8");
    }

    #[test]
    fn individual_people_receive_original_style_role_labels() {
        assert_eq!(
            person_role_for_kinds(None, Some(FacilityKind::Office)),
            "Salesman"
        );
        assert_eq!(
            person_role_for_kinds(None, Some(FacilityKind::HotelSingle)),
            "Hotel Guest"
        );
        assert_eq!(
            person_role_for_kinds(
                Some(FacilityKind::Housekeeping),
                Some(FacilityKind::HotelSuite)
            ),
            "Housekeeper"
        );
        assert_eq!(
            person_role_for_kinds(Some(FacilityKind::Security), Some(FacilityKind::Office)),
            "Security Guard"
        );
    }

    #[test]
    fn individual_person_stress_respects_mood_and_wait_time() {
        assert_eq!(person_stress_for(PersonMood::Calm, 0.0), 0.0);
        assert!(person_stress_for(PersonMood::Concerned, 0.0) >= 0.5);
        assert!(person_stress_for(PersonMood::Angry, 0.0) >= 0.82);
        assert!(person_stress_for(PersonMood::Angry, 30.0) > 0.99);
    }

    #[test]
    fn person_place_labels_include_the_tenant_floor() {
        let mut tower = Tower::new(32, 2_000_000);
        for floor in 1..=7 {
            for x in 0..9 {
                tower.place_floor(GridPosition { x, floor }).unwrap();
            }
        }
        let office = tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 7 })
            .unwrap();
        assert_eq!(person_place_label(0, &tower), "Outside");
        assert_eq!(person_place_label(office, &tower), "Office, Floor 7");
    }

    #[test]
    fn hidden_elevator_display_only_fades_the_shaft_interior() {
        assert_eq!(elevator_display_alpha(true), 1.0);
        assert_eq!(elevator_display_alpha(false), 0.22);
        assert_eq!(elevator_infrastructure_alpha(true), 1.0);
        assert_eq!(elevator_infrastructure_alpha(false), 1.0);
        assert_eq!(hidden_elevator_outline_color(), BLACK);
    }

    #[test]
    fn elevator_default_floors_use_the_original_pink_number_atlases() {
        assert_eq!(elevator_number_texture_indices(false), (0, 1, 2));
        assert_eq!(elevator_number_texture_indices(true), (3, 4, 5));
    }

    #[test]
    fn compound_elevator_labels_only_keep_glyph_ink() {
        let mut rgba = vec![
            25, 25, 25, 255, // shaft background
            38, 38, 38, 255, // shaft bevel
            63, 63, 63, 255, // normal label ink
            190, 90, 80, 255, // selected label ink
        ];
        apply_elevator_glyph_transparency(&mut rgba);
        assert_eq!(rgba[3], 0);
        assert_eq!(rgba[7], 0);
        assert_eq!(rgba[11], 255);
        assert_eq!(rgba[15], 255);
    }

    #[test]
    fn compound_elevator_labels_cover_the_shaft_ribs_with_a_centered_panel() {
        assert_eq!(
            elevator_compound_label_backplate(100.0, 72.0, 64.0),
            Rect::new(116.0, 72.0, 32.0, 36.0)
        );
        assert_eq!(
            elevator_compound_label_backplate(100.0, 72.0, 32.0),
            Rect::new(100.0, 72.0, 32.0, 36.0)
        );
    }

    #[test]
    fn mode_dropdown_exposes_every_map_mode() {
        assert_eq!(MODE_MENU_LABELS, ["Edit", "Evaluation", "Rent", "Hotel"]);
        for (index, expected) in [
            ViewMode::Edit,
            ViewMode::Evaluation,
            ViewMode::Pricing,
            ViewMode::Hotel,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(expected.menu_index(), index);
            assert_eq!(
                mode_menu_choice_at(mode_menu_entry_rect(index).center()),
                Some(expected)
            );
        }
    }

    #[test]
    fn pricing_steps_both_directions_and_control_targets_one_tenant_type() {
        assert_eq!(stepped_price_level(1, 1), 2);
        assert_eq!(stepped_price_level(1, -1), 0);
        assert_eq!(stepped_price_level(3, 1), 0);
        assert_eq!(stepped_price_level(0, -1), 3);

        let mut tower = Tower::new(48, 10_000_000);
        for x in 0..48 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        let first = tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 1 })
            .unwrap();
        let second = tower
            .place(FacilityKind::Office, GridPosition { x: 10, floor: 1 })
            .unwrap();
        let shop = tower
            .place(FacilityKind::Shop, GridPosition { x: 24, floor: 1 })
            .unwrap();
        assert_eq!(
            set_price_for_tenant_kind(&mut tower, FacilityKind::Office, 3),
            2
        );
        assert_eq!(
            tower
                .facilities()
                .iter()
                .find(|facility| facility.id == first)
                .unwrap()
                .price_level,
            3
        );
        assert_eq!(
            tower
                .facilities()
                .iter()
                .find(|facility| facility.id == second)
                .unwrap()
                .price_level,
            3
        );
        assert_eq!(
            tower
                .facilities()
                .iter()
                .find(|facility| facility.id == shop)
                .unwrap()
                .price_level,
            1
        );
    }

    #[test]
    fn top_level_sound_toggle_is_a_compact_icon_and_stars_are_centered() {
        assert!(sound_menu_button_rect().x > mode_menu_button_rect().x);
        assert_eq!(sound_menu_button_rect().w, 32.0);
        let speaker = speaker_icon_geometry(sound_menu_button_rect());
        assert_eq!(speaker.body.x.fract(), 0.0);
        assert_eq!(speaker.body.y.fract(), 0.0);
        assert_eq!(speaker.cone[0].y, speaker.body.y + speaker.body.h * 0.5);
        assert!(speaker.body.x + speaker.body.w - speaker.cone[0].x >= 4.0);
        let row = rating_row_rect();
        let first = rating_star_rect(0);
        let last = rating_star_rect(4);
        assert_eq!(first.w, 22.0);
        assert_eq!(first.h, 18.0);
        assert!(((first.x - row.x) - (row.x + row.w - last.x - last.w)).abs() < f32::EPSILON);
        let points = star_points(first);
        assert!((points[0].x - first.center().x).abs() < f32::EPSILON);
        assert!((points[0].y - first.y).abs() < f32::EPSILON);
        assert!(points.iter().all(|point| {
            point.x >= first.x - f32::EPSILON
                && point.x <= first.x + first.w + f32::EPSILON
                && point.y >= first.y - f32::EPSILON
                && point.y <= first.y + first.h + f32::EPSILON
        }));
        let sounds = NativeSounds {
            opening_theme: PathBuf::new(),
            opening_theme_process: RefCell::new(None),
            construction: PathBuf::new(),
            lobby_segment: PathBuf::new(),
            demolition: PathBuf::new(),
            no_money: PathBuf::new(),
            payment: PathBuf::new(),
            fire_alert: PathBuf::new(),
            fire_response: PathBuf::new(),
            bomb_threat: PathBuf::new(),
            bomb_armed: PathBuf::new(),
            bomb_explosion: PathBuf::new(),
            elevator_move: PathBuf::new(),
            elevator_open: [PathBuf::new(), PathBuf::new()],
            escalator_move: PathBuf::new(),
            crowd: PathBuf::new(),
            parking_arrival: PathBuf::new(),
            parking_departure: PathBuf::new(),
            muted: Cell::new(false),
        };
        assert!(!sounds.is_muted());
        assert!(sounds.toggle_muted());
        assert!(sounds.is_muted());
        assert!(!sounds.toggle_muted());
        assert!(!sounds.is_muted());
        sounds.set_muted(true);
        assert!(sounds.is_muted());
        sounds.set_muted(false);
        assert!(!sounds.is_muted());
    }

    #[cfg(feature = "bundled-resources")]
    #[test]
    fn desktop_icon_is_rendered_independently_at_each_system_size() {
        let small = render_opentower_icon::<{ 16 * 16 * 4 }>(16);
        let medium = render_opentower_icon::<{ 32 * 32 * 4 }>(32);
        let large = render_opentower_icon::<{ 64 * 64 * 4 }>(64);
        assert_eq!(small.len(), 16 * 16 * 4);
        assert_eq!(medium.len(), 32 * 32 * 4);
        assert_eq!(large.len(), 64 * 64 * 4);
        assert!(
            large
                .chunks_exact(4)
                .any(|pixel| pixel[..3] == [255, 238, 0])
        );
        assert!(
            large
                .chunks_exact(4)
                .any(|pixel| pixel[..3] == [238, 34, 24])
        );
        assert!(
            large.chunks_exact(4).any(|pixel| {
                pixel[0] > 12 && pixel[0] < 248 && pixel[1] > 12 && pixel[1] < 248
            })
        );
    }

    #[test]
    fn edge_aware_ui_upscale_preserves_pixel_art_corners() {
        let white = [255, 255, 255, 255];
        let black = [0, 0, 0, 255];
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        let mut input = vec![white; 9];
        input[1] = black;
        input[3] = black;
        input[5] = blue;
        input[7] = red;
        let input = input.into_iter().flatten().collect::<Vec<_>>();
        let (width, height, output) = scale2x_rgba(3, 3, &input);
        assert_eq!((width, height), (6, 6));
        let center_top_left = (2 * width + 2) * 4;
        assert_eq!(&output[center_top_left..center_top_left + 4], &black);
        let center_top_right = (2 * width + 3) * 4;
        assert_eq!(&output[center_top_right..center_top_right + 4], &white);
    }

    #[test]
    fn simulated_days_include_a_seven_day_week() {
        assert_eq!(weekday_name(1), "Monday");
        assert_eq!(weekday_name(5), "Friday");
        assert_eq!(weekday_name(7), "Sunday");
        assert_eq!(weekday_name(8), "Monday");
    }

    #[test]
    fn quarterly_reports_close_at_five_on_the_first_day_of_the_next_quarter() {
        let day_four = 3 * 24 * 60;
        assert_eq!(
            quarter_closed_between(day_four + 299, day_four + 300),
            Some((1, 1))
        );
        assert_eq!(quarter_closed_between(day_four + 300, day_four + 301), None);

        let year_two = 15 * 24 * 60;
        assert_eq!(
            quarter_closed_between(year_two + 299, year_two + 300),
            Some((2, 1))
        );
    }

    #[test]
    fn finance_ledger_separates_operations_construction_and_other_income() {
        let mut ledger = FinanceLedger::new(2_000_000);
        ledger.reconcile_external_cash(1_960_000);
        ledger.record(&[
            simtower_core::IncomeEvent {
                facility_id: 1,
                kind: FacilityKind::Office,
                amount: 10_000,
            },
            simtower_core::IncomeEvent {
                facility_id: 2,
                kind: FacilityKind::Elevator,
                amount: -20_000,
            },
        ]);
        ledger.last_observed_cash = 1_950_000;
        ledger.reconcile_external_cash(2_150_000);
        ledger.close_quarter(1, 1, 2_150_000, HashMap::new());

        let report = ledger.last_report.as_ref().unwrap();
        assert_eq!(report.total_income, 10_000);
        assert_eq!(report.total_maintenance, 20_000);
        assert_eq!(report.construction_costs, 40_000);
        assert_eq!(report.other_income, 200_000);
        assert_eq!(report.income_by_kind[&FacilityKind::Office], 10_000);
        assert_eq!(report.maintenance_by_kind[&FacilityKind::Elevator], 20_000);
        assert_eq!(ledger.period_start_balance, 2_150_000);
        assert_eq!(ledger.tenant_income, 0);
    }

    #[test]
    fn tenant_information_uses_original_business_names_and_evaluations() {
        assert_eq!(
            default_tenant_name(FacilityKind::Restaurant, Some(0)),
            "English Pub"
        );
        assert_eq!(
            default_tenant_name(FacilityKind::FastFood, Some(1)),
            "Chinese Cafe"
        );
        assert_eq!(
            default_tenant_name(FacilityKind::Shop, Some(10)),
            "Sports Gear"
        );

        let facility = Facility {
            id: 7,
            kind: FacilityKind::FastFood,
            position: GridPosition { x: 10, floor: 2 },
            occupancy: simtower_core::FacilityOccupancy::Occupied,
            price_level: 1,
            stories: 1,
        };
        assert_eq!(
            tenant_evaluation(&facility, false, 48, 12),
            "No transportation connected"
        );
        assert_eq!(
            tenant_evaluation(&facility, true, 48, 12),
            "Business is very good!"
        );
        assert_eq!(
            tenant_evaluation(&facility, true, 0, 0),
            "Very few customers"
        );
    }

    #[test]
    fn parking_connectivity_follows_ramps_and_adjacent_spaces() {
        let facility = |id, kind, x, floor| Facility {
            id,
            kind,
            position: GridPosition { x, floor },
            occupancy: simtower_core::FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories: 1,
        };
        let facilities = vec![
            facility(1, FacilityKind::Lobby, 0, 1),
            facility(2, FacilityKind::Ramp, 0, -1),
            facility(3, FacilityKind::Parking, 16, -1),
            facility(4, FacilityKind::Parking, 20, -1),
            facility(5, FacilityKind::Parking, 28, -1),
            facility(6, FacilityKind::Ramp, 0, -2),
            facility(7, FacilityKind::Parking, 16, -2),
            facility(8, FacilityKind::Ramp, 40, -2),
            facility(9, FacilityKind::Parking, 56, -2),
        ];

        let connected = connected_parking_spaces(&facilities);
        assert_eq!(connected, HashSet::from([3, 4, 7]));
    }

    #[test]
    fn parking_schedule_staggers_commuters_and_overnight_cars() {
        // Every commuter car is present at noon and gone before the morning
        // arrival window; overnight cars use the inverse schedule.
        assert!(parking_space_is_occupied(0, 1, 0, 42, 12 * 60));
        assert!(!parking_space_is_occupied(0, 1, 0, 42, 5 * 60));
        assert!(parking_space_is_occupied(0, 0, 1, 42, 2 * 60));
        assert!(!parking_space_is_occupied(0, 0, 1, 42, 12 * 60));
        assert!(!parking_space_is_occupied(1, 1, 1, 42, 12 * 60));
    }

    #[test]
    fn parked_car_variants_never_select_the_disconnected_x() {
        let mut parking = Facility {
            id: 1,
            kind: FacilityKind::Parking,
            position: GridPosition { x: 0, floor: -1 },
            occupancy: simtower_core::FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories: 1,
        };
        for id in 1..=128 {
            parking.id = id;
            assert!((1..=13).contains(&parking_car_frame(&parking)));
        }
    }

    #[test]
    fn residential_tenant_panels_show_literal_original_headcounts() {
        let occupied = |kind| Facility {
            id: kind as u64 + 1,
            kind,
            position: GridPosition { x: 10, floor: 2 },
            occupancy: simtower_core::FacilityOccupancy::Occupied,
            price_level: 1,
            stories: 1,
        };

        let single = tenant_info_details(&occupied(FacilityKind::HotelSingle), 0, 1, 0);
        assert_eq!(single.fields[0].1, "1 / 1");
        assert_eq!(single.people_icons, 1);

        let twin = tenant_info_details(&occupied(FacilityKind::HotelTwin), 0, 2, 0);
        assert_eq!(twin.fields[0].1, "2 / 2");
        assert_eq!(twin.people_icons, 2);

        let suite = tenant_info_details(&occupied(FacilityKind::HotelSuite), 0, 1, 0);
        assert_eq!(suite.fields[0].1, "1 / 2");
        assert_eq!(suite.people_icons, 1);

        let condo = tenant_info_details(&occupied(FacilityKind::Condo), 0, 0, 0);
        assert_eq!(condo.people_icons, 3);

        assert_eq!(FacilityKind::Office.population_capacity(), 6);
        assert_eq!(FacilityKind::Condo.population_capacity(), 3);
        assert_eq!(FacilityKind::HotelSingle.population_capacity(), 1);
        assert_eq!(FacilityKind::HotelTwin.population_capacity(), 2);
        assert_eq!(FacilityKind::HotelSuite.population_capacity(), 2);
        assert_eq!(proportional_people_icons(24, 48), 15);
    }

    #[test]
    fn tenant_people_meter_crops_the_original_black_silhouette_strip() {
        let rect = Rect::new(10.0, 20.0, 460.0, 39.0);
        assert!(tenant_people_strip_layout(rect, 0, 640.0).is_none());
        let (source, destination) = tenant_people_strip_layout(rect, 30, 640.0).unwrap();
        assert_eq!(source, Rect::new(0.0, 0.0, 480.0, 36.0));
        assert!(destination.w <= rect.w - 16.0);
        assert!((destination.w / destination.h - source.w / source.h).abs() < 0.001);
        assert!(destination.y >= rect.y);
        assert!(destination.y + destination.h <= rect.y + rect.h);
    }

    #[test]
    fn tenant_information_rolls_income_into_yesterdays_profit() {
        let mut finances = HashMap::new();
        let event = IncomeEvent {
            facility_id: 42,
            kind: FacilityKind::Restaurant,
            amount: 6_000,
        };
        record_tenant_finances(&mut finances, 3, &[event]);
        assert_eq!(finances[&42].current_profit, 6_000);
        assert_eq!(finances[&42].previous_profit, 0);

        record_tenant_finances(&mut finances, 4, &[]);
        assert_eq!(finances[&42].current_profit, 0);
        assert_eq!(finances[&42].previous_profit, 6_000);
    }

    #[test]
    fn narrow_topbar_controls_push_the_clock_to_the_right() {
        assert_eq!(topbar_clock_x(1_200.0, 160.0, 430.0), 520.0);
        assert_eq!(topbar_clock_x(800.0, 160.0, 430.0), 442.0);
    }

    #[test]
    fn game_menu_exposes_file_actions_report_toggle_and_ux_scales() {
        assert_eq!(GAME_MENU_ACTIONS.len(), 6);
        assert_eq!(
            GAME_MENU_ACTIONS.map(|(_, label)| label),
            [
                "New",
                "Save",
                "Load",
                "Automatic reports",
                "UX scale",
                "Quit",
            ]
        );
        for (index, (action, _)) in GAME_MENU_ACTIONS.iter().enumerate() {
            assert_eq!(
                game_menu_action_at(game_menu_entry_rect(index).center()),
                Some(*action)
            );
        }
        assert_eq!(game_menu_button_rect().x, 6.0);
        assert_eq!(GAME_MENU_ACTIONS[4].0, GameMenuAction::UxScale);
        for (index, scale) in UX_SCALE_CHOICES.iter().copied().enumerate() {
            assert_eq!(
                ux_scale_menu_choice_at(ux_scale_menu_entry_rect(index).center()),
                Some(scale)
            );
        }
    }

    #[test]
    fn welcome_dialog_offers_the_original_startup_actions() {
        let viewport = vec2(640.0, 480.0);
        assert_eq!(
            WELCOME_ACTIONS.map(|(_, label)| label),
            ["New Tower", "Load Saved Tower", "Quit"]
        );
        let dialog = welcome_dialog_rect(viewport);
        assert_eq!(dialog, Rect::new(190.0, 182.0, 264.0, 119.0));
        assert_eq!(
            welcome_button_rect(viewport, 0),
            Rect::new(207.0, 192.0, 230.0, 27.0)
        );
        assert_eq!(
            welcome_button_rect(viewport, 1),
            Rect::new(207.0, 228.0, 230.0, 27.0)
        );
        assert_eq!(
            welcome_button_rect(viewport, 2),
            Rect::new(281.0, 264.0, 82.0, 27.0)
        );
        for (index, (action, _)) in WELCOME_ACTIONS.iter().enumerate() {
            let button = welcome_button_rect(viewport, index);
            assert!(dialog.contains(button.center()));
            assert_eq!(welcome_action_at(viewport, button.center()), Some(*action));
        }
        assert_eq!(welcome_action_at(viewport, vec2(0.0, 0.0)), None);

        let widescreen = vec2(1_280.0, 720.0);
        assert_eq!(title_screen_transform(widescreen), (1.5, vec2(160.0, 0.0)));
        assert_eq!(
            welcome_dialog_rect(widescreen),
            Rect::new(445.0, 273.0, 396.0, 178.5)
        );
    }

    #[test]
    fn ux_scale_supports_fractional_choices_and_transforms_only_ui_coordinates() {
        assert_eq!(UxScale::One.factor(), 1.0);
        assert_eq!(UxScale::OneAndQuarter.factor(), 1.25);
        assert_eq!(UxScale::OneAndHalf.factor(), 1.5);
        assert_eq!(UxScale::Two.factor(), 2.0);
        assert_eq!(
            UxScale::from_preference("1.25"),
            Some(UxScale::OneAndQuarter)
        );
        assert_eq!(UxScale::from_preference("1.5x"), Some(UxScale::OneAndHalf));
        assert_eq!(UxScale::from_preference("3"), Some(UxScale::Two));
        assert_eq!(UxScale::from_preference("bogus"), None);
        assert_eq!(
            physical_to_ui(vec2(1_000.0, 500.0), UxScale::OneAndQuarter),
            vec2(800.0, 400.0)
        );
    }

    #[test]
    fn save_names_are_safe_and_receive_the_native_extension() {
        assert_eq!(DEFAULT_SAVE_NAME, "Tower");
        assert_eq!(
            sanitized_save_filename("My Tower").unwrap(),
            "My Tower.smtower"
        );
        assert_eq!(
            sanitized_save_filename("../../Bad:Name").unwrap(),
            "_.._Bad_Name.smtower"
        );
        assert_eq!(
            sanitized_save_filename("Existing.SMTOWER").unwrap(),
            "Existing.SMTOWER"
        );
        assert!(sanitized_save_filename("... ").is_err());
    }

    #[test]
    fn scaled_ui_text_rasterizes_at_output_density_without_changing_layout_size() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (raster_size, font_scale) = ui_text_rasterization(14.0, scale);
            assert_eq!(raster_size, (14.0 * scale).round() as u16);
            assert!((f32::from(raster_size) * font_scale - 14.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn save_dialog_starts_with_the_current_tower_name_selected() {
        let mut dialog = SaveNameDialog::with_default_name(DEFAULT_SAVE_NAME);
        assert_eq!(dialog.name, "Tower");
        assert!(dialog.replace_default_on_input);

        dialog.push_character('M');
        dialog.push_character('y');
        assert_eq!(dialog.name, "My");
        assert!(!dialog.replace_default_on_input);

        let mut dialog = SaveNameDialog::with_default_name("Loaded Tower");
        assert_eq!(dialog.name, "Loaded Tower");
        assert!(dialog.replace_default_on_input);
        dialog.backspace();
        assert!(dialog.name.is_empty());
        assert!(!dialog.replace_default_on_input);

        assert_eq!(
            save_default_name_for_path(Path::new("/tmp/My Tower.smtower")),
            "My Tower"
        );
    }

    #[test]
    fn save_name_text_repeat_waits_then_repeats_at_a_steady_rate() {
        let mut repeat = TextKeyRepeat {
            key: KeyCode::Backspace,
            action: TextEditAction::Backspace,
            elapsed: 0.0,
            next_repeat: TEXT_REPEAT_DELAY_SECONDS,
        };
        assert_eq!(repeat.advance(0.40), 0);
        assert_eq!(repeat.advance(0.03), 1);
        assert_eq!(repeat.advance(TEXT_REPEAT_INTERVAL_SECONDS * 3.0), 3);
        assert_eq!(key_for_text_character('A'), Some(KeyCode::A));
        assert_eq!(key_for_text_character('_'), Some(KeyCode::Minus));
        assert_eq!(key_for_text_character(' '), Some(KeyCode::Space));
    }

    #[test]
    fn save_game_json_round_trips_simulation_state() {
        let mut tower = Tower::new(32, 2_000_000);
        for x in 0..16 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 4, floor: 1 })
            .unwrap();
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let save = SaveGame {
            format_version: SAVE_FORMAT_VERSION,
            tower: tower.clone(),
            traffic: traffic.clone(),
            camera_x: 3.5,
            camera_floor: 2.0,
            last_running_speed: SimulationSpeed::Triple,
            next_fire_minute: 900,
            finance_ledger: FinanceLedger {
                tenant_income: 25_000,
                maintenance: 4_000,
                ..FinanceLedger::default()
            },
            options: SaveOptions {
                muted: true,
                ux_scale: UxScale::OneAndHalf,
                automatic_finance_reports: false,
            },
            legacy_automatic_finance_reports: None,
            tenant_variants: HashMap::new(),
            tenant_variant_bags: HashMap::new(),
            tenant_names: HashMap::new(),
            person_names: HashMap::from([(7, "Morgan".to_owned())]),
            tenant_finances: HashMap::new(),
            hotel_guest_stays: HashSet::new(),
            vip_visit: None,
            last_vip_attempt_day: 0,
            bomb_event: None,
        };
        let json = serde_json::to_vec(&save).expect("save should encode");
        let decoded: SaveGame = serde_json::from_slice(&json).expect("save should decode");
        assert_eq!(decoded.format_version, SAVE_FORMAT_VERSION);
        assert_eq!(decoded.tower, tower);
        assert_eq!(decoded.traffic, traffic);
        assert_eq!(decoded.last_running_speed, SimulationSpeed::Triple);
        assert_eq!(decoded.finance_ledger.tenant_income, 25_000);
        assert_eq!(
            decoded.person_names.get(&7).map(String::as_str),
            Some("Morgan")
        );
        assert_eq!(
            decoded.resolved_options(),
            SaveOptions {
                muted: true,
                ux_scale: UxScale::OneAndHalf,
                automatic_finance_reports: false,
            }
        );
        let root = serde_json::from_slice::<serde_json::Value>(&json).unwrap();
        assert!(root.get("options").is_some());
        assert!(root.get("automatic_finance_reports").is_none());
        let mut future_value = serde_json::to_value(&save).unwrap();
        let future_options = future_value
            .get_mut("options")
            .unwrap()
            .as_object_mut()
            .unwrap();
        future_options.remove("ux_scale");
        future_options.insert("future_option".to_owned(), serde_json::json!("supported"));
        let future_decoded: SaveGame = serde_json::from_value(future_value).unwrap();
        assert_eq!(future_decoded.options.ux_scale, UxScale::One);
        assert!(future_decoded.options.muted);
        decoded.tower.validate_save_state().unwrap();
        decoded.traffic.validate_save_state(&decoded.tower).unwrap();
    }

    #[test]
    fn saves_from_before_endgame_events_remain_loadable() {
        let tower = Tower::new(32, 2_000_000);
        let save = SaveGame {
            format_version: SAVE_FORMAT_VERSION,
            tower,
            traffic: TrafficSimulation::new(),
            camera_x: 0.0,
            camera_floor: 1.0,
            last_running_speed: SimulationSpeed::Normal,
            next_fire_minute: 900,
            finance_ledger: FinanceLedger::default(),
            options: SaveOptions::default(),
            legacy_automatic_finance_reports: None,
            tenant_variants: HashMap::new(),
            tenant_variant_bags: HashMap::new(),
            tenant_names: HashMap::new(),
            person_names: HashMap::new(),
            tenant_finances: HashMap::new(),
            hotel_guest_stays: HashSet::new(),
            vip_visit: None,
            last_vip_attempt_day: 0,
            bomb_event: None,
        };
        let mut value = serde_json::to_value(save).unwrap();
        let root = value.as_object_mut().unwrap();
        root.remove("options");
        root.insert(
            "automatic_finance_reports".to_owned(),
            serde_json::json!(false),
        );
        root.remove("vip_visit");
        root.remove("last_vip_attempt_day");
        root.remove("bomb_event");
        root.remove("person_names");
        let tower = root.get_mut("tower").unwrap().as_object_mut().unwrap();
        tower.remove("tower_status_awarded");
        tower.remove("cathedral_wedding_day");
        tower.remove("vip_approved");
        let decoded: SaveGame = serde_json::from_value(value).unwrap();
        assert!(!decoded.tower.tower_status_awarded());
        assert!(!decoded.tower.vip_approved());
        assert!(decoded.vip_visit.is_none());
        assert!(decoded.bomb_event.is_none());
        assert_eq!(
            decoded.resolved_options(),
            SaveOptions {
                automatic_finance_reports: false,
                ..SaveOptions::default()
            }
        );
    }

    #[test]
    fn currency_compacts_seven_figure_values_and_keeps_the_sign_before_the_dollar() {
        assert_eq!(format_number(2_000_000), "2,000,000");
        assert_eq!(format_currency(999_999), "$999,999");
        assert_eq!(format_currency(1_000_000), "$1.0M");
        assert_eq!(format_currency(2_040_000), "$2.0M");
        assert_eq!(format_currency(2_060_000), "$2.1M");
        assert_eq!(format_currency(-1_260_000), "-$1.3M");
        assert_eq!(format_currency(-12_345), "-$12,345");
    }

    #[test]
    fn report_period_heading_uses_only_supported_ascii_glyphs() {
        let heading = finance_period_label(2, 3);
        assert_eq!(heading, "Year 2     Quarter 3");
        assert!(heading.is_ascii());
    }

    #[test]
    fn office_and_food_art_tracks_actual_visitors_and_hours() {
        assert_eq!(office_visible_worker_count(0), 0);
        assert_eq!(office_visible_worker_count(1), 1);
        assert_eq!(office_visible_worker_count(4), 4);
        assert_eq!(office_visible_worker_count(6), 6);
        assert_eq!(office_visible_worker_count(12), 6);
        assert_eq!(
            facility_crowd_level(FacilityKind::Office, true, 0, true),
            FacilityCrowd::Empty
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Office, true, 2, true),
            FacilityCrowd::Light
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Office, true, 4, true),
            FacilityCrowd::Heavy
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Office, true, 2, false),
            FacilityCrowd::Closed
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Shop, true, 3, false),
            FacilityCrowd::Closed
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Restaurant, true, 0, true),
            FacilityCrowd::Empty
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Restaurant, true, 69, true),
            FacilityCrowd::Light
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Restaurant, true, 70, true),
            FacilityCrowd::Heavy
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::Restaurant, true, 70, false),
            FacilityCrowd::Closed
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::FastFood, true, 34, true),
            FacilityCrowd::Light
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::FastFood, true, 35, true),
            FacilityCrowd::Heavy
        );
        assert_eq!(
            facility_crowd_level(FacilityKind::FastFood, false, 35, false),
            FacilityCrowd::Empty
        );
    }

    #[test]
    fn palette_buttons_are_individually_clickable() {
        for index in 0..PALETTE_BUTTONS.len() {
            let rect = palette_button_rect(index);
            assert_eq!(palette_button_at(rect.center()), Some(index));
        }
    }

    #[test]
    fn ground_and_sky_are_anchored_to_world_scrolling() {
        let anchor = 600.0;
        assert_eq!(world_ground_y(anchor, 0.0), anchor);
        assert_eq!(world_ground_y(anchor, 3.0), anchor + 3.0 * FLOOR_HEIGHT);

        let origin = sky_tile_origin_x(0.0);
        let shifted = sky_tile_origin_x(1.0);
        assert_eq!(shifted - origin, -CELL_WIDTH);
        assert_eq!(VERTICAL_WHEEL_FLOORS_PER_STEP, 0.5);
        assert_eq!(HORIZONTAL_WHEEL_COLUMNS_PER_STEP, 2.5);
    }

    #[test]
    fn each_emergency_stair_is_culled_independently_while_panning() {
        let [left, right] = emergency_stair_screen_xs(0, 100, 40.0);
        assert!(!horizontal_span_is_visible(
            left,
            EMERGENCY_STAIR_HALF_WIDTH,
            1_200.0
        ));
        assert!(horizontal_span_is_visible(
            right,
            EMERGENCY_STAIR_HALF_WIDTH,
            1_200.0
        ));
    }

    #[test]
    fn city_horizon_tiles_cover_the_entire_canvas_without_gaps() {
        for camera_x in [0.0, 1.25, 37.0, 190.75] {
            let layout = city_tile_layout(2048.0, camera_x, 96.0);
            assert!(!layout.is_empty());
            assert!(layout.first().unwrap().0 <= 0.0);
            assert!(layout.first().unwrap().0 + 96.0 >= 0.0);
            assert!(layout.last().unwrap().0 < 2048.0);
            assert!(layout.last().unwrap().0 + 96.0 >= 2048.0);
            for pair in layout.windows(2) {
                assert!((pair[1].0 - pair[0].0 - 96.0).abs() < 0.001);
                assert_ne!(pair[0].1, pair[1].1);
            }
        }
    }

    #[test]
    fn paused_people_use_one_fully_opaque_walk_frame() {
        for animation_seconds in [0.0, 0.125, 0.5, 1.0, 8.75] {
            let (current, next) = person_walk_frame_weights(animation_seconds, 17, true);
            assert!(
                (current == 1.0 && next == 0.0) || (current == 0.0 && next == 1.0),
                "paused frame must not remain between animation cels"
            );
        }

        let (current, next) = person_walk_frame_weights(0.125, 17, false);
        assert!((current + next - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn person_animation_cadence_is_independent_of_game_speed() {
        let frame_dt = 1.0 / 60.0;
        assert_eq!(
            visual_animation_delta(frame_dt, SimulationSpeed::Normal),
            frame_dt
        );
        assert_eq!(
            visual_animation_delta(frame_dt, SimulationSpeed::Tenfold),
            frame_dt
        );
        assert_eq!(
            visual_animation_delta(frame_dt, SimulationSpeed::Paused),
            0.0
        );
    }

    #[test]
    fn screen_rows_map_to_first_floor_and_first_basement() {
        assert_eq!(CELL_WIDTH, 8.0);
        assert_eq!(FLOOR_HEIGHT, 36.0);
        assert_eq!(ROOM_HEIGHT, 24.0);
        assert_eq!(FLOOR_HEIGHT / CELL_WIDTH, 4.5);
        let ground = 600.0;
        assert_eq!(world_floor_at_y(ground, ground - 12.0), 1);
        assert_eq!(world_floor_at_y(ground, ground + 12.0), -1);
        assert_eq!(floor_top_y(ground, 1), ground - FLOOR_HEIGHT);
        assert_eq!(floor_top_y(ground, -1), ground);
        assert_eq!(utility_strip_top_y(ground, 1), ground - FLOOR_HEIGHT);
        assert_eq!(
            tenant_room_top_y(ground, 1),
            ground - FLOOR_HEIGHT + (FLOOR_HEIGHT - ROOM_HEIGHT)
        );
        assert_eq!(tenant_room_top_y(ground, -1), ground);
        assert_eq!(utility_strip_top_y(ground, -1), ground + ROOM_HEIGHT);
        let first_floor_person_y = interior_person_top_y(ground, 0.0);
        assert_eq!(first_floor_person_y, tenant_room_top_y(ground, 1));
        assert_eq!(
            first_floor_person_y + PERSON_SPRITE_HEIGHT,
            tenant_room_top_y(ground, 1) + ROOM_HEIGHT
        );
    }

    #[test]
    fn roof_crane_tracks_the_left_edge_of_the_highest_floor() {
        let floors = [
            GridPosition { x: 2, floor: -1 },
            GridPosition { x: 8, floor: 1 },
            GridPosition { x: 9, floor: 1 },
            GridPosition { x: 6, floor: 3 },
            GridPosition { x: 7, floor: 3 },
        ];
        assert_eq!(
            roof_crane_anchor(&floors),
            Some(GridPosition { x: 6, floor: 3 })
        );
        assert_eq!(roof_crane_anchor(&floors[..1]), None);
    }

    #[test]
    fn lobby_drag_fills_every_crossed_column_in_both_directions() {
        assert_eq!(dragged_columns(3, 6), vec![4, 5, 6]);
        assert_eq!(dragged_columns(6, 3), vec![5, 4, 3]);
        assert!(dragged_columns(4, 4).is_empty());
    }

    #[test]
    fn shift_floor_fill_spans_the_full_supported_width() {
        let mut tower = Tower::new(16, 1_000_000);
        assert_eq!(
            floor_fill_plan(&tower, 1).columns,
            (0..16).collect::<Vec<_>>()
        );

        for x in 4..12 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        assert_eq!(
            floor_fill_plan(&tower, 2).columns,
            (4..12).collect::<Vec<_>>()
        );

        tower.place_floor(GridPosition { x: 6, floor: 2 }).unwrap();
        assert_eq!(
            floor_fill_plan(&tower, 2).columns,
            vec![4, 5, 7, 8, 9, 10, 11]
        );
        assert!(
            floor_fill_plan(&tower, simtower_core::MAX_FLOOR + 1)
                .columns
                .is_empty()
        );
    }

    #[test]
    fn shift_lobby_fill_and_preview_runs_cover_the_exact_paintable_area() {
        let mut tower = Tower::new(8, 1_000_000);
        for floor in 1..=3 {
            for x in 0..8 {
                tower.place_floor(GridPosition { x, floor }).unwrap();
            }
        }
        let plan = lobby_fill_plan(&tower, 1, 3);
        assert_eq!(plan.columns, (0..8).collect::<Vec<_>>());
        assert_eq!(contiguous_column_runs(&plan.columns), vec![(0, 8)]);

        let split = vec![0, 1, 2, 5, 6];
        assert_eq!(contiguous_column_runs(&split), vec![(0, 3), (5, 7)]);
    }

    #[test]
    fn shift_tenant_fill_starts_at_cursor_and_stops_at_first_blocker() {
        let mut tower = Tower::new(40, 2_000_000);
        for x in 0..40 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        tower
            .place(FacilityKind::Shop, GridPosition { x: 22, floor: 1 })
            .unwrap();

        let plan = tenant_fill_plan(
            &tower,
            FacilityKind::Office,
            GridPosition { x: 4, floor: 1 },
        );
        assert_eq!(
            plan.positions,
            vec![
                GridPosition { x: 4, floor: 1 },
                GridPosition { x: 13, floor: 1 }
            ]
        );
        assert_eq!(plan.limiting_error, Some(PlacementError::Occupied));
    }

    #[test]
    fn only_insufficient_funds_uses_the_no_money_cue() {
        assert!(placement_error_is_insufficient_funds(
            &PlacementError::InsufficientFunds {
                required: 2_000,
                available: 1_000,
            }
        ));
        assert!(!placement_error_is_insufficient_funds(
            &PlacementError::MissingFloor
        ));
    }

    #[test]
    fn facility_resource_registry_matches_the_original_type_groups() {
        assert_eq!(
            facility_resource_ids(FacilityKind::Lobby),
            FacilityResourceIds {
                bitmap: 2536,
                source_width: 992,
                source_height: 36,
                ambience: &[],
            }
        );
        assert_eq!(facility_resource_ids(FacilityKind::Office).bitmap, 1448);
        assert_eq!(
            facility_resource_ids(FacilityKind::Office).ambience,
            &[1448]
        );
        assert_eq!(facility_resource_ids(FacilityKind::Condo).bitmap, 1576);
        assert_eq!(
            facility_resource_ids(FacilityKind::Condo).ambience,
            &[1576, 1577]
        );
        assert_eq!(facility_resource_ids(FacilityKind::Restaurant).bitmap, 1384);
        assert_eq!(
            facility_resource_ids(FacilityKind::Restaurant).ambience,
            &[1384, 1385]
        );
        assert_eq!(facility_resource_ids(FacilityKind::Security).bitmap, 1896);
        assert!(
            facility_resource_ids(FacilityKind::Security)
                .ambience
                .is_empty()
        );
        assert_eq!(CONSTRUCTION_SOUND_ID, 7000);
        assert_eq!(LOBBY_SEGMENT_SOUND_ID, 7001);
        assert_eq!(NO_MONEY_SOUND_ID, 7002);
        assert_eq!(DEMOLITION_SOUND_ID, 7003);
        assert_eq!(PAYMENT_SOUND_ID, 10013);
        assert_eq!(FIRE_ALERT_SOUND_ID, 10006);
        assert_eq!(FIRE_RESPONSE_SOUND_ID, 10009);
        assert_eq!(FIRE_RESCUE_COST, 500_000);
        assert_eq!(ELEVATOR_MOVE_SOUND_ID, 6000);
        assert_eq!(ELEVATOR_OPEN_SOUND_IDS, [6001, 6002]);
        assert_eq!(CROWD_SOUND_ID, 8000);
    }

    #[test]
    fn embedded_facility_bitmaps_cover_the_registered_source_rectangles() {
        let resources: [(FacilityKind, AssetRef, u32, u32); 13] = [
            (FacilityKind::Lobby, LOBBY_FIRST_STORY_BMPS[0], 992, 36),
            (FacilityKind::Lobby, LOBBY_FIRST_STORY_BMPS[1], 992, 36),
            (FacilityKind::Lobby, LOBBY_FIRST_STORY_BMPS[2], 992, 36),
            (FacilityKind::Lobby, LOBBY_SECOND_STORY_BMPS[0], 664, 36),
            (FacilityKind::Lobby, LOBBY_SECOND_STORY_BMPS[1], 664, 36),
            (FacilityKind::Lobby, LOBBY_SECOND_STORY_BMPS[2], 664, 36),
            (FacilityKind::Lobby, LOBBY_THIRD_STORY_BMPS[0], 336, 36),
            (FacilityKind::Lobby, LOBBY_THIRD_STORY_BMPS[1], 336, 36),
            (FacilityKind::Lobby, LOBBY_THIRD_STORY_BMPS[2], 336, 36),
            (FacilityKind::Office, OFFICE_1_BMP, 288, 24),
            (FacilityKind::Condo, CONDO_STATE_BMPS[3], 128, 24),
            (FacilityKind::Restaurant, RESTAURANT_1_BMP, 384, 24),
            (FacilityKind::Security, SECURITY_BMP, 128, 24),
        ];

        for (kind, bytes, expected_width, expected_height) in resources {
            let image = DibImage::decode_bmp(bytes.bytes().unwrap())
                .expect("registered bitmap must decode");
            let mapping = facility_resource_ids(kind);
            assert_eq!(image.width(), expected_width, "wrong bitmap for {kind:?}");
            assert_eq!(image.height(), expected_height, "wrong bitmap for {kind:?}");
            if kind == FacilityKind::Lobby {
                assert!(image.width() >= (LOBBY_FACADE_OFFSET + LOBBY_FACADE_WIDTH) as u32);
            } else {
                assert!(image.width() >= u32::from(mapping.source_width));
            }
            assert!(image.height() >= u32::from(mapping.source_height));
        }

        for (name, bytes, width) in [
            ("vacant office", OFFICE_VACANT_BMP, 144),
            ("occupied condo", CONDO_STATE_BMPS[1], 128),
            ("occupied single room", HOTEL_SINGLE_OCCUPIED_1_BMP, 256),
            ("occupied twin room", HOTEL_TWIN_OCCUPIED_1_BMP, 384),
            ("occupied suite", HOTEL_SUITE_OCCUPIED_1_BMP, 640),
            ("occupied restaurant", RESTAURANT_OCCUPIED_1_BMP, 384),
            ("occupied fast food", FAST_FOOD_OCCUPIED_1_BMP, 256),
        ] {
            let image = DibImage::decode_bmp(bytes.bytes().unwrap())
                .expect("tenant state bitmap must decode");
            assert_eq!((image.width(), image.height()), (width, 24), "{name}");
        }

        let opening_theme = simtower_formats::inspect_wave(OPENING_THEME_WAV.bytes().unwrap())
            .expect("opening theme resource must be a valid WAVE file");
        assert_eq!(opening_theme.channels, 1);
        let sound = simtower_formats::inspect_wave(CONSTRUCTION_WAV.bytes().unwrap())
            .expect("construction sound resource must be a valid WAVE file");
        assert_eq!(sound.channels, 1);
        let lobby_sound = simtower_formats::inspect_wave(LOBBY_SEGMENT_WAV.bytes().unwrap())
            .expect("lobby segment sound resource must be a valid WAVE file");
        assert_eq!(lobby_sound.channels, 1);
        let no_money_sound = simtower_formats::inspect_wave(NO_MONEY_WAV.bytes().unwrap())
            .expect("no-money sound resource must be a valid WAVE file");
        assert_eq!(no_money_sound.channels, 1);
        let demolition_sound = simtower_formats::inspect_wave(DEMOLITION_WAV.bytes().unwrap())
            .expect("demolition sound resource must be a valid WAVE file");
        assert_eq!(demolition_sound.channels, 1);
        let payment_sound = simtower_formats::inspect_wave(PAYMENT_WAV.bytes().unwrap())
            .expect("payment sound resource must be a valid WAVE file");
        assert_eq!(payment_sound.channels, 1);
        let fire_alert_sound = simtower_formats::inspect_wave(FIRE_ALERT_WAV.bytes().unwrap())
            .expect("fire alert sound resource must be a valid WAVE file");
        assert_eq!(fire_alert_sound.channels, 1);
        let fire_response_sound =
            simtower_formats::inspect_wave(FIRE_RESPONSE_WAV.bytes().unwrap())
                .expect("fire response sound resource must be a valid WAVE file");
        assert_eq!(fire_response_sound.channels, 1);
        for bytes in [
            ELEVATOR_MOVE_WAV,
            ELEVATOR_OPEN_1_WAV,
            ELEVATOR_OPEN_2_WAV,
            CROWD_1_WAV,
        ] {
            let sound = simtower_formats::inspect_wave(bytes.bytes().unwrap())
                .expect("traffic sound resource must be a valid WAVE file");
            assert_eq!(sound.channels, 1);
        }

        let floor_strip = DibImage::decode_bmp(FLOOR_STRIP_BMP.bytes().unwrap())
            .expect("floor strip must decode");
        assert_eq!((floor_strip.width(), floor_strip.height()), (128, 12));
        let sky =
            DibImage::decode_bmp(SKY_BMP.bytes().unwrap()).expect("sky and dirt tile must decode");
        assert_eq!((sky.width(), sky.height()), (200, 288));
        assert_eq!(SKY_HORIZON, 264.0);
        let ground =
            DibImage::decode_bmp(GROUND_BMP.bytes().unwrap()).expect("ground gradient must decode");
        assert_eq!((ground.width(), ground.height()), (32, 360));
        let stairs = DibImage::decode_bmp(EMERGENCY_STAIRS_BMP.bytes().unwrap())
            .expect("emergency stair bitmap must decode");
        assert_eq!((stairs.width(), stairs.height()), (48, 36));
        let crane = DibImage::decode_bmp(ROOF_CRANE_BMP.bytes().unwrap())
            .expect("roof crane bitmap must decode");
        assert_eq!((crane.width(), crane.height()), (36, 36));
        let awning =
            DibImage::decode_bmp(LOBBY_AWNING_BMP.bytes().unwrap()).expect("awning must decode");
        assert_eq!((awning.width(), awning.height()), (112, 36));
        let scaffolding =
            DibImage::decode_bmp(SCAFFOLD_BMP.bytes().unwrap()).expect("scaffolding must decode");
        assert_eq!((scaffolding.width(), scaffolding.height()), (328, 36));
        let tenant_people = DibImage::decode_bmp(PEOPLE_SILHOUETTE_BMPS[0].bytes().unwrap())
            .expect("tenant occupant silhouette strip must decode");
        assert_eq!((tenant_people.width(), tenant_people.height()), (640, 36));
        let emergency_guards = DibImage::decode_bmp(PEOPLE_SILHOUETTE_BMPS[1].bytes().unwrap())
            .expect("emergency guard strip must decode");
        assert_eq!(
            (emergency_guards.width(), emergency_guards.height()),
            (160, 36)
        );
        assert!(emergency_guard_source_x(true, 0.0) + 16.0 <= emergency_guards.width() as f32);
        let santa = DibImage::decode_bmp(SANTA_BMP.bytes().unwrap())
            .expect("Santa event bitmap must decode");
        assert_eq!((santa.width(), santa.height()), (140, 48));
        let treasure = DibImage::decode_bmp(TREASURE_BMP.bytes().unwrap())
            .expect("treasure event bitmap must decode");
        assert_eq!((treasure.width(), treasure.height()), (84, 80));
        let shaft = DibImage::decode_bmp(ELEVATOR_SHAFT_BMP.bytes().unwrap())
            .expect("empty elevator shaft infrastructure must decode");
        assert_eq!((shaft.width(), shaft.height()), (16, 36));
        for palette in [PALETTE_BMP, PALETTE_SELECTED_BMP, PALETTE_DISABLED_BMP] {
            let palette = DibImage::decode_bmp(palette.bytes().unwrap())
                .expect("toolbox state bitmap must decode");
            assert_eq!((palette.width(), palette.height()), (256, 128));
        }
    }

    #[test]
    fn seasonal_dates_select_the_original_hidden_sky_events() {
        assert_eq!(
            seasonal_event_for_date("12-25"),
            Some(SeasonalEventKind::Santa)
        );
        assert_eq!(
            seasonal_event_for_date("10-31\n"),
            Some(SeasonalEventKind::Witch)
        );
        assert_eq!(seasonal_event_for_date("10-03"), None);
    }

    #[test]
    fn original_weather_palettes_follow_day_and_time() {
        assert!(!weather_is_raining(1));
        assert!(weather_is_raining(3));
        assert!(weather_is_raining(5));
        assert_eq!(rain_palette_index(0), 0);
        assert_eq!(rain_palette_index(12 * 60), 5);
        assert_eq!(rain_palette_index(23 * 60 + 59), 9);
        let rain = rain_tile_layout(1_990.0, TOPBAR_HEIGHT, 1_145.0, 3.5, 32.0, 360.0);
        assert!(!rain.is_empty());
        assert!(
            rain.iter()
                .map(|origin| origin.x)
                .fold(f32::INFINITY, f32::min)
                <= 0.0
        );
        assert!(
            rain.iter()
                .map(|origin| origin.x + 32.0)
                .fold(f32::NEG_INFINITY, f32::max)
                >= 1_990.0
        );
        assert!(
            rain.iter()
                .map(|origin| origin.y)
                .fold(f32::INFINITY, f32::min)
                <= TOPBAR_HEIGHT
        );
        assert!(
            rain.iter()
                .map(|origin| origin.y + 360.0)
                .fold(f32::NEG_INFINITY, f32::max)
                >= 1_145.0
        );
        assert_eq!(world_lighting(2 * 60).night_alpha, 118);
        assert_eq!(world_lighting(12 * 60).night_alpha, 0);
        assert!(world_lighting(5 * 60 + 30).warm_alpha > 0);
        assert!(world_lighting(18 * 60 + 30).warm_alpha > 0);
        assert_eq!(world_lighting(19 * 60).night_alpha, 118);
    }

    #[test]
    fn tenant_light_frames_follow_time_without_swapping_office_scenes() {
        assert_eq!(office_sprite_frame(12 * 60), 0);
        assert_eq!(office_sprite_frame(20 * 60), 1);
        assert_eq!(tenant_variant_count(FacilityKind::Office), Some(6));
        assert_eq!(hotel_sprite_frame(12 * 60, 1, false, false), 0);
        assert_eq!(hotel_sprite_frame(20 * 60, 1, false, false), 1);
        assert_eq!(hotel_sprite_frame(12 * 60, 0, false, false), 2);
        assert_eq!(hotel_sprite_frame(20 * 60, 0, false, false), 3);
        assert_eq!(hotel_sprite_frame(12 * 60, 0, true, false), 4);
        assert_eq!(hotel_sprite_frame(20 * 60, 0, true, false), 5);
        assert_eq!(hotel_sprite_frame(12 * 60, 0, false, true), 6);
        assert_eq!(hotel_sprite_frame(20 * 60, 0, false, true), 7);
        let checkout_times = (1..=512).map(hotel_checkout_minute).collect::<Vec<_>>();
        assert!(
            checkout_times
                .iter()
                .all(|minute| { (6 * 60 + 30..=12 * 60).contains(minute) })
        );
        assert!(checkout_times.iter().copied().min().unwrap() < 7 * 60);
        assert!(checkout_times.iter().copied().max().unwrap() > 11 * 60 + 30);
    }

    #[test]
    fn rating_stars_remove_the_button_cell_but_keep_unearned_gray_art() {
        let earned =
            DibImage::decode_bmp(STAR_ON_BMP.bytes().unwrap()).expect("earned star should decode");
        let mut earned_rgba = earned.rgba().to_vec();
        apply_star_transparency(&mut earned_rgba, true);
        assert_eq!(
            earned_rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0)
                .count(),
            148
        );

        let unearned = DibImage::decode_bmp(STAR_OFF_BMP.bytes().unwrap())
            .expect("unearned star should decode");
        let mut unearned_rgba = unearned.rgba().to_vec();
        apply_star_transparency(&mut unearned_rgba, false);
        let opaque = unearned_rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .collect::<Vec<_>>();
        assert_eq!(opaque.len(), 148);
        assert!(opaque.iter().all(|pixel| pixel[..3] == [140, 140, 140]));
    }

    #[test]
    fn detailed_people_groups_follow_destination_roles() {
        assert_eq!(person_sprite_sheet(Some(FacilityKind::Office), 2), 0);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::HotelSingle), 2), 1);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::Medical), 2), 2);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::FastFood), 2), 3);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::Cathedral), 2), 4);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::Shop), 2), 5);
        assert_eq!(person_sprite_sheet(Some(FacilityKind::Recycling), 2), 6);
        assert_eq!(
            traveler_sprite_sheet(
                PersonRole::Housekeeper,
                Some(FacilityKind::Housekeeping),
                Some(FacilityKind::HotelSingle),
                2,
            ),
            1,
            "housekeeping staff keep their dedicated sprite while visiting hotel rooms"
        );
        assert_eq!(
            traveler_sprite_sheet(PersonRole::Child, None, Some(FacilityKind::FastFood), 2),
            6,
            "children keep the original child walking sheet at every destination"
        );
        assert_eq!(
            traveler_sprite_sheet(
                PersonRole::WomanWithChild,
                Some(FacilityKind::Condo),
                Some(FacilityKind::Shop),
                2,
            ),
            5,
            "mothers use the original woman/customer walking artwork"
        );
    }

    #[test]
    fn traffic_audio_events_follow_people_and_elevator_transitions() {
        let idle = TrafficAudioSnapshot {
            people: 2,
            moving_cars: 0,
            passengers: 0,
            escalator_riders: 0,
        };
        let arriving = TrafficAudioSnapshot {
            people: 3,
            moving_cars: 1,
            passengers: 0,
            escalator_riders: 0,
        };
        assert_eq!(
            traffic_sound_events(idle, arriving),
            vec![TrafficSoundEvent::ElevatorMove, TrafficSoundEvent::Crowd]
        );

        let boarded = TrafficAudioSnapshot {
            people: 3,
            moving_cars: 0,
            passengers: 1,
            escalator_riders: 1,
        };
        assert_eq!(
            traffic_sound_events(arriving, boarded),
            vec![
                TrafficSoundEvent::ElevatorOpen,
                TrafficSoundEvent::EscalatorMove
            ]
        );
    }

    #[test]
    fn escalator_rider_art_tracks_progress_in_both_directions() {
        assert_eq!(escalator_frame_for_position(3, 3, 2.0, 1), 0);
        assert_eq!(escalator_frame_for_position(3, 3, 2.5, 1), 4);
        assert_eq!(escalator_frame_for_position(3, 3, 3.0, 1), 7);
        assert_eq!(escalator_frame_for_position(3, 4, 3.0, 1), 0);
        assert_eq!(escalator_frame_for_position(3, 4, 2.5, 1), 4);
        assert_eq!(escalator_frame_for_position(3, 4, 2.0, 1), 7);
        assert_eq!(escalator_frame_for_position(1, 1, 1.0, 2), 4);
    }

    #[test]
    fn escalator_motor_copy_is_quieter_without_changing_wave_headers() {
        let source = ELEVATOR_MOVE_WAV
            .bytes()
            .expect("elevator sound should load");
        let quiet = attenuate_unsigned_pcm8_wav(source, 0.25);
        assert_eq!(&quiet[..12], &source[..12]);
        assert_eq!(quiet.len(), source.len());
        assert_ne!(quiet, source);
    }

    #[test]
    fn payment_audio_queues_one_ding_per_income_batch() {
        let mut sounds = SimulationSoundState::default();
        sounds.queue_payments(&[
            IncomeEvent {
                facility_id: 1,
                kind: FacilityKind::Office,
                amount: 15_000,
            },
            IncomeEvent {
                facility_id: 4,
                kind: FacilityKind::Condo,
                amount: 8_000,
            },
            IncomeEvent {
                facility_id: 2,
                kind: FacilityKind::Restaurant,
                amount: -4_000,
            },
            IncomeEvent {
                facility_id: 3,
                kind: FacilityKind::FastFood,
                amount: 0,
            },
        ]);
        assert_eq!(sounds.payment_pending, 1);
        sounds.queue_payments(&[IncomeEvent {
            facility_id: 5,
            kind: FacilityKind::Shop,
            amount: 10_000,
        }]);
        assert_eq!(sounds.payment_pending, 2);
    }

    #[test]
    fn elevator_queue_palette_progresses_from_black_to_pink_to_red() {
        let mut rgba = vec![
            255, 255, 0, 255, // colored queue-person pixel
            222, 222, 222, 255, // light-gray transparent background
        ];
        apply_queue_sprite_palette(&mut rgba, Some(QUEUE_CONCERNED_PINK));
        assert_eq!(&rgba[..4], &[255, 92, 152, 255]);
        assert_eq!(rgba[7], 0);
        assert_eq!(
            [
                queue_sheet_for_mood(PersonMood::Calm),
                queue_sheet_for_mood(PersonMood::Concerned),
                queue_sheet_for_mood(PersonMood::Angry),
            ],
            [0, 1, 2]
        );
    }

    #[test]
    fn elevator_bell_plays_once_for_every_five_open_events() {
        let mut phase = 0;
        let decisions = (0..10)
            .map(|_| should_play_elevator_bell(&mut phase))
            .collect::<Vec<_>>();
        assert_eq!(
            decisions,
            [
                true, false, false, false, false, true, false, false, false, false
            ]
        );
    }

    #[test]
    fn tenant_variant_families_cover_every_original_style() {
        assert_eq!(HOTEL_SINGLE_VARIANT_IDS, [1192, 1194]);
        assert_eq!(HOTEL_TWIN_VARIANT_IDS, [1256, 1258, 1260, 1262]);
        assert_eq!(HOTEL_SUITE_VARIANT_IDS, [1320, 1322]);
        assert_eq!(OFFICE_VARIANT_IDS, [1448, 1448, 1449, 1449, 1450, 1450]);
        assert_eq!(CONDO_VARIANT_IDS, [1576, 1581, 1586]);
        assert_eq!(RESTAURANT_VARIANT_IDS, [1384, 1386, 1388, 1390, 1392]);
        assert_eq!(FAST_FOOD_VARIANT_IDS, [1768, 1770, 1772, 1774, 1776]);
        assert_eq!(
            SHOP_VARIANT_IDS,
            [
                1640, 1641, 1642, 1643, 1644, 1645, 1646, 1647, 1648, 1649, 1650,
            ]
        );

        let families: [(&str, &[AssetRef], u32, u32); 8] = [
            (
                "single hotel",
                &[HOTEL_SINGLE_1_BMP, HOTEL_SINGLE_2_BMP],
                32,
                24,
            ),
            (
                "twin hotel",
                &[
                    HOTEL_TWIN_1_BMP,
                    HOTEL_TWIN_2_BMP,
                    HOTEL_TWIN_3_BMP,
                    HOTEL_TWIN_4_BMP,
                ],
                48,
                24,
            ),
            ("suite", &[HOTEL_SUITE_1_BMP, HOTEL_SUITE_2_BMP], 80, 24),
            (
                "office sheets",
                &[OFFICE_1_BMP, OFFICE_2_BMP, OFFICE_3_BMP],
                288,
                24,
            ),
            (
                "condo",
                &[
                    CONDO_STATE_BMPS[3],
                    CONDO_STATE_BMPS[8],
                    CONDO_STATE_BMPS[13],
                ],
                128,
                24,
            ),
            (
                "restaurant",
                &[
                    RESTAURANT_1_BMP,
                    RESTAURANT_2_BMP,
                    RESTAURANT_3_BMP,
                    RESTAURANT_4_BMP,
                    RESTAURANT_5_BMP,
                ],
                384,
                24,
            ),
            (
                "fast food",
                &[
                    FAST_FOOD_1_BMP,
                    FAST_FOOD_2_BMP,
                    FAST_FOOD_3_BMP,
                    FAST_FOOD_4_BMP,
                    FAST_FOOD_5_BMP,
                ],
                256,
                24,
            ),
            (
                "shop",
                &[
                    SHOP_1_BMP,
                    SHOP_2_BMP,
                    SHOP_3_BMP,
                    SHOP_4_BMP,
                    SHOP_5_BMP,
                    SHOP_6_BMP,
                    SHOP_7_BMP,
                    SHOP_8_BMP,
                    SHOP_9_BMP,
                    SHOP_10_BMP,
                    SHOP_11_BMP,
                ],
                288,
                24,
            ),
        ];
        for (name, family, expected_width, expected_height) in families {
            for bytes in family {
                let image = DibImage::decode_bmp(bytes.bytes().unwrap())
                    .unwrap_or_else(|_| panic!("{name} variant must decode"));
                assert_eq!(
                    (image.width(), image.height()),
                    (expected_width, expected_height),
                    "wrong dimensions for {name} variant"
                );
            }
        }
    }

    #[test]
    fn tenant_visual_choice_is_stable_and_can_select_every_style() {
        for (kind, count) in [
            (FacilityKind::HotelSingle, HOTEL_SINGLE_VARIANT_IDS.len()),
            (FacilityKind::HotelTwin, HOTEL_TWIN_VARIANT_IDS.len()),
            (FacilityKind::HotelSuite, HOTEL_SUITE_VARIANT_IDS.len()),
            (
                FacilityKind::Office,
                tenant_variant_count(FacilityKind::Office).unwrap(),
            ),
            (FacilityKind::Condo, CONDO_VARIANT_IDS.len()),
            (FacilityKind::Restaurant, RESTAURANT_VARIANT_IDS.len()),
            (FacilityKind::FastFood, FAST_FOOD_VARIANT_IDS.len()),
            (FacilityKind::Shop, SHOP_VARIANT_IDS.len()),
        ] {
            let mut seen = vec![false; count];
            for id in 1..=512 {
                let facility = Facility {
                    id,
                    kind,
                    position: GridPosition {
                        x: id as u16,
                        floor: 1,
                    },
                    occupancy: simtower_core::FacilityOccupancy::Occupied,
                    price_level: 1,
                    stories: kind.spec().height as u8,
                };
                let first = facility_variant_index(&facility, seen.len());
                let second = facility_variant_index(&facility, seen.len());
                assert_eq!(first, second);
                seen[first] = true;
            }
            assert!(
                seen.into_iter().all(|was_selected| was_selected),
                "{kind:?} did not exercise every visual style"
            );
        }
    }

    #[test]
    fn multistyle_tenant_placements_exhaust_each_style_before_repeating() {
        for kind in [
            FacilityKind::Office,
            FacilityKind::Condo,
            FacilityKind::HotelSingle,
            FacilityKind::HotelTwin,
            FacilityKind::HotelSuite,
            FacilityKind::Restaurant,
            FacilityKind::FastFood,
            FacilityKind::Shop,
        ] {
            let variant_count = tenant_variant_count(kind).unwrap();
            let mut bag = TenantVariantBag::default();
            let choices = (0..variant_count * 3)
                .map(|_| bag.next(variant_count))
                .collect::<Vec<_>>();

            for cycle in choices.chunks_exact(variant_count) {
                let mut sorted = cycle.to_vec();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..variant_count).collect::<Vec<_>>(), "{kind:?}");
            }
            assert!(
                choices.windows(2).all(|pair| pair[0] != pair[1]),
                "{kind:?} repeated the same style back-to-back"
            );
        }
    }

    #[test]
    fn sprite_hit_testing_maps_the_cursor_hotspot_into_the_displayed_frame() {
        let source = Rect::new(20.0, 4.0, 10.0, 8.0);
        let destination = Rect::new(100.0, 200.0, 40.0, 32.0);
        assert_eq!(
            scaled_source_pixel(source, destination, vec2(120.0, 216.0)),
            Some((25, 8))
        );
        assert_eq!(
            scaled_source_pixel(source, destination, vec2(99.0, 216.0)),
            None
        );
    }

    #[test]
    fn alpha_masks_allow_transparent_sprite_pixels_to_click_through() {
        let rgba = [
            255, 255, 255, 0, // transparent
            10, 20, 30, 255, // opaque
        ];
        let mask = AlphaMask::from_rgba(2, 1, &rgba).expect("valid mask");
        assert!(!mask.is_opaque(0, 0));
        assert!(mask.is_opaque(1, 0));
        assert!(!mask.is_opaque(2, 0));
    }
}
