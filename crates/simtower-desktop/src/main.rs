#[cfg(feature = "bundled-resources")]
use macroquad::miniquad::conf::Icon;
use macroquad::miniquad::window;
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use simtower_core::{
    Clock, ElevatorDirection, ElevatorMode, FLOOR_CONSTRUCTION_COST, Facility, FacilityKind,
    GridPosition, IncomeEvent, MAX_ELEVATOR_QUEUE_PER_FLOOR, MIN_FLOOR, PersonActivity, PersonMood,
    PlacementError, SimulationSpeed, Tower, TrafficSimulation, elevator_car_cost, facility_is_open,
};
use simtower_formats::DibImage;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

mod resources;
use resources::*;

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
const SAVE_FORMAT_VERSION: u32 = 1;
const SAVE_EXTENSION: &str = "smtower";
const DEFAULT_SAVE_NAME: &str = "Tower";
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
const PALETTE_X: f32 = 12.0;
const PALETTE_Y: f32 = 78.0;
const PALETTE_TITLE_HEIGHT: f32 = 20.0;
const PALETTE_BUTTON_SIZE: f32 = 38.0;
const PALETTE_GAP: f32 = 2.0;
const PALETTE_COLUMNS: usize = 3;
const SUBMENU_WIDTH: f32 = 164.0;
const SUBMENU_HEADER_HEIGHT: f32 = 19.0;
const SUBMENU_ROW_HEIGHT: f32 = 28.0;

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
const FIRE_RESPONSE_SOUND_ID: u16 = 10004;
const ELEVATOR_MOVE_SOUND_ID: u16 = 6000;
const ELEVATOR_OPEN_SOUND_IDS: [u16; 2] = [6001, 6002];
const ELEVATOR_BELL_DIVISOR: u8 = 5;
const MAX_PAYMENT_SOUND_QUEUE: u8 = 30;
const PAYMENT_SOUND_INTERVAL: f32 = 0.5;
const FIRE_RESCUE_COST: i64 = 300_000;
const FIRE_QUICK_RESPONSE_SECONDS: f32 = 22.0;
const FIRE_HELICOPTER_DURATION_SECONDS: f32 = 12.0;
const FIRE_RESPONSE_DECISION_SECONDS: f32 = 8.0;
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
const LOBBY_FACADE_X: f32 = 272.0;
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FireResponse {
    AwaitingDecision,
    Security,
    Helicopter,
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
    #[serde(default = "default_true")]
    automatic_finance_reports: bool,
    #[serde(default)]
    tenant_variants: HashMap<u64, usize>,
    #[serde(default)]
    tenant_variant_bags: HashMap<FacilityKind, TenantVariantBag>,
}

struct SaveNameDialog {
    name: String,
    error: Option<String>,
    overwrite_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameMenuAction {
    New,
    Save,
    Load,
    AutomaticReports,
    Quit,
}

const GAME_MENU_ACTIONS: [(GameMenuAction, &str); 5] = [
    (GameMenuAction::New, "New"),
    (GameMenuAction::Save, "Save"),
    (GameMenuAction::Load, "Load"),
    (GameMenuAction::AutomaticReports, "Automatic reports"),
    (GameMenuAction::Quit, "Quit"),
];

const fn default_true() -> bool {
    true
}

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
    let image =
        DibImage::decode_bmp(SIMTOWER_ICON_BMP.bytes()?).map_err(|error| error.to_string())?;
    Ok(Icon {
        small: resize_icon::<{ 16 * 16 * 4 }>(&image, 16),
        medium: resize_icon::<{ 32 * 32 * 4 }>(&image, 32),
        big: resize_icon::<{ 64 * 64 * 4 }>(&image, 64),
    })
}

#[cfg(feature = "bundled-resources")]
fn resize_icon<const LEN: usize>(image: &DibImage, side: usize) -> [u8; LEN] {
    assert_eq!(LEN, side * side * 4);
    let mut output = [0_u8; LEN];
    let source_width = image.width() as usize;
    let source_height = image.height() as usize;
    for y in 0..side {
        for x in 0..side {
            let source_x = x * source_width / side;
            let source_y = y * source_height / side;
            let source = (source_y * source_width + source_x) * 4;
            let target = (y * side + x) * 4;
            output[target..target + 4].copy_from_slice(&image.rgba()[source..source + 4]);
        }
    }
    output
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
}

impl TrafficAudioSnapshot {
    fn capture(traffic: &TrafficSimulation) -> Self {
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
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrafficSoundEvent {
    ElevatorMove,
    ElevatorOpen,
    Crowd,
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
    events
}

#[derive(Default)]
struct SimulationSoundState {
    previous: TrafficAudioSnapshot,
    initialized: bool,
    elevator_move_cooldown: f32,
    elevator_open_cooldown: f32,
    crowd_cooldown: f32,
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
    ) {
        self.elevator_move_cooldown = (self.elevator_move_cooldown - dt).max(0.0);
        self.elevator_open_cooldown = (self.elevator_open_cooldown - dt).max(0.0);
        self.crowd_cooldown = (self.crowd_cooldown - dt).max(0.0);
        if !simulation_paused {
            self.payment_cooldown = (self.payment_cooldown - dt).max(0.0);
            if self.payment_pending > 0 && self.payment_cooldown <= 0.0 {
                sounds.play_payment();
                self.payment_pending -= 1;
                self.payment_cooldown = PAYMENT_SOUND_INTERVAL;
            }
        }

        let current = TrafficAudioSnapshot::capture(traffic);
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
                _ => {}
            }
        }
        self.previous = current;
    }
}

struct App {
    tower: Tower,
    traffic: TrafficSimulation,
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
    visual_people_x: HashMap<u64, f32>,
    tenant_variants: HashMap<u64, usize>,
    tenant_variant_bags: HashMap<FacilityKind, TenantVariantBag>,
    people_animation_seconds: f64,
    view_mode: ViewMode,
    speed_menu_open: bool,
    last_running_speed: SimulationSpeed,
    seasonal_event: Option<SeasonalEventState>,
    treasure_popup: Option<TreasurePopup>,
    fire_event: Option<FireEventState>,
    next_fire_minute: u64,
    last_star_rating: u8,
    promotion_dialog_open: bool,
    promotion_resume_speed: Option<SimulationSpeed>,
    promotion_rating: u8,
    finance_ledger: FinanceLedger,
    finance_panel_open: bool,
    finance_resume_speed: Option<SimulationSpeed>,
    automatic_finance_reports: bool,
    intro_remaining: f32,
    game_menu_open: bool,
    mode_menu_open: bool,
    save_name_dialog: Option<SaveNameDialog>,
}

impl App {
    fn new(assets: OriginalAssets) -> Self {
        let tower = fresh_tower();
        let finance_ledger = FinanceLedger::new(tower.cash());
        Self {
            tower,
            traffic: TrafficSimulation::new(),
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
            visual_people_x: HashMap::new(),
            tenant_variants: HashMap::new(),
            tenant_variant_bags: HashMap::new(),
            people_animation_seconds: 0.0,
            view_mode: ViewMode::Edit,
            speed_menu_open: false,
            last_running_speed: SimulationSpeed::Normal,
            seasonal_event: seasonal_event_for_system_date()
                .map(|kind| SeasonalEventState { kind, x: -180.0 }),
            treasure_popup: None,
            fire_event: None,
            next_fire_minute: 11 * 60,
            last_star_rating: 1,
            promotion_dialog_open: false,
            promotion_resume_speed: None,
            promotion_rating: 1,
            finance_ledger,
            finance_panel_open: false,
            finance_resume_speed: None,
            automatic_finance_reports: true,
            intro_remaining: 2.4,
            game_menu_open: false,
            mode_menu_open: false,
            save_name_dialog: None,
        }
    }

    fn start_new_tower(&mut self) {
        self.tower = fresh_tower();
        self.traffic = TrafficSimulation::new();
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
        self.visual_people_x.clear();
        self.tenant_variants.clear();
        self.tenant_variant_bags.clear();
        self.people_animation_seconds = 0.0;
        self.view_mode = ViewMode::Edit;
        self.speed_menu_open = false;
        self.last_running_speed = SimulationSpeed::Normal;
        self.seasonal_event =
            seasonal_event_for_system_date().map(|kind| SeasonalEventState { kind, x: -180.0 });
        self.treasure_popup = None;
        self.fire_event = None;
        self.next_fire_minute = 11 * 60;
        self.last_star_rating = 1;
        self.promotion_dialog_open = false;
        self.promotion_resume_speed = None;
        self.promotion_rating = 1;
        self.finance_ledger = FinanceLedger::new(self.tower.cash());
        self.finance_panel_open = false;
        self.finance_resume_speed = None;
        self.intro_remaining = 0.0;
        self.game_menu_open = false;
        self.mode_menu_open = false;
        self.save_name_dialog = None;
    }

    fn begin_save_name_dialog(&mut self) {
        self.save_name_dialog = Some(SaveNameDialog {
            name: DEFAULT_SAVE_NAME.to_owned(),
            error: None,
            overwrite_path: None,
        });
        self.status = "Enter a save-game name".to_owned();
    }

    fn close_finance_panel(&mut self) {
        self.finance_panel_open = false;
        if let Some(speed) = self.finance_resume_speed.take() {
            self.tower.clock.speed = speed;
        }
    }

    fn update_save_name_dialog(&mut self) {
        let confirming_overwrite = self
            .save_name_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.overwrite_path.is_some());
        if confirming_overwrite {
            let mut confirm = is_key_pressed(KeyCode::Enter);
            let mut decline = is_key_pressed(KeyCode::Escape);
            if is_mouse_button_pressed(MouseButton::Left) {
                let point = vec2(mouse_position().0, mouse_position().1);
                confirm |= save_dialog_save_rect().contains(point);
                decline |= save_dialog_cancel_rect().contains(point);
            }
            if decline {
                if let Some(dialog) = &mut self.save_name_dialog {
                    dialog.overwrite_path = None;
                    dialog.error = Some("Existing save kept; enter a different name".to_owned());
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
                dialog.name.pop();
                dialog.error = None;
            }
            while let Some(character) = get_char_pressed() {
                if !character.is_control() && dialog.name.chars().count() < 64 {
                    dialog.name.push(character);
                    dialog.error = None;
                }
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let point = vec2(mouse_position().0, mouse_position().1);
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
            automatic_finance_reports: self.automatic_finance_reports,
            tenant_variants: self.tenant_variants.clone(),
            tenant_variant_bags: self.tenant_variant_bags.clone(),
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
        self.automatic_finance_reports = save.automatic_finance_reports;
        self.finance_ledger.ensure_initialized(self.tower.cash());
        self.tool = ToolMode::Inspect;
        self.view_mode = ViewMode::Edit;
        self.open_menu = None;
        self.speed_menu_open = false;
        self.game_menu_open = false;
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
        self.simulation_sounds = SimulationSoundState::default();
        self.treasure_popup = None;
        self.fire_event = None;
        self.promotion_dialog_open = false;
        self.promotion_resume_speed = None;
        self.last_star_rating = self.star_rating();
        self.promotion_rating = self.last_star_rating;
        self.intro_remaining = 0.0;
        self.sync_tenant_variants();
        Ok(())
    }

    fn update(&mut self) {
        let dt = get_frame_time();
        if self.intro_remaining > 0.0 {
            self.intro_remaining = (self.intro_remaining - dt).max(0.0);
            if is_key_pressed(KeyCode::Escape)
                || is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
            {
                self.intro_remaining = 0.0;
            }
            return;
        }
        if self.save_name_dialog.is_some() {
            self.update_save_name_dialog();
            return;
        }
        if self.finance_panel_open {
            if is_key_pressed(KeyCode::Escape)
                || (is_mouse_button_pressed(MouseButton::Left)
                    && finance_ok_rect().contains(vec2(mouse_position().0, mouse_position().1)))
            {
                self.close_finance_panel();
            }
            return;
        }
        if self.promotion_dialog_open {
            let dismiss = is_key_pressed(KeyCode::Escape)
                || is_key_pressed(KeyCode::Enter)
                || (is_mouse_button_pressed(MouseButton::Left)
                    && promotion_ok_rect().contains(vec2(mouse_position().0, mouse_position().1)));
            if dismiss {
                self.promotion_dialog_open = false;
                if let Some(speed) = self.promotion_resume_speed.take() {
                    self.tower.clock.speed = speed;
                }
            }
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
        self.traffic.sync_with_tower(&self.tower);
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
        self.finance_ledger.last_observed_cash = self.tower.cash();
        let closed_quarter =
            quarter_closed_between(previous_absolute_minute, self.tower.clock.absolute_minute());
        if let Some((year, quarter)) = closed_quarter {
            let population_by_kind = self.population_by_kind();
            self.finance_ledger
                .close_quarter(year, quarter, self.tower.cash(), population_by_kind);
            if self.automatic_finance_reports {
                if self.tower.clock.speed != SimulationSpeed::Paused {
                    self.last_running_speed = self.tower.clock.speed;
                    self.finance_resume_speed = Some(self.tower.clock.speed);
                    self.tower.clock.speed = SimulationSpeed::Paused;
                }
                self.finance_panel_open = true;
            }
        }
        self.traffic.advance_after_sync(&self.tower, simulation_dt);
        self.update_fire_event(simulation_dt);
        let current_rating = self.star_rating();
        if current_rating > self.last_star_rating {
            self.promotion_rating = current_rating;
            self.promotion_dialog_open = true;
            self.promotion_resume_speed = (self.tower.clock.speed != SimulationSpeed::Paused)
                .then_some(self.tower.clock.speed);
            self.tower.clock.speed = SimulationSpeed::Paused;
            self.status = format!("Tower promoted to {current_rating} stars");
        }
        self.last_star_rating = current_rating;
        // Visual interpolation uses wall-clock frame time, not accelerated
        // simulation time. At 5x/10x the old code advanced a two-frame walk
        // cycle dozens of times per second and snapped visual positions to
        // every simulation step, which looked like flicker and stutter.
        let visual_dt = visual_animation_delta(dt, self.tower.clock.speed);
        self.people_animation_seconds += f64::from(visual_dt);
        self.simulation_sounds.queue_payments(&income_events);
        self.simulation_sounds.update(
            dt,
            &self.traffic,
            &self.assets.sounds,
            self.tower.clock.speed == SimulationSpeed::Paused,
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
                self.handle_elevator_panel_click(vec2(mouse_position().0, mouse_position().1));
            }
            return;
        }
        self.update_camera();
        self.update_shortcuts();
        if is_mouse_button_pressed(MouseButton::Right) {
            let point = vec2(mouse_position().0, mouse_position().1);
            self.game_menu_open = false;
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

    fn update_fire_event(&mut self, dt: f32) {
        if let Some(event) = &mut self.fire_event {
            if event.response == FireResponse::AwaitingDecision {
                return;
            }
            event.elapsed += dt;
            self.traffic.evacuate_facility(event.facility_id);
            if event.elapsed >= event.duration {
                let facility_id = event.facility_id;
                let saves_unit = event.saves_unit;
                self.fire_event = None;
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
        self.traffic.evacuate_facility(facility_id);
        apply_emergency_speed(&mut self.tower.clock, &mut self.last_running_speed);
        self.speed_menu_open = false;
        self.fire_event = Some(FireEventState {
            facility_id,
            elapsed: 0.0,
            decision_elapsed: 0.0,
            duration: 0.0,
            response: FireResponse::AwaitingDecision,
            saves_unit: false,
            security_start_x: None,
        });
        self.status = format!(
            "Fire reported in tenant #{facility_id}; choose emergency response — speed set to 1x"
        );
    }

    fn update_fire_response_dialog(&mut self, dt: f32) {
        if let Some(event) = &mut self.fire_event {
            event.decision_elapsed =
                (event.decision_elapsed + dt).min(FIRE_RESPONSE_DECISION_SECONDS);
        }
        let mouse = vec2(mouse_position().0, mouse_position().1);
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
                self.fire_event = None;
                return;
            };
            let Some((security_id, response_seconds)) = nearest_security_fire_response(
                target,
                self.tower
                    .facilities()
                    .iter()
                    .filter(|facility| facility.kind == FacilityKind::Security),
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
                .map(|security| {
                    security_team_start_x(target, security, self.tower.floors(), self.tower.width())
                })
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
                self.camera_x =
                    (self.camera_x - horizontal_delta * 8.0).clamp(0.0, maximum_camera_x);
            } else {
                self.camera_floor = (self.camera_floor + wheel.1 * 2.0).clamp(-8.0, 99.0);
            }
        }
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
        let blend = 1.0 - (-10.0 * dt).exp();
        for person in self.traffic.people() {
            let visual_x = self.visual_people_x.entry(person.id).or_insert(person.x);
            *visual_x += (person.x - *visual_x) * blend;
        }
    }

    fn population(&self) -> u32 {
        self.population_by_kind().values().copied().sum()
    }

    fn population_by_kind(&self) -> HashMap<FacilityKind, u32> {
        let reachable = self.traffic.reachable_facility_set();
        let mut totals = HashMap::new();
        for facility in self.tower.facilities().iter().filter(|facility| {
            facility.is_occupied()
                && reachable.contains(&facility.id)
                && self.traffic.has_received_visitor(facility.id)
        }) {
            let population = match facility.kind {
                FacilityKind::Restaurant
                | FacilityKind::FastFood
                | FacilityKind::Shop
                | FacilityKind::Cinema
                | FacilityKind::PartyHall => self
                    .economic_patronage(facility, true)
                    .min(facility.kind.population_capacity()),
                _ => facility.kind.population_capacity(),
            };
            let total = totals.entry(facility.kind).or_insert(0_u32);
            *total = total.saturating_add(population);
        }
        totals
    }

    fn economic_patronage(&self, facility: &Facility, reachable: bool) -> u32 {
        if !facility.is_occupied() || !reachable || !self.traffic.has_received_visitor(facility.id)
        {
            return 0;
        }
        let actual = self
            .traffic
            .daily_visitors_at(facility.id)
            .max(self.traffic.visitors_at(facility.id) as u32);
        let weekday = (self.tower.clock.day.saturating_sub(1) % 7) + 1;
        let weekend = weekday >= 6;
        actual.max(settled_commercial_patronage(facility.kind, weekend))
    }

    fn star_rating(&self) -> u8 {
        self.tower.star_rating_for_population(self.population())
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
        if game_menu_button_rect().contains(point) {
            self.game_menu_open = !self.game_menu_open;
            self.mode_menu_open = false;
            self.open_menu = None;
            self.speed_menu_open = false;
            return;
        }
        if self.game_menu_open {
            let action = game_menu_action_at(point);
            self.game_menu_open = false;
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
            }
            return;
        }
        if mode_menu_button_rect().contains(point) {
            self.mode_menu_open = !self.mode_menu_open;
            self.game_menu_open = false;
            self.open_menu = None;
            self.speed_menu_open = false;
            return;
        }
        if self.mode_menu_open {
            if let Some(mode) = mode_menu_choice_at(point) {
                self.select_view_mode(mode);
                return;
            }
            self.mode_menu_open = false;
            if mode_menu_rect().contains(point) {
                return;
            }
        }
        if self.speed_menu_open
            && let Some(speed) = speed_menu_choice_at(point)
        {
            self.set_simulation_speed(speed);
            self.speed_menu_open = false;
            return;
        }
        if point.y <= 30.0 && point.x >= screen_width() - 360.0 {
            self.finance_panel_open = true;
            self.open_menu = None;
            self.speed_menu_open = false;
            self.status = "Quarterly financial report".to_owned();
            return;
        }
        if sound_menu_button_rect().contains(point) {
            self.game_menu_open = false;
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
        if let Some((menu, index)) = self.menu_entry_at(point) {
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
        if let Some(index) = palette_button_at(point) {
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
        if point.y < TOPBAR_HEIGHT || palette_rect().contains(point) {
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
                    self.traffic.sync_with_tower(&self.tower);
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
                ToolMode::Build(FacilityKind::Lobby) => {
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
                    let adds_car_to_extension = existing_shaft.is_some()
                        && self.tower.is_elevator_extension(kind, placement);
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
                            self.assets.play_construction(kind);
                            self.traffic.sync_with_tower(&self.tower);
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
                                        self.traffic.add_elevator_car(shaft_id, placement.floor)
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
                let selected = self
                    .facility_at_screen_point(point)
                    .map(|facility| (facility.id, facility.kind, facility.position));
                if selected.is_some_and(|(_, kind, position)| {
                    is_elevator_kind(kind) && self.open_elevator_controls_for(kind, position.x)
                }) {
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
                        self.traffic.sync_with_tower(&self.tower);
                        self.assets.play_demolition();
                        self.status = format!("Demolished {} #{}", facility.kind.spec().name, id)
                    }
                    Err(error) => self.status = error.to_string(),
                },
                Some(DemolitionTarget::Floor(position)) => {
                    match self.tower.demolish_floor(position) {
                        Ok(()) => {
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
            self.assets.play_construction(FacilityKind::Lobby);
            self.status = format!(
                "Painted {built} {}-story lobby slice{} across the full buildable length of floor {}",
                stories,
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
        self.traffic.sync_with_tower(&self.tower);
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
                        "Built lobby slice on floor {}, column {} — keep dragging to extend",
                        position.floor, position.x
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
            self.status = match self.tool {
                ToolMode::Floor => format!(
                    "Extended empty floor by {built} slice{} on level {}",
                    if built == 1 { "" } else { "s" },
                    current.floor
                ),
                ToolMode::Build(FacilityKind::Lobby) => format!(
                    "Painted {built} {}-story lobby slice{} on floor {}",
                    self.lobby_drag_stories,
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
            self.traffic.sync_with_tower(&self.tower);
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
        if let Err(error) = self.tower.purchase_elevator_car(kind) {
            self.report_placement_error(error);
            return;
        }
        if self.traffic.add_elevator_car(shaft_id, position.floor) {
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
        self.status = if is_elevator_kind(kind) {
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
                let evaluation = match facility_crowd_level(
                    kind,
                    occupied,
                    visitors,
                    facility_is_open(kind, self.tower.clock.minute_of_day),
                ) {
                    FacilityCrowd::Closed => "closed",
                    FacilityCrowd::Empty => "needs attention",
                    FacilityCrowd::Light => "average",
                    FacilityCrowd::Heavy => "excellent",
                };
                self.status = format!(
                    "{} #{}: {} — {} people currently inside",
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
            let source_x = (f32::from(facility.position.x - run_start) * CELL_WIDTH)
                .rem_euclid(LOBBY_BODY_WIDTH);
            let sprite = &self.assets.facilities.lobby[story];
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
            return sprite_pixel_is_opaque(
                sprite,
                source,
                Rect::new(x, base_y - FLOOR_HEIGHT, width, FLOOR_HEIGHT * 2.0),
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
        let upper_ordinal = base_ordinal + 1;
        let center_x = f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5;
        let occupants = self
            .traffic
            .people()
            .iter()
            .filter(|person| {
                person.activity == PersonActivity::UsingStairs
                    && (person.x - center_x).abs() < 0.1
                    && person.floor_position >= f32::from(base_ordinal) - 0.01
                    && person.floor_position <= f32::from(upper_ordinal) + 0.01
            })
            .count();
        Rect::new(
            occupants.min(6) as f32 * f32::from(facility.kind.spec().width) * CELL_WIDTH,
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
        let weekend = self.tower.clock.day.is_multiple_of(3);
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
        self.save_name_dialog.is_some()
            || self.elevator_panel.is_some()
            || self.promotion_dialog_open
            || self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            || point.y < TOPBAR_HEIGHT
            || palette_rect().contains(point)
            || (self.game_menu_open && game_menu_rect().contains(point))
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
            && !self.finance_panel_open
            && !self.promotion_dialog_open
            && !self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            && self.intro_remaining <= 0.0
            && !self.point_is_ui(mouse);
        let inspect_cursor = self.tool == ToolMode::Inspect
            && self.view_mode == ViewMode::Edit
            && self.elevator_panel.is_none()
            && !self.finance_panel_open
            && !self.promotion_dialog_open
            && !self
                .fire_event
                .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
            && self.intro_remaining <= 0.0
            && !self.point_is_ui(mouse);
        let resize_handle =
            (!self.promotion_dialog_open && !demolition_cursor && !self.point_is_ui(mouse))
                .then(|| self.elevator_extension_at(mouse))
                .flatten();
        window::show_mouse(resize_handle.is_none() && !demolition_cursor && !inspect_cursor);
        self.draw_world_background();
        self.draw_world();
        self.draw_world_lighting();
        self.draw_topbar();
        self.draw_palette();
        if let Some(menu) = self.open_menu {
            self.draw_submenu(menu);
        }
        if self.speed_menu_open {
            self.draw_speed_menu();
        }
        if self.game_menu_open {
            self.draw_game_menu();
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
        self.draw_ui_tooltip(mouse);
        if demolition_cursor {
            self.draw_demolition_cursor(mouse);
        } else if let Some((_, _, _, direction)) = resize_handle {
            self.draw_elevator_resize_cursor(mouse, direction);
        } else if inspect_cursor {
            self.draw_magnifier_cursor(mouse);
        }
        if self.intro_remaining > 0.0 {
            self.draw_intro();
        }
        if self.save_name_dialog.is_some() {
            self.draw_save_name_dialog();
        }
        if self.promotion_dialog_open {
            self.draw_promotion_dialog();
        }
        if self
            .fire_event
            .is_some_and(|event| event.response == FireResponse::AwaitingDecision)
        {
            self.draw_fire_response_dialog();
        }
    }

    fn draw_intro(&self) {
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), BLACK);
        if self.intro_remaining > 1.0 {
            let scale = (screen_width() / 640.0).min(screen_height() / 480.0);
            let size = vec2(640.0 * scale, 480.0 * scale);
            draw_texture_ex(
                &self.assets.intro_store,
                (screen_width() - size.x) * 0.5,
                (screen_height() - size.y) * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        } else {
            draw_texture_ex(
                &self.assets.intro_title,
                screen_width() * 0.5 - 167.0,
                screen_height() * 0.5 - 150.0,
                WHITE,
                DrawTextureParams::default(),
            );
            draw_texture_ex(
                &self.assets.intro_maxis,
                screen_width() * 0.5 - 118.0,
                screen_height() * 0.5 + 44.0,
                WHITE,
                DrawTextureParams::default(),
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
        for position in self
            .tower
            .floors()
            .iter()
            .filter(|position| self.floor_cell_is_visible(**position))
        {
            self.draw_empty_floor(*position);
        }
        self.draw_floor_end_stairs();
        for facility in self.tower.facilities().iter().filter(|facility| {
            !is_vertical_transport(facility.kind) && self.facility_is_visible(facility)
        }) {
            self.draw_facility(facility);
        }
        // Entrance facade and exterior awnings belong to the lobby layer.
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
        self.draw_roof_crane();
        self.draw_people();
        self.draw_fire_response();
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

    fn draw_security_response_team(&self, event: FireEventState, target: &Facility) {
        let target_center =
            f32::from(target.position.x) + f32::from(target.kind.spec().width) * 0.5;
        let start_x = event.security_start_x.unwrap_or(target_center);
        let direction = if start_x <= target_center { 1.0 } else { -1.0 };
        // The responders work from the corridor at the tenant boundary. If
        // they enter the unit center they overlap the flame frames and read as
        // corrupted pixels instead of a separate team.
        let destination_x = if direction > 0.0 {
            f32::from(target.position.x) - 0.5
        } else {
            f32::from(target.end_x()) + 0.5
        };
        let travel_seconds = (event.duration - 6.0).max(0.1);
        let progress = (event.elapsed / travel_seconds).clamp(0.0, 1.0);
        let team_world_x = start_x + (destination_x - start_x) * progress;
        let screen_x = (team_world_x - self.camera_x) * CELL_WIDTH;
        let y = tenant_room_top_y(self.ground_y(), target.position.floor);
        let arrived = progress >= 1.0;
        for member in 0..3 {
            let trailing_offset = member as f32 * 7.0 * -direction;
            // The first pair is the equipment-carrying walk cycle. At the
            // scene, freeze on the original clean working pose; alternating
            // substantially different poses while stationary caused flashing.
            for (frame, weight) in security_team_animation_frames(event.elapsed, arrived, member) {
                if weight > 0.01 {
                    self.draw_person_frame(
                        2,
                        frame,
                        screen_x - 4.0 + trailing_offset,
                        y,
                        with_alpha(WHITE, weight),
                    );
                }
            }
        }
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
            &format!("Security dispatches automatically in {remaining} seconds."),
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
        draw_texture_ex(
            &self.assets.star_award,
            rect.x + 17.0,
            rect.y + 20.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(110.0, 110.0)),
                ..Default::default()
            },
        );
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

    fn draw_empty_floor(&self, position: GridPosition) {
        let x = (f32::from(position.x) - self.camera_x) * CELL_WIDTH;
        let room_y = tenant_room_top_y(self.ground_y(), position.floor);
        draw_rectangle(
            x,
            room_y,
            CELL_WIDTH,
            ROOM_HEIGHT,
            Color::from_rgba(55, 56, 57, 255),
        );
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
        let top = bottom - f32::from(facility.height()) * FLOOR_HEIGHT;
        x + width >= 0.0
            && x <= screen_width()
            && bottom >= TOPBAR_HEIGHT - FLOOR_HEIGHT
            && top <= screen_height() + FLOOR_HEIGHT
    }

    fn draw_floor_end_stairs(&self) {
        const STAIR_HALF_WIDTH: f32 = 24.0;
        let floor_cells = self.tower.floors().iter().copied().collect::<HashSet<_>>();
        for start in self.tower.floors() {
            if !self.floor_cell_is_visible(*start)
                || (start.x > 0
                    && floor_cells.contains(&GridPosition {
                        x: start.x - 1,
                        floor: start.floor,
                    }))
            {
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

            let x = (f32::from(start.x) - self.camera_x) * CELL_WIDTH;
            let y = floor_top_y(self.ground_y(), start.floor);
            for (destination_x, source_x) in [
                (x - STAIR_HALF_WIDTH, 0.0),
                (
                    (f32::from(end_x) - self.camera_x) * CELL_WIDTH,
                    STAIR_HALF_WIDTH,
                ),
            ] {
                draw_texture_ex(
                    &self.assets.emergency_stairs,
                    destination_x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(STAIR_HALF_WIDTH, FLOOR_HEIGHT)),
                        source: Some(Rect::new(source_x, 0.0, STAIR_HALF_WIDTH, FLOOR_HEIGHT)),
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
            for story in 0..facility.height() {
                self.draw_scaffolding(x, y - f32::from(story) * FLOOR_HEIGHT, width);
            }
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
            self.draw_empty_elevator_shaft(x, y, width, facility.position.floor);
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
            if facility.is_burned() {
                Color::new(0.28, 0.24, 0.22, 1.0)
            } else {
                facility_light_tint(facility.kind, crowd)
            },
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
            facility_is_open(facility.kind, self.tower.clock.minute_of_day),
        )
    }

    fn facility_visual_load(&self, facility: &Facility) -> usize {
        match facility.kind {
            FacilityKind::Parking => {
                let parking_count = self
                    .tower
                    .facilities()
                    .iter()
                    .filter(|other| other.kind == FacilityKind::Parking)
                    .count()
                    .max(1);
                self.traffic.people().len().div_ceil(parking_count).min(14)
            }
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
            FacilityKind::Cathedral => usize::from(
                self.tower.clock.day.is_multiple_of(7)
                    && (10 * 60..14 * 60).contains(&self.tower.clock.minute_of_day),
            ),
            _ => self.traffic.visitors_at(facility.id),
        }
    }

    fn draw_facility_state_overlay(&self, facility: &Facility, x: f32, base_y: f32, width: f32) {
        if facility.is_burned() {
            let room_y = tenant_room_top_y(self.ground_y(), facility.position.floor);
            draw_rectangle(
                x,
                room_y,
                width,
                ROOM_HEIGHT,
                Color::new(0.08, 0.06, 0.05, 0.48),
            );
            let scorch_count = (width / 24.0).ceil() as usize;
            for scorch in 0..scorch_count {
                let center_x = x + 10.0 + scorch as f32 * 24.0;
                draw_circle(
                    center_x.min(x + width - 5.0),
                    room_y + 15.0 + (scorch % 2) as f32 * 3.0,
                    5.0 + (scorch % 3) as f32,
                    Color::new(0.03, 0.025, 0.02, 0.7),
                );
            }
        }

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

    fn draw_empty_elevator_shaft(&self, x: f32, y: f32, width: f32, floor: i16) {
        let tile_width = self.assets.elevator_shaft.width();
        let mut offset = 0.0;
        while offset < width {
            let draw_width = (width - offset).min(tile_width);
            draw_texture_ex(
                &self.assets.elevator_shaft,
                x + offset,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(draw_width, FLOOR_HEIGHT)),
                    source: Some(Rect::new(0.0, 0.0, draw_width, FLOOR_HEIGHT)),
                    ..Default::default()
                },
            );
            offset += draw_width;
        }

        // Resources 2024-2026 are the original light-gray shaft floor
        // designations. Cars are drawn later and naturally cover the label.
        self.draw_elevator_floor_number(floor, x, y, width);
    }

    fn draw_elevator_floor_number(&self, floor: i16, x: f32, y: f32, width: f32) {
        if (1..=9).contains(&floor) || floor == 100 {
            let frame = if floor == 100 { 10 } else { floor as usize };
            draw_texture_ex(
                &self.assets.elevator_numbers[0],
                x + (width - 32.0) * 0.5,
                y,
                WHITE,
                DrawTextureParams {
                    source: Some(Rect::new(frame as f32 * 32.0, 0.0, 32.0, 36.0)),
                    dest_size: Some(vec2(32.0, 36.0)),
                    ..Default::default()
                },
            );
            return;
        }

        let label = if floor < 0 {
            format!("B{}", floor.unsigned_abs())
        } else {
            floor.to_string()
        };
        let glyph_width = 16.0;
        let total_width = glyph_width * label.len() as f32;
        let mut draw_x = x + (width - total_width) * 0.5;
        for character in label.chars() {
            let (texture, source_x) = if character == 'B' {
                (&self.assets.elevator_numbers[2], 0.0)
            } else {
                (
                    &self.assets.elevator_numbers[1],
                    character.to_digit(10).unwrap_or(0) as f32 * glyph_width,
                )
            };
            draw_texture_ex(
                texture,
                draw_x,
                y,
                WHITE,
                DrawTextureParams {
                    source: Some(Rect::new(source_x, 0.0, glyph_width, 36.0)),
                    dest_size: Some(vec2(glyph_width, 36.0)),
                    ..Default::default()
                },
            );
            draw_x += glyph_width;
        }
    }

    fn draw_stair_transport(
        &self,
        facility: &Facility,
        x: f32,
        base_y: f32,
        width: f32,
        sprite: &Sprite,
    ) {
        draw_texture_ex(
            &sprite.texture,
            x,
            base_y - FLOOR_HEIGHT,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(width, FLOOR_HEIGHT * 2.0)),
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
        let source_x = (f32::from(column - run_start) * CELL_WIDTH).rem_euclid(LOBBY_BODY_WIDTH);
        for story in 0..facility.stories {
            let sprite = &self.assets.facilities.lobby[usize::from(story)];
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
        // Resources 2536-2538 are the matching ground, second, and third
        // original super-lobby layers. Each carries its own left facade.
        for story in 0..facility.stories {
            let lobby = &self.assets.facilities.lobby[usize::from(story)];
            draw_texture_ex(
                &lobby.texture,
                x,
                y - f32::from(story) * FLOOR_HEIGHT,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(facade_width, FLOOR_HEIGHT)),
                    source: Some(Rect::new(LOBBY_FACADE_X, 0.0, facade_width, FLOOR_HEIGHT)),
                    ..Default::default()
                },
            );
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
                    Some((
                        lobby_fill_plan(&self.tower, position.floor, stories),
                        stories,
                    ))
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
        let (width, height, valid) = match self.tool {
            ToolMode::Floor => (
                CELL_WIDTH,
                FLOOR_HEIGHT,
                self.tower.can_place_floor(position).is_ok(),
            ),
            ToolMode::Build(FacilityKind::Lobby) => {
                let stories = lobby_stories_for_position(&self.tower, position);
                let mut preview = self.tower.clone();
                (
                    CELL_WIDTH,
                    f32::from(stories) * FLOOR_HEIGHT,
                    preview.place_lobby(position, stories).is_ok(),
                )
            }
            ToolMode::Build(kind) => (
                f32::from(kind.spec().width) * CELL_WIDTH,
                f32::from(kind.spec().height) * FLOOR_HEIGHT,
                self.tower.can_place(kind, position).is_ok(),
            ),
            ToolMode::Inspect | ToolMode::Demolish => unreachable!(),
        };
        let x = (f32::from(position.x) - self.camera_x) * CELL_WIDTH;
        let y = floor_top_y(self.ground_y(), position.floor);
        let draw_y = y + FLOOR_HEIGHT - height;
        let color = if valid { GREEN } else { RED };
        draw_rectangle_lines(x, draw_y, width, height, 3.0, color);
    }

    fn draw_demolition_cursor(&self, mouse: Vec2) {
        const RADIUS: f32 = 9.0;
        // The intersection is the hotspot: demolition hit-testing uses this
        // exact mouse pixel rather than the surrounding grid cell.
        for thickness in [5.0, 2.0] {
            let color = if thickness > 2.0 { BLACK } else { RED };
            draw_line(
                mouse.x - RADIUS,
                mouse.y - RADIUS,
                mouse.x + RADIUS,
                mouse.y + RADIUS,
                thickness,
                color,
            );
            draw_line(
                mouse.x + RADIUS,
                mouse.y - RADIUS,
                mouse.x - RADIUS,
                mouse.y + RADIUS,
                thickness,
                color,
            );
        }
        draw_circle(mouse.x, mouse.y, 1.25, WHITE);
    }

    fn draw_magnifier_cursor(&self, mouse: Vec2) {
        const CURSOR_WIDTH: f32 = 22.0;
        const CURSOR_HEIGHT: f32 = 21.0;
        const LENS_CENTER: Vec2 = Vec2::new(7.0, 7.0);
        // Hit-testing uses `mouse` itself, so align the center of the glass
        // with that exact pixel rather than using the bitmap's upper-left.
        let top_left = mouse - LENS_CENTER;
        draw_texture_ex(
            &self.assets.magnifier_cursor,
            top_left.x,
            top_left.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(CURSOR_WIDTH, CURSOR_HEIGHT)),
                ..Default::default()
            },
        );
    }

    fn draw_elevator_resize_cursor(&self, mouse: Vec2, direction: i8) {
        const CURSOR_SIZE: f32 = 25.0;
        // The mouse hotspot is the fingertip, matching the original cursor.
        // Keeping the image centered on the mouse put the clickable point half
        // a floor away from where the hand appeared to point.
        let top_left = if direction > 0 {
            mouse - vec2(CURSOR_SIZE * 0.5, 0.0)
        } else {
            mouse - vec2(CURSOR_SIZE * 0.5, CURSOR_SIZE)
        };
        draw_texture_ex(
            &self.assets.elevator_cursor,
            top_left.x,
            top_left.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(CURSOR_SIZE, CURSOR_SIZE)),
                flip_y: direction < 0,
                ..Default::default()
            },
        );
    }

    fn draw_people(&self) {
        let destination_kinds = self
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
            if matches!(person.activity, PersonActivity::WaitingForElevator { .. }) {
                self.draw_queue_person(person.mood, person.id, x, y);
                continue;
            }
            if self.view_mode != ViewMode::Edit {
                self.draw_silhouette_person(person.id, x, y);
                continue;
            }
            let destination_kind = destination_kinds
                .get(&person.destination_facility_id)
                .copied();
            let sheet = person_sprite_sheet(destination_kind, person.id);
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
        for facility in self
            .tower
            .facilities()
            .iter()
            .filter(|facility| self.facility_is_visible(facility))
        {
            let color = match self.view_mode {
                ViewMode::Edit => continue,
                ViewMode::Evaluation => match self.facility_crowd(facility) {
                    FacilityCrowd::Closed => Color::from_rgba(110, 115, 120, 255),
                    FacilityCrowd::Empty => Color::from_rgba(220, 45, 45, 255),
                    FacilityCrowd::Light => Color::from_rgba(235, 196, 35, 255),
                    FacilityCrowd::Heavy => Color::from_rgba(45, 105, 225, 255),
                },
                ViewMode::Pricing => {
                    if !facility.kind.has_tenant_occupancy() {
                        continue;
                    }
                    match facility.price_level {
                        0 => Color::from_rgba(45, 105, 225, 255),
                        1 => Color::from_rgba(235, 196, 35, 255),
                        2 => Color::from_rgba(235, 125, 35, 255),
                        _ => Color::from_rgba(220, 45, 45, 255),
                    }
                }
                ViewMode::Hotel => {
                    if !is_hotel_room(facility.kind) {
                        continue;
                    }
                    if facility.is_occupied() {
                        Color::from_rgba(45, 105, 225, 255)
                    } else {
                        Color::from_rgba(220, 45, 45, 255)
                    }
                }
            };
            let width = f32::from(facility.kind.spec().width) * CELL_WIDTH;
            let height = f32::from(facility.kind.spec().height) * FLOOR_HEIGHT;
            let x = (f32::from(facility.position.x) - self.camera_x) * CELL_WIDTH;
            let y = floor_top_y(self.ground_y(), facility.position.floor) - height + FLOOR_HEIGHT;
            draw_rectangle(x, y, width, height, with_alpha(color, 0.16));
            draw_rectangle_lines(x, y, width, height, 2.0, color);
        }
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
        let sheet = usize::from(id % 11 == 0);
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
        for shaft in self
            .traffic
            .elevators()
            .iter()
            .filter(|shaft| shaft.visible)
        {
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
            for (car_index, car) in shaft.cars.iter().enumerate() {
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
                let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH + car_index as f32 * 2.0;
                let y = floor_position_top_y(self.ground_y(), car.floor_position);
                draw_texture_ex(
                    car_texture,
                    x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(frame_width, FLOOR_HEIGHT)),
                        source: Some(source),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_elevator_endcaps(&self) {
        for shaft in self
            .traffic
            .elevators()
            .iter()
            .filter(|shaft| shaft.visible && !shaft.served_floors.is_empty())
        {
            let top = *shaft.served_floors.last().expect("checked above");
            let bottom = *shaft.served_floors.first().expect("checked above");
            let x = (f32::from(shaft.x) - self.camera_x) * CELL_WIDTH;
            let width = f32::from(shaft.kind.spec().width) * CELL_WIDTH;
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
                    WHITE,
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
            screen_width(),
            screen_height() - TOPBAR_HEIGHT,
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
            screen_width(),
            screen_height() - TOPBAR_HEIGHT,
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
        draw_rectangle(0.0, 0.0, screen_width(), TOPBAR_HEIGHT, CLASSIC_FACE);
        draw_line(
            0.0,
            TOPBAR_HEIGHT - 1.0,
            screen_width(),
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
        for star in 0..5 {
            let rect = rating_star_rect(star);
            draw_texture_ex(
                if star < usize::from(rating) {
                    &self.assets.star_on
                } else {
                    &self.assets.star_off
                },
                rect.x,
                rect.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(rect.size()),
                    ..Default::default()
                },
            );
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
        let time_x = topbar_clock_x(screen_width(), time_width, controls_right);
        draw_text(&time, time_x, 23.0, 17.0, BLACK);
        let money = format!("Fund ${}    Pop {}", self.tower.cash(), self.population());
        let money_width = measure_text(&money, None, 17, 1.0).width;
        let money_x = (screen_width() - money_width - 12.0).max(time_x + time_width + 18.0);
        draw_text(&money, money_x, 23.0, 17.0, BLACK);
        draw_rectangle(
            6.0,
            35.0,
            screen_width() - 12.0,
            22.0,
            Color::from_rgba(235, 235, 235, 255),
        );
        draw_rectangle_lines(6.0, 35.0, screen_width() - 12.0, 22.0, 1.0, CLASSIC_SHADOW);
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

        let mouse = vec2(mouse_position().0, mouse_position().1);
        if let Some(star) = (0..5).find(|star| rating_star_rect(*star).contains(mouse)) {
            self.draw_star_tooltip(star + 1, rating_star_rect(star).x, 31.0);
        }
    }

    fn draw_game_menu(&self) {
        let menu = game_menu_rect();
        draw_classic_panel(menu);
        let mouse = vec2(mouse_position().0, mouse_position().1);
        for (index, (action, label)) in GAME_MENU_ACTIONS.iter().enumerate() {
            let row = game_menu_entry_rect(index);
            let hovered = row.contains(mouse);
            if hovered {
                draw_rectangle(row.x, row.y, row.w, row.h, TITLE_BLUE);
            }
            if *action == GameMenuAction::AutomaticReports && self.automatic_finance_reports {
                draw_menu_checkmark(row, if hovered { WHITE } else { BLACK });
            }
            draw_text(
                label,
                row.x
                    + if *action == GameMenuAction::AutomaticReports {
                        25.0
                    } else {
                        8.0
                    },
                row.y + 17.0,
                14.0,
                if hovered { WHITE } else { BLACK },
            );
        }
    }

    fn draw_mode_menu(&self) {
        let menu = mode_menu_rect();
        draw_classic_panel(menu);
        let mouse = vec2(mouse_position().0, mouse_position().1);
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
            screen_width(),
            screen_height() - TOPBAR_HEIGHT,
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
                &format!("{filename} already exists."),
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
        draw_text(visible_name, input.x + 6.0, input.y + 19.0, 16.0, BLACK);
        let caret_x = input.x
            + 6.0
            + measure_text(visible_name, None, 16, 1.0)
                .width
                .min(max_width);
        draw_line(caret_x, input.y + 5.0, caret_x, input.y + 22.0, 1.0, BLACK);
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
        let x = x.min(screen_width() - width - 4.0).max(4.0);
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
                    let texture = match (paused, selected) {
                        (true, true) => &self.assets.play_selected,
                        (true, false) => &self.assets.play,
                        (false, true) => &self.assets.pause_selected,
                        (false, false) => &self.assets.pause,
                    };
                    draw_cropped_icon(
                        texture,
                        Rect::new(16.0, 0.0, 32.0, 32.0),
                        button_rect,
                        WHITE,
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
                        Rect::new(f32::from(index) * icon_width, 0.0, icon_width, 21.0),
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
        let mouse = vec2(mouse_position().0, mouse_position().1);
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
            draw_atlas_icon(
                icon_palette,
                entry.atlas_column,
                entry.atlas_row,
                Rect::new(row.x + 2.0, row.y + 2.0, 24.0, 24.0),
                WHITE,
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
            draw_text(
                speed_label(speed),
                row.x + 25.0,
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
        let x = x.min(screen_width() - width - 4.0).max(4.0);
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

const fn settled_commercial_patronage(kind: FacilityKind, weekend: bool) -> u32 {
    match kind {
        FacilityKind::FastFood => {
            if weekend {
                48
            } else {
                35
            }
        }
        FacilityKind::Shop => {
            if weekend {
                30
            } else {
                25
            }
        }
        FacilityKind::Restaurant => 35,
        FacilityKind::Cinema => 120,
        FacilityKind::PartyHall => 50,
        _ => 0,
    }
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
        || is_key_down(KeyCode::LeftSuper)
        || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    if control && shift {
        3
    } else if control {
        2
    } else {
        1
    }
}

fn seasonal_event_for_date(date: &str) -> Option<SeasonalEventKind> {
    match date.trim() {
        "12-25" => Some(SeasonalEventKind::Santa),
        "10-31" => Some(SeasonalEventKind::Witch),
        _ => None,
    }
}

fn lobby_stories_for_position(tower: &Tower, position: GridPosition) -> u8 {
    let is_first_lobby = !tower
        .facilities()
        .iter()
        .any(|facility| facility.kind == FacilityKind::Lobby);
    if is_first_lobby && position.floor == 1 {
        requested_lobby_stories()
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
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", "(Get-Date).ToString('MM-dd')"])
        .output()
        .ok()?;
    #[cfg(not(target_os = "windows"))]
    let output = Command::new("date").arg("+%m-%d").output().ok()?;
    seasonal_event_for_date(&String::from_utf8_lossy(&output.stdout))
}

const fn weather_is_raining(day: u32) -> bool {
    day % 5 == 0 || day % 7 == 3
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

fn hotel_sprite_frame(minute_of_day: u16, visitors: usize) -> usize {
    // The first pair is a room in use, the second is a clean room waiting
    // for its next guest. Later pairs are dirty/infested states, which are
    // reserved until housekeeping simulation supplies those conditions.
    usize::from(visitors == 0) * 2 + usize::from(night_sprite_active(minute_of_day))
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
        84.0,
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
        158.0,
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

fn save_name_dialog_rect() -> Rect {
    Rect::new(
        screen_width() * 0.5 - 190.0,
        screen_height() * 0.5 - 80.0,
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
    let output = Command::new("powershell.exe")
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
    if value < 0 {
        format!("-${}", format_number(value.saturating_abs()))
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

fn draw_menu_checkmark(row: Rect, color: Color) {
    let x = row.x + 7.0;
    let y = row.y + 12.0;
    draw_line(x, y, x + 4.0, y + 4.0, 2.0, color);
    draw_line(x + 4.0, y + 4.0, x + 12.0, y - 5.0, 2.0, color);
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

fn elevator_panel_rect() -> Rect {
    let width = 280.0;
    let height = 465.0_f32.min(screen_height() - TOPBAR_HEIGHT - 20.0);
    Rect::new(
        (screen_width() - width) * 0.5,
        TOPBAR_HEIGHT + (screen_height() - TOPBAR_HEIGHT - height) * 0.5,
        width,
        height,
    )
}

fn finance_panel_rect() -> Rect {
    let width = 343.0;
    let height = 364.0;
    Rect::new(
        (screen_width() - width) * 0.5,
        ((screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
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
        (screen_width() - width) * 0.5,
        ((screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
        width,
        height,
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

fn fire_response_dialog_rect() -> Rect {
    let width = 540.0;
    let height = 210.0;
    Rect::new(
        (screen_width() - width) * 0.5,
        ((screen_height() - height) * 0.5).max(TOPBAR_HEIGHT + 8.0),
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

const fn promotion_rating_label(star_rating: u8) -> &'static str {
    match star_rating {
        2 => "Two Star Rating!",
        3 => "Three Star Rating!",
        4 => "Four Star Rating!",
        5 => "Five Star Rating!",
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

fn with_alpha(mut color: Color, alpha: f32) -> Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
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
    draw_texture_ex(
        texture,
        destination.x + 3.0,
        destination.y + 3.0,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(destination.w - 6.0, destination.h - 6.0)),
            source: Some(Rect::new(
                f32::from(column) * 32.0,
                f32::from(row) * 32.0,
                32.0,
                32.0,
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
    play: Texture2D,
    play_selected: Texture2D,
    pause: Texture2D,
    pause_selected: Texture2D,
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
    star_on: Texture2D,
    star_off: Texture2D,
    _tower_logo: Texture2D,
    finance_dialog: Texture2D,
    fire_large: [Texture2D; 4],
    fire_small: Texture2D,
    fire_helicopter: Texture2D,
    fire_alert: Texture2D,
    _fire_aftermath: Texture2D,
    star_award: Texture2D,
    _terrorist_portrait: Texture2D,
    fire_dispatch: Texture2D,
    _vip_arrival: Texture2D,
    recycling_fill: [Texture2D; 5],
    recycling_truck: Texture2D,
    facilities: FacilitySprites,
    sounds: NativeSounds,
}

struct FacilitySprites {
    lobby: [Sprite; 3],
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

impl OriginalAssets {
    fn load() -> Result<Self, String> {
        Ok(Self {
            sky: texture_from_bmp(SKY_BMP)?,
            ground: texture_from_bmp(GROUND_BMP)?,
            intro_store: texture_from_bmp(INTRO_STORE_BMP)?,
            intro_title: texture_from_bmp_with_white_transparency(INTRO_TITLE_BMP)?,
            intro_maxis: texture_from_bmp_with_white_transparency(INTRO_MAXIS_BMP)?,
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
            tool_palette: texture_from_bmp(PALETTE_BMP)?,
            tool_palette_selected: texture_from_bmp(PALETTE_SELECTED_BMP)?,
            tool_palette_disabled: texture_from_bmp(PALETTE_DISABLED_BMP)?,
            play: texture_from_bmp(PLAY_BMP)?,
            play_selected: texture_from_bmp(PLAY_SELECTED_BMP)?,
            pause: texture_from_bmp(PAUSE_BMP)?,
            pause_selected: texture_from_bmp(PAUSE_SELECTED_BMP)?,
            pointer_tools: texture_from_bmp(POINTER_TOOLS_BMP)?,
            pointer_tools_selected: texture_from_bmp(POINTER_TOOLS_SELECTED_BMP)?,
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
            elevator_numbers: ELEVATOR_NUMBER_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "elevator number texture count mismatch".to_owned())?,
            star_on: texture_from_star_bmp(STAR_ON_BMP, true)?,
            star_off: texture_from_star_bmp(STAR_OFF_BMP, false)?,
            _tower_logo: texture_from_bmp_with_white_transparency(TOWER_LOGO_BMP)?,
            finance_dialog: texture_from_bmp(FINANCE_DIALOG_BMP)?,
            fire_large: FIRE_LARGE_BMPS
                .map(texture_from_bmp_with_white_transparency)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "fire texture count mismatch".to_owned())?,
            fire_small: texture_from_bmp_with_white_transparency(FIRE_SMALL_BMP)?,
            fire_helicopter: texture_from_bmp_with_white_transparency(FIRE_HELICOPTER_BMP)?,
            fire_alert: texture_from_bmp_with_white_transparency(FIRE_ALERT_BMP)?,
            _fire_aftermath: texture_from_bmp_with_white_transparency(FIRE_AFTERMATH_BMP)?,
            star_award: texture_from_bmp_with_white_transparency(STAR_AWARD_BMP)?,
            _terrorist_portrait: texture_from_bmp_with_white_transparency(TERRORIST_PORTRAIT_BMP)?,
            fire_dispatch: texture_from_bmp_with_white_transparency(FIRE_DISPATCH_BMP)?,
            _vip_arrival: texture_from_bmp_with_white_transparency(VIP_ARRIVAL_BMP)?,
            recycling_fill: [
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[1])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[2])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[3])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[4])?,
                texture_from_bmp_with_white_transparency(RECYCLING_LEVEL_BMPS[5])?,
            ],
            recycling_truck: texture_from_bmp_with_white_transparency(RECYCLING_TRUCK_BMP)?,
            facilities: FacilitySprites {
                lobby: [
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2536,
                        LOBBY_BACKGROUND_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2537,
                        LOBBY_SECOND_STORY_BMP,
                    )?,
                    facility_sprite_from_bmp_with_id(
                        FacilityKind::Lobby,
                        2538,
                        LOBBY_THIRD_STORY_BMP,
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
                    false,
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
            FacilityKind::Lobby => &self.facilities.lobby[0],
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
            if !facility.is_occupied() {
                let sprite = &vacant[style];
                return (sprite, sprite.source);
            }
            let sprite = &occupied[style];
            let mut source = sprite.source;
            let frame = hotel_sprite_frame(minute_of_day, visual_load);
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
            let sprite = &self.facilities.parking[usize::from(visual_load > 0)];
            let mut source = sprite.source;
            if visual_load > 0 {
                source.x = ((facility.id as usize + visual_load) % 14) as f32 * source.w;
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
    construction: PathBuf,
    lobby_segment: PathBuf,
    demolition: PathBuf,
    no_money: PathBuf,
    payment: PathBuf,
    fire_response: PathBuf,
    elevator_move: PathBuf,
    elevator_open: [PathBuf; 2],
    crowd: PathBuf,
    muted: Cell<bool>,
}

impl NativeSounds {
    fn install() -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("opentower-audio-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let construction = directory.join(format!("construction-{CONSTRUCTION_SOUND_ID}.wav"));
        let lobby_segment = directory.join(format!("lobby-segment-{LOBBY_SEGMENT_SOUND_ID}.wav"));
        let demolition = directory.join(format!("demolition-{DEMOLITION_SOUND_ID}.wav"));
        let no_money = directory.join(format!("no-money-{NO_MONEY_SOUND_ID}.wav"));
        let payment = directory.join(format!("payment-{PAYMENT_SOUND_ID}.wav"));
        let fire_response = directory.join(format!("fire-response-{FIRE_RESPONSE_SOUND_ID}.wav"));
        let elevator_move = directory.join(format!("elevator-move-{ELEVATOR_MOVE_SOUND_ID}.wav"));
        let elevator_open =
            ELEVATOR_OPEN_SOUND_IDS.map(|id| directory.join(format!("elevator-open-{id}.wav")));
        let crowd = directory.join(format!("crowd-{CROWD_SOUND_ID}.wav"));
        std::fs::write(&construction, CONSTRUCTION_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&lobby_segment, LOBBY_SEGMENT_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&demolition, DEMOLITION_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&no_money, NO_MONEY_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&payment, PAYMENT_WAV.bytes()?).map_err(|error| error.to_string())?;
        std::fs::write(&fire_response, FIRE_RESPONSE_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&elevator_move, ELEVATOR_MOVE_WAV.bytes()?)
            .map_err(|error| error.to_string())?;
        for (path, bytes) in elevator_open
            .iter()
            .zip([ELEVATOR_OPEN_1_WAV, ELEVATOR_OPEN_2_WAV])
        {
            std::fs::write(path, bytes.bytes()?).map_err(|error| error.to_string())?;
        }
        std::fs::write(&crowd, CROWD_1_WAV.bytes()?).map_err(|error| error.to_string())?;
        Ok(Self {
            construction,
            lobby_segment,
            demolition,
            no_money,
            payment,
            fire_response,
            elevator_move,
            elevator_open,
            crowd,
            muted: Cell::new(false),
        })
    }

    fn is_muted(&self) -> bool {
        self.muted.get()
    }

    fn toggle_muted(&self) -> bool {
        let muted = !self.muted.get();
        self.muted.set(muted);
        muted
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

    fn play_fire_response(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.fire_response.clone());
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

    fn play_crowd(&self) {
        if self.is_muted() {
            return;
        }
        play_native_sound_async(self.crowd.clone());
    }
}

fn play_native_sound_async(path: PathBuf) {
    let _ = std::thread::spawn(move || play_native_sound(path));
}

#[cfg(target_os = "macos")]
fn play_native_sound(path: PathBuf) {
    if let Ok(mut child) = Command::new("afplay").arg(path).spawn() {
        let _ = child.wait();
    }
}

#[cfg(target_os = "windows")]
fn play_native_sound(path: PathBuf) {
    let escaped = path.to_string_lossy().replace('\'', "''");
    let script = format!("(New-Object System.Media.SoundPlayer '{escaped}').PlaySync()");
    if let Ok(mut child) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .spawn()
    {
        let _ = child.wait();
    }
}

#[cfg(target_os = "linux")]
fn play_native_sound(path: PathBuf) {
    let child = Command::new("paplay")
        .arg(&path)
        .spawn()
        .or_else(|_| Command::new("aplay").arg(path).spawn());
    if let Ok(mut child) = child {
        let _ = child.wait();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn play_native_sound(_path: PathBuf) {}

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

fn emergency_events_unlocked(star_rating: u8) -> bool {
    star_rating >= FacilityKind::Security.unlock_stars()
}

fn apply_emergency_speed(clock: &mut Clock, last_running_speed: &mut SimulationSpeed) {
    // All emergency entry points (fire, terrorist, and future incidents) use
    // this shared control so a high simulation rate cannot hide the response.
    clock.speed = SimulationSpeed::Normal;
    *last_running_speed = SimulationSpeed::Normal;
}

fn nearest_security_fire_response<'a>(
    target: &Facility,
    security_offices: impl Iterator<Item = &'a Facility>,
) -> Option<(u64, f32)> {
    security_offices
        .map(|security| {
            let floor_distance = (security.position.floor - target.position.floor).unsigned_abs();
            let target_center =
                f32::from(target.position.x) + f32::from(target.kind.spec().width) * 0.5;
            let security_center =
                f32::from(security.position.x) + f32::from(security.kind.spec().width) * 0.5;
            let horizontal_distance = (security_center - target_center).abs();
            (
                security.id,
                8.0 + f32::from(floor_distance) * 3.0 + horizontal_distance * 0.05,
            )
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn security_team_start_x(
    target: &Facility,
    security: &Facility,
    floors: &[GridPosition],
    tower_width: u16,
) -> f32 {
    if target.position.floor == security.position.floor {
        return f32::from(security.position.x) + f32::from(security.kind.spec().width) * 0.5;
    }
    let left = floors
        .iter()
        .filter(|position| position.floor == target.position.floor)
        .map(|position| position.x)
        .min()
        .unwrap_or(0);
    let right = floors
        .iter()
        .filter(|position| position.floor == target.position.floor)
        .map(|position| position.x.saturating_add(1))
        .max()
        .unwrap_or(tower_width);
    let target_center = f32::from(target.position.x) + f32::from(target.kind.spec().width) * 0.5;
    if (target_center - f32::from(left)).abs() <= (f32::from(right) - target_center).abs() {
        f32::from(left)
    } else {
        f32::from(right)
    }
}

fn fire_response_timeout_remaining(elapsed: f32) -> u32 {
    (FIRE_RESPONSE_DECISION_SECONDS - elapsed).max(0.0).ceil() as u32
}

fn security_team_animation_frames(elapsed: f32, arrived: bool, member: usize) -> [(u64, f32); 2] {
    if arrived {
        [(4, 1.0), (4, 0.0)]
    } else {
        let (first, second) = person_walk_frame_weights(f64::from(elapsed), member as u64, false);
        [(0, first), (1, second)]
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

fn texture_from_star_bmp(asset: AssetRef, earned: bool) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let mut rgba = image.rgba().to_vec();
    apply_star_transparency(&mut rgba, earned);
    let texture = Texture2D::from_rgba8(width, height, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

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
            // The toolbox cell uses several gray bevel shades, not white.
            // Remove gray background pixels while retaining the black hand
            // outline and its pink/red interior.
            if maximum - minimum <= 4 && maximum > 32 {
                pixel[3] = 0;
            }
        }
    }
    let texture = Texture2D::from_rgba8(CURSOR_WIDTH as u16, CURSOR_HEIGHT as u16, &rgba);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

fn texture_from_bmp(asset: AssetRef) -> Result<Texture2D, String> {
    let image = DibImage::decode_bmp(asset.bytes()?).map_err(|error| error.to_string())?;
    let width = u16::try_from(image.width()).map_err(|_| "bitmap is too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "bitmap is too tall".to_owned())?;
    let texture = Texture2D::from_rgba8(width, height, image.rgba());
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

#[cfg(target_os = "macos")]
fn maximize_window() {
    use objc_rs::runtime::Object;
    use objc_rs::{class, msg_send, sel, sel_impl};

    // AppKit's zoom action is the native maximize operation and deliberately
    // keeps the app in a normal, decorated window rather than a fullscreen Space.
    unsafe {
        let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let window: *mut Object = msg_send![application, mainWindow];
        if !window.is_null() {
            let _: () = msg_send![window, zoom: std::ptr::null_mut::<Object>()];
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
    }

    #[test]
    fn emergencies_remain_disabled_until_security_is_unlocked() {
        let security_stars = FacilityKind::Security.unlock_stars();
        assert!(!emergency_events_unlocked(security_stars - 1));
        assert!(emergency_events_unlocked(security_stars));
        assert!(emergency_events_unlocked(5));
    }

    #[test]
    fn every_emergency_forces_the_clock_and_resume_speed_to_one_x() {
        for starting_speed in [
            SimulationSpeed::Paused,
            SimulationSpeed::Fast,
            SimulationSpeed::Triple,
            SimulationSpeed::Quintuple,
            SimulationSpeed::Tenfold,
        ] {
            let mut clock = Clock::default();
            clock.speed = starting_speed;
            let mut resume_speed = starting_speed;
            apply_emergency_speed(&mut clock, &mut resume_speed);
            assert_eq!(clock.speed, SimulationSpeed::Normal);
            assert_eq!(resume_speed, SimulationSpeed::Normal);
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
        let (_, nearby_seconds) =
            nearest_security_fire_response(&target, std::iter::once(&nearby)).unwrap();
        let (_, distant_seconds) =
            nearest_security_fire_response(&target, std::iter::once(&distant)).unwrap();
        assert!(nearby_seconds <= FIRE_QUICK_RESPONSE_SECONDS);
        assert!(distant_seconds > FIRE_QUICK_RESPONSE_SECONDS);
    }

    #[test]
    fn fire_dialog_counts_down_before_automatic_security_dispatch() {
        assert_eq!(fire_response_timeout_remaining(0.0), 8);
        assert_eq!(fire_response_timeout_remaining(0.1), 8);
        assert_eq!(fire_response_timeout_remaining(7.1), 1);
        assert_eq!(fire_response_timeout_remaining(8.0), 0);
        assert_eq!(fire_response_timeout_remaining(20.0), 0);
    }

    #[test]
    fn security_team_holds_one_clean_frame_after_arrival() {
        let walking = security_team_animation_frames(0.1, false, 0);
        assert_eq!(walking[0].0, 0);
        assert_eq!(walking[1].0, 1);
        assert!((walking[0].1 + walking[1].1 - 1.0).abs() < f32::EPSILON);
        for elapsed in [0.0, 0.2, 1.0, 20.0] {
            for member in 0..3 {
                assert_eq!(
                    security_team_animation_frames(elapsed, true, member),
                    [(4, 1.0), (4, 0.0)]
                );
            }
        }
    }

    #[test]
    fn security_team_enters_from_the_nearest_outside_stair_on_other_floors() {
        let target = Facility {
            id: 1,
            kind: FacilityKind::Shop,
            position: GridPosition { x: 10, floor: 6 },
            occupancy: simtower_core::FacilityOccupancy::Occupied,
            price_level: 1,
            stories: 1,
        };
        let security = Facility {
            id: 2,
            kind: FacilityKind::Security,
            position: GridPosition { x: 100, floor: 2 },
            occupancy: simtower_core::FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories: 1,
        };
        let floors = (4..80)
            .map(|x| GridPosition { x, floor: 6 })
            .collect::<Vec<_>>();
        assert_eq!(security_team_start_x(&target, &security, &floors, 256), 4.0);

        let same_floor_security = Facility {
            position: GridPosition { x: 52, floor: 6 },
            ..security
        };
        assert_eq!(
            security_team_start_x(&target, &same_floor_security, &floors, 256),
            60.0
        );
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
        let sounds = NativeSounds {
            construction: PathBuf::new(),
            lobby_segment: PathBuf::new(),
            demolition: PathBuf::new(),
            no_money: PathBuf::new(),
            payment: PathBuf::new(),
            fire_response: PathBuf::new(),
            elevator_move: PathBuf::new(),
            elevator_open: [PathBuf::new(), PathBuf::new()],
            crowd: PathBuf::new(),
            muted: Cell::new(false),
        };
        assert!(!sounds.is_muted());
        assert!(sounds.toggle_muted());
        assert!(sounds.is_muted());
        assert!(!sounds.toggle_muted());
        assert!(!sounds.is_muted());
    }

    #[test]
    fn simulated_days_include_a_seven_day_week() {
        assert_eq!(weekday_name(1), "Monday");
        assert_eq!(weekday_name(5), "Friday");
        assert_eq!(weekday_name(7), "Sunday");
        assert_eq!(weekday_name(8), "Monday");
    }

    #[test]
    fn settled_patronage_matches_original_weekday_and_weekend_business_levels() {
        assert_eq!(
            settled_commercial_patronage(FacilityKind::FastFood, false),
            35
        );
        assert_eq!(
            settled_commercial_patronage(FacilityKind::FastFood, true),
            48
        );
        assert_eq!(settled_commercial_patronage(FacilityKind::Shop, false), 25);
        assert_eq!(settled_commercial_patronage(FacilityKind::Shop, true), 30);
        assert_eq!(
            settled_commercial_patronage(FacilityKind::Restaurant, false),
            35
        );
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
    fn narrow_topbar_controls_push_the_clock_to_the_right() {
        assert_eq!(topbar_clock_x(1_200.0, 160.0, 430.0), 520.0);
        assert_eq!(topbar_clock_x(800.0, 160.0, 430.0), 442.0);
    }

    #[test]
    fn game_menu_exposes_file_actions_and_automatic_report_toggle() {
        assert_eq!(GAME_MENU_ACTIONS.len(), 5);
        assert_eq!(
            GAME_MENU_ACTIONS.map(|(_, label)| label),
            ["New", "Save", "Load", "Automatic reports", "Quit"]
        );
        for (index, (action, _)) in GAME_MENU_ACTIONS.iter().enumerate() {
            assert_eq!(
                game_menu_action_at(game_menu_entry_rect(index).center()),
                Some(*action)
            );
        }
        assert_eq!(game_menu_button_rect().x, 6.0);
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
            automatic_finance_reports: false,
            tenant_variants: HashMap::new(),
            tenant_variant_bags: HashMap::new(),
        };
        let json = serde_json::to_vec(&save).expect("save should encode");
        let decoded: SaveGame = serde_json::from_slice(&json).expect("save should decode");
        assert_eq!(decoded.format_version, SAVE_FORMAT_VERSION);
        assert_eq!(decoded.tower, tower);
        assert_eq!(decoded.traffic, traffic);
        assert_eq!(decoded.last_running_speed, SimulationSpeed::Triple);
        assert_eq!(decoded.finance_ledger.tenant_income, 25_000);
        assert!(!decoded.automatic_finance_reports);
        decoded.tower.validate_save_state().unwrap();
        decoded.traffic.validate_save_state(&decoded.tower).unwrap();
    }

    #[test]
    fn report_currency_is_grouped_and_keeps_the_sign_before_the_dollar() {
        assert_eq!(format_number(2_000_000), "2,000,000");
        assert_eq!(format_currency(2_000_000), "$2,000,000");
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
        assert_eq!(FIRE_RESPONSE_SOUND_ID, 10004);
        assert_eq!(ELEVATOR_MOVE_SOUND_ID, 6000);
        assert_eq!(ELEVATOR_OPEN_SOUND_IDS, [6001, 6002]);
        assert_eq!(CROWD_SOUND_ID, 8000);
    }

    #[test]
    fn embedded_facility_bitmaps_cover_the_registered_source_rectangles() {
        let resources: [(FacilityKind, AssetRef, u32, u32); 7] = [
            (FacilityKind::Lobby, LOBBY_BACKGROUND_BMP, 992, 36),
            (FacilityKind::Lobby, LOBBY_SECOND_STORY_BMP, 992, 36),
            (FacilityKind::Lobby, LOBBY_THIRD_STORY_BMP, 992, 36),
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
            assert!(image.width() >= u32::from(mapping.source_width));
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
        assert_eq!(hotel_sprite_frame(12 * 60, 1), 0);
        assert_eq!(hotel_sprite_frame(20 * 60, 1), 1);
        assert_eq!(hotel_sprite_frame(12 * 60, 0), 2);
        assert_eq!(hotel_sprite_frame(20 * 60, 0), 3);
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
    }

    #[test]
    fn traffic_audio_events_follow_people_and_elevator_transitions() {
        let idle = TrafficAudioSnapshot {
            people: 2,
            moving_cars: 0,
            passengers: 0,
        };
        let arriving = TrafficAudioSnapshot {
            people: 3,
            moving_cars: 1,
            passengers: 0,
        };
        assert_eq!(
            traffic_sound_events(idle, arriving),
            vec![TrafficSoundEvent::ElevatorMove, TrafficSoundEvent::Crowd]
        );

        let boarded = TrafficAudioSnapshot {
            people: 3,
            moving_cars: 0,
            passengers: 1,
        };
        assert_eq!(
            traffic_sound_events(arriving, boarded),
            vec![TrafficSoundEvent::ElevatorOpen]
        );
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
