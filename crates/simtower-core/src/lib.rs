//! Platform-independent tower simulation.
//!
//! Values that have not yet been verified against the original executable are
//! deliberately kept out of this crate. Known construction prices and the
//! lobby floor rule come from strings embedded in the original game.

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

mod traffic;

pub use traffic::{
    ABANDON_ELEVATOR_WAIT_SECONDS, ANGRY_WAIT_SECONDS, CONCERNED_WAIT_SECONDS, ELEVATOR_CAPACITY,
    EXPRESS_ELEVATOR_CAPACITY, ElevatorCar, ElevatorDirection, ElevatorMode, ElevatorSchedule,
    ElevatorShaft, MAX_ELEVATOR_CARS, MAX_ELEVATOR_QUEUE_PER_FLOOR, MAX_SIMULATED_PEOPLE, Person,
    PersonActivity, PersonMood, TrafficSimulation, elevator_capacity, facility_is_open,
    floor_from_ordinal, floor_ordinal, schedule_index, schedule_period,
};

pub const MIN_FLOOR: i16 = -9;
pub const MAX_FLOOR: i16 = 100;
pub const DEFAULT_TOWER_WIDTH: u16 = 256;
pub const DEFAULT_STARTING_CASH: i64 = 2_000_000;
pub const FLOOR_CONSTRUCTION_COST: i64 = 500;
pub const MAX_ELEVATOR_SHAFTS: usize = 24;
pub const MAX_STAIRS_AND_ESCALATORS: usize = 64;
pub const MAX_SECURITY_OFFICES: usize = 10;
pub const MAX_MEDICAL_CENTERS: usize = 10;
pub const MAX_THEATRES_AND_PARTY_HALLS: usize = 16;
pub const MAX_COMMERCIAL_TENANTS: usize = 512;
pub const MAX_PARKING_SPACES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FacilityKind {
    Lobby,
    Office,
    Condo,
    HotelSingle,
    HotelTwin,
    HotelSuite,
    Restaurant,
    FastFood,
    Shop,
    Cinema,
    PartyHall,
    Metro,
    Parking,
    Medical,
    Security,
    Recycling,
    Stairs,
    Escalator,
    Elevator,
    ServiceElevator,
    ExpressElevator,
    Housekeeping,
    Ramp,
    Cathedral,
}

impl FacilityKind {
    pub const ALL: [Self; 24] = [
        Self::Lobby,
        Self::Office,
        Self::Condo,
        Self::HotelSingle,
        Self::HotelTwin,
        Self::HotelSuite,
        Self::Restaurant,
        Self::FastFood,
        Self::Shop,
        Self::Cinema,
        Self::PartyHall,
        Self::Metro,
        Self::Parking,
        Self::Medical,
        Self::Security,
        Self::Recycling,
        Self::Stairs,
        Self::Escalator,
        Self::Elevator,
        Self::ServiceElevator,
        Self::ExpressElevator,
        Self::Housekeeping,
        Self::Ramp,
        Self::Cathedral,
    ];

    pub const fn spec(self) -> FacilitySpec {
        match self {
            // Lobbies are variable-length structural strips in the original.
            // Each placement is one slice so click-dragging can extend them.
            Self::Lobby => FacilitySpec::new("Lobby", 1, 1, 1_250),
            Self::Office => FacilitySpec::new("Office", 9, 1, 40_000),
            Self::Condo => FacilitySpec::new("Condominium", 16, 1, 80_000),
            Self::HotelSingle => FacilitySpec::new("Single hotel room", 4, 1, 20_000),
            Self::HotelTwin => FacilitySpec::new("Twin hotel room", 6, 1, 50_000),
            Self::HotelSuite => FacilitySpec::new("Hotel suite", 10, 1, 100_000),
            Self::Restaurant => FacilitySpec::new("Restaurant", 24, 1, 200_000),
            Self::FastFood => FacilitySpec::new("Fast food", 16, 1, 100_000),
            Self::Shop => FacilitySpec::new("Shop", 12, 1, 100_000),
            Self::Cinema => FacilitySpec::new("Movie theater", 31, 2, 500_000),
            Self::PartyHall => FacilitySpec::new("Party hall", 24, 2, 100_000),
            Self::Metro => FacilitySpec::new("Metro station", 30, 3, 1_000_000),
            Self::Parking => FacilitySpec::new("Parking space", 4, 1, 3_000),
            Self::Medical => FacilitySpec::new("Medical center", 26, 1, 500_000),
            Self::Security => FacilitySpec::new("Security", 16, 1, 100_000),
            Self::Recycling => FacilitySpec::new("Recycling center", 25, 2, 500_000),
            Self::Stairs => FacilitySpec::new("Stairs", 8, 2, 5_000),
            Self::Escalator => FacilitySpec::new("Escalator", 8, 2, 20_000),
            Self::Elevator => FacilitySpec::new("Elevator", 4, 1, 200_000),
            Self::ServiceElevator => FacilitySpec::new("Service elevator", 4, 1, 100_000),
            Self::ExpressElevator => FacilitySpec::new("Express elevator", 6, 1, 400_000),
            Self::Housekeeping => FacilitySpec::new("Housekeeping", 15, 1, 50_000),
            Self::Ramp => FacilitySpec::new("Parking ramp", 16, 1, 50_000),
            Self::Cathedral => FacilitySpec::new("Cathedral", 28, 5, 3_000_000),
        }
    }

    /// Star level at which the original SimTower build tool exposes this item.
    pub const fn unlock_stars(self) -> u8 {
        match self {
            Self::Lobby
            | Self::Office
            | Self::Condo
            | Self::FastFood
            | Self::Stairs
            | Self::Elevator => 1,
            Self::HotelSingle | Self::Security | Self::ServiceElevator | Self::Housekeeping => 2,
            Self::HotelTwin
            | Self::HotelSuite
            | Self::Restaurant
            | Self::Shop
            | Self::Cinema
            | Self::PartyHall
            | Self::Parking
            | Self::Medical
            | Self::Recycling
            | Self::Escalator
            | Self::ExpressElevator
            | Self::Ramp => 3,
            Self::Metro => 4,
            Self::Cathedral => 5,
        }
    }

    /// Maximum population contribution used by SimTower's rating counter.
    /// Commercial population is the venue's peak patron count; the original
    /// carries the most recently observed value between opening periods.
    pub const fn population_capacity(self) -> u32 {
        match self {
            Self::Office => 6,
            Self::Condo => 3,
            Self::HotelSingle => 1,
            Self::HotelTwin | Self::HotelSuite => 2,
            Self::FastFood => 48,
            Self::Shop => 30,
            Self::Restaurant => 35,
            Self::PartyHall => 50,
            Self::Cinema => 120,
            _ => 0,
        }
    }

    pub const fn has_tenant_occupancy(self) -> bool {
        self.population_capacity() > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FacilitySpec {
    pub name: &'static str,
    pub width: u16,
    pub height: u16,
    pub construction_cost: i64,
}

impl FacilitySpec {
    const fn new(name: &'static str, width: u16, height: u16, construction_cost: i64) -> Self {
        Self {
            name,
            width,
            height,
            construction_cost,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct GridPosition {
    pub x: u16,
    pub floor: i16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FacilityOccupancy {
    NotApplicable,
    VacantUntil(u64),
    Occupied,
    /// The room was destroyed by an emergency and remains unusable until it
    /// is demolished and rebuilt.
    Burned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Facility {
    pub id: u64,
    pub kind: FacilityKind,
    pub position: GridPosition,
    pub occupancy: FacilityOccupancy,
    /// Original four-step rent/sale price index: low through maximum.
    pub price_level: u8,
    /// Number of vertically stacked lobby stories. Other facilities use their
    /// fixed specification height.
    pub stories: u8,
}

impl Facility {
    pub fn end_x(&self) -> u16 {
        self.position.x + self.kind.spec().width
    }

    pub const fn is_occupied(&self) -> bool {
        matches!(self.occupancy, FacilityOccupancy::Occupied)
    }

    pub const fn is_burned(&self) -> bool {
        matches!(self.occupancy, FacilityOccupancy::Burned)
    }

    pub fn height(&self) -> u16 {
        if self.kind == FacilityKind::Lobby {
            self.stories as u16
        } else {
            self.kind.spec().height
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SecretDiscovery {
    pub position: GridPosition,
    pub amount: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncomeEvent {
    pub facility_id: u64,
    pub kind: FacilityKind,
    pub amount: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SimulationSpeed {
    Paused,
    Normal,
    Fast,
    Triple,
    Quintuple,
    Tenfold,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Clock {
    pub day: u32,
    pub minute_of_day: u16,
    pub speed: SimulationSpeed,
    subminute_nanoseconds: u32,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            day: 1,
            minute_of_day: 8 * 60,
            speed: SimulationSpeed::Normal,
            subminute_nanoseconds: 0,
        }
    }
}

impl Clock {
    pub fn absolute_minute(&self) -> u64 {
        u64::from(self.day.saturating_sub(1)) * 24 * 60 + u64::from(self.minute_of_day)
    }

    pub fn advance(&mut self, real_seconds: f32) {
        let minutes_per_second = match self.speed {
            SimulationSpeed::Paused => 0_u64,
            SimulationSpeed::Normal => 2,
            SimulationSpeed::Fast => 4,
            SimulationSpeed::Triple => 6,
            SimulationSpeed::Quintuple => 10,
            SimulationSpeed::Tenfold => 20,
        };
        let elapsed_real_nanoseconds = (real_seconds * 1_000_000_000.0).round() as u64;
        let accumulated =
            u64::from(self.subminute_nanoseconds) + elapsed_real_nanoseconds * minutes_per_second;
        let elapsed_minutes = accumulated / 1_000_000_000;
        self.subminute_nanoseconds = (accumulated % 1_000_000_000) as u32;
        let total = u64::from(self.minute_of_day) + elapsed_minutes;
        self.day += (total / (24 * 60)) as u32;
        self.minute_of_day = (total % (24 * 60)) as u16;
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tower {
    width: u16,
    cash: i64,
    next_facility_id: u64,
    floors: Vec<GridPosition>,
    facilities: Vec<Facility>,
    starting_cash_cheat_used: bool,
    treasure_position: GridPosition,
    treasure_found: bool,
    recent_secret: Option<SecretDiscovery>,
    pub clock: Clock,
}

impl Default for Tower {
    fn default() -> Self {
        Self::new(DEFAULT_TOWER_WIDTH, DEFAULT_STARTING_CASH)
    }
}

impl Tower {
    pub fn new(width: u16, starting_cash: i64) -> Self {
        assert!(width > 0, "tower width must be non-zero");
        Self {
            width,
            cash: starting_cash,
            next_facility_id: 1,
            floors: Vec::new(),
            facilities: Vec::new(),
            starting_cash_cheat_used: false,
            treasure_position: GridPosition {
                x: width.saturating_mul(37) / 64,
                floor: -3,
            },
            treasure_found: false,
            recent_secret: None,
            clock: Clock::default(),
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn cash(&self) -> i64 {
        self.cash
    }

    /// Pays a one-off emergency or service charge without allowing the tower
    /// balance to go below zero.
    pub fn try_spend(&mut self, amount: i64) -> bool {
        if amount < 0 || self.cash < amount {
            return false;
        }
        self.cash -= amount;
        true
    }

    /// Permanently disables a tenant after destructive fire damage. The
    /// facility remains in place so the bulldozer is required before a
    /// replacement can be built.
    pub fn mark_facility_burned(&mut self, facility_id: u64) -> bool {
        let Some(facility) = self
            .facilities
            .iter_mut()
            .find(|item| item.id == facility_id)
        else {
            return false;
        };
        if !facility.kind.has_tenant_occupancy() {
            return false;
        }
        facility.occupancy = FacilityOccupancy::Burned;
        true
    }

    pub fn facilities(&self) -> &[Facility] {
        &self.facilities
    }

    pub fn floors(&self) -> &[GridPosition] {
        &self.floors
    }

    pub fn validate_save_state(&self) -> Result<(), String> {
        if self.width == 0 || self.width > 4096 {
            return Err("tower width is outside the supported range".to_owned());
        }
        if self.clock.day == 0 || self.clock.minute_of_day >= 24 * 60 {
            return Err("tower clock is invalid".to_owned());
        }
        let mut floor_cells = HashSet::new();
        for floor in &self.floors {
            if floor.x >= self.width || !(MIN_FLOOR..=MAX_FLOOR).contains(&floor.floor) {
                return Err("save contains an invalid structural floor cell".to_owned());
            }
            if !floor_cells.insert(*floor) {
                return Err("save contains a duplicate structural floor cell".to_owned());
            }
        }
        let mut facility_ids = HashSet::new();
        for facility in &self.facilities {
            if facility.id == 0 || !facility_ids.insert(facility.id) {
                return Err("save contains an invalid or duplicate facility id".to_owned());
            }
            if facility.end_x() > self.width
                || !(MIN_FLOOR..=MAX_FLOOR).contains(&facility.position.floor)
                || facility.price_level > 3
                || (facility.kind == FacilityKind::Lobby && !(1..=3).contains(&facility.stories))
                || (facility.kind != FacilityKind::Lobby
                    && facility.stories != facility.kind.spec().height as u8)
                || (facility.is_burned() && !facility.kind.has_tenant_occupancy())
            {
                return Err("save contains invalid facility data".to_owned());
            }
        }
        let minimum_next_id = self
            .facilities
            .iter()
            .map(|facility| facility.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        if self.next_facility_id < minimum_next_id {
            return Err("save contains an invalid next facility id".to_owned());
        }
        Ok(())
    }

    /// The original fresh-tower secret: trying the lobby tool at the extreme
    /// lower-left doubles the initial two-million-dollar fund.
    pub fn activate_starting_cash_cheat(&mut self) -> bool {
        if self.starting_cash_cheat_used || !self.facilities.is_empty() {
            return false;
        }
        self.starting_cash_cheat_used = true;
        self.cash = self.cash.max(DEFAULT_STARTING_CASH * 2);
        true
    }

    pub fn take_secret_discovery(&mut self) -> Option<SecretDiscovery> {
        self.recent_secret.take()
    }

    /// Selects the one hidden underground treasure site for this tower. The
    /// desktop supplies a fresh seed for each new game while deterministic
    /// core tests can retain the stable default location.
    pub fn seed_treasure_site(&mut self, seed: u64) {
        if self.treasure_found {
            return;
        }
        let mixed = seed.wrapping_mul(6_364_136_223_846_793_005).rotate_left(17);
        self.treasure_position = GridPosition {
            x: (mixed % u64::from(self.width)) as u16,
            floor: -3 - ((mixed >> 32) % 6) as i16,
        };
    }

    pub fn population(&self) -> u32 {
        self.facilities
            .iter()
            .filter(|facility| facility.is_occupied())
            .map(|facility| facility.kind.population_capacity())
            .sum()
    }

    pub fn advance_time(&mut self, real_seconds: f32) -> Vec<IncomeEvent> {
        self.advance_time_with_visitors(real_seconds, &[])
    }

    /// Advances the clock and settles tenant revenue using actual patronage
    /// observed by the traffic simulation. The original uses distinct poor,
    /// average, and good daily receipts for public businesses.
    pub fn advance_time_with_visitors(
        &mut self,
        real_seconds: f32,
        visitors: &[(u64, u32)],
    ) -> Vec<IncomeEvent> {
        self.advance_time_with_economy(real_seconds, visitors, &[])
    }

    /// Advances the complete economy with live commercial patronage and the
    /// actual number of cars in each elevator shaft. The original charges
    /// once per shaft plus once per installed car, never per shaft cell.
    pub fn advance_time_with_economy(
        &mut self,
        real_seconds: f32,
        visitors: &[(u64, u32)],
        elevator_cars: &[(u64, FacilityKind, u8)],
    ) -> Vec<IncomeEvent> {
        self.advance_time_with_connected_economy(real_seconds, visitors, elevator_cars, None)
    }

    /// Desktop simulation entry point. When a reachability set is supplied,
    /// a tenant's move-in clock may expire but it remains vacant until its
    /// space has a complete route from a ground-floor lobby.
    pub fn advance_time_with_connected_economy(
        &mut self,
        real_seconds: f32,
        visitors: &[(u64, u32)],
        elevator_cars: &[(u64, FacilityKind, u8)],
        reachable_facilities: Option<&[u64]>,
    ) -> Vec<IncomeEvent> {
        let previous_minute = self.clock.absolute_minute();
        self.clock.advance(real_seconds);
        let current_minute = self.clock.absolute_minute();
        let visitor_lookup = visitors
            .iter()
            .copied()
            .collect::<std::collections::HashMap<_, _>>();
        let reachable_lookup =
            reachable_facilities.map(|ids| ids.iter().copied().collect::<HashSet<_>>());
        let mut events = Vec::new();
        let mut newly_occupied = HashSet::new();
        for facility in &mut self.facilities {
            let is_reachable = reachable_lookup
                .as_ref()
                .is_none_or(|ids| ids.contains(&facility.id));
            if matches!(
                facility.occupancy,
                FacilityOccupancy::VacantUntil(move_in_minute)
                    if current_minute >= move_in_minute && is_reachable
            ) {
                facility.occupancy = FacilityOccupancy::Occupied;
                newly_occupied.insert(facility.id);
                if let Some(amount) = move_in_income(facility.kind, facility.price_level) {
                    self.cash += amount;
                    events.push(IncomeEvent {
                        facility_id: facility.id,
                        kind: facility.kind,
                        amount,
                    });
                }
            }
        }
        let rating = self.star_rating();
        for facility in &self.facilities {
            let patron_count = visitor_lookup.get(&facility.id).copied().unwrap_or(0);
            let revenue = if facility.is_occupied() && !newly_occupied.contains(&facility.id) {
                recurring_income(facility.kind, facility.price_level, patron_count).saturating_mul(
                    i64::from(scheduled_payment_count(
                        facility.kind,
                        previous_minute,
                        current_minute,
                    )),
                )
            } else {
                0
            };
            let expense = maintenance_cost(facility.kind, rating).saturating_mul(i64::from(
                maintenance_payment_count(facility.kind, previous_minute, current_minute),
            ));
            let amount = revenue.saturating_sub(expense);
            if amount != 0 {
                self.cash += amount;
                events.push(IncomeEvent {
                    facility_id: facility.id,
                    kind: facility.kind,
                    amount,
                });
            }
        }
        let quarterly_payments = quarterly_payment_count(previous_minute, current_minute);
        if quarterly_payments != 0 {
            let inferred_shafts;
            let elevator_cars = if elevator_cars.is_empty() {
                let mut shafts = Vec::new();
                for facility in self.facilities.iter().filter(|facility| {
                    matches!(
                        facility.kind,
                        FacilityKind::Elevator
                            | FacilityKind::ServiceElevator
                            | FacilityKind::ExpressElevator
                    )
                }) {
                    if !shafts
                        .iter()
                        .any(|(_, kind, x)| *kind == facility.kind && *x == facility.position.x)
                    {
                        shafts.push((facility.id, facility.kind, facility.position.x));
                    }
                }
                inferred_shafts = shafts
                    .into_iter()
                    .map(|(id, kind, _)| (id, kind, 1_u8))
                    .collect::<Vec<_>>();
                inferred_shafts.as_slice()
            } else {
                elevator_cars
            };
            for &(shaft_id, kind, car_count) in elevator_cars {
                let per_shaft = elevator_shaft_maintenance_cost(kind);
                let per_car = elevator_car_maintenance_cost(kind);
                if per_shaft == 0 && per_car == 0 {
                    continue;
                }
                let amount = -per_shaft
                    .saturating_add(per_car.saturating_mul(i64::from(car_count)))
                    .saturating_mul(i64::from(quarterly_payments));
                self.cash += amount;
                events.push(IncomeEvent {
                    facility_id: shaft_id,
                    kind,
                    amount,
                });
            }
        }
        events
    }

    pub fn set_price_level(&mut self, facility_id: u64, price_level: u8) -> bool {
        let Some(facility) = self
            .facilities
            .iter_mut()
            .find(|facility| facility.id == facility_id && facility.kind.has_tenant_occupancy())
        else {
            return false;
        };
        facility.price_level = price_level.min(3);
        true
    }

    /// Current tower grade using the original cumulative population and
    /// buildable facility gates. The VIP visit and medical/recycling demand
    /// events are not simulated yet, so their built prerequisites are used.
    pub fn star_rating(&self) -> u8 {
        self.star_rating_for_population(self.population())
    }

    /// Calculates the tower grade from an externally observed population.
    /// The desktop simulation uses this for commercial spaces, whose
    /// population is their actual patron count rather than their maximum
    /// capacity.
    pub fn star_rating_for_population(&self, population: u32) -> u8 {
        if population < 300 {
            return 1;
        }

        let has = |kind| self.facilities.iter().any(|facility| facility.kind == kind);
        if population < 1_000 || !has(FacilityKind::Security) {
            return 2;
        }

        let suite_count = self
            .facilities
            .iter()
            .filter(|facility| facility.kind == FacilityKind::HotelSuite && facility.is_occupied())
            .count();
        let four_star_services = suite_count >= 2
            && has(FacilityKind::Medical)
            && has(FacilityKind::Recycling)
            && has(FacilityKind::Parking);
        if population < 5_000 || !four_star_services {
            return 3;
        }
        if population < 10_000 || !has(FacilityKind::Metro) {
            return 4;
        }
        5
    }

    pub fn has_floor(&self, position: GridPosition) -> bool {
        self.floors.contains(&position)
    }

    /// True when this segment touches an existing same-type shaft above or
    /// below. The original charges the shaft purchase only for the first
    /// segment; endpoint extensions pay only for any new structural deck.
    pub fn is_elevator_extension(&self, kind: FacilityKind, position: GridPosition) -> bool {
        if !matches!(
            kind,
            FacilityKind::Elevator | FacilityKind::ServiceElevator | FacilityKind::ExpressElevator
        ) {
            return false;
        }
        let ordinal = floor_ordinal(position.floor);
        self.facilities.iter().any(|facility| {
            facility.kind == kind
                && facility.position.x == position.x
                && (floor_ordinal(facility.position.floor) - ordinal).abs() == 1
        })
    }

    pub fn placement_cost(&self, kind: FacilityKind, position: GridPosition) -> i64 {
        if self.is_elevator_extension(kind, position) {
            0
        } else {
            kind.spec().construction_cost
        }
    }

    pub fn can_place_floor(&self, position: GridPosition) -> Result<(), PlacementError> {
        if !(MIN_FLOOR..=MAX_FLOOR).contains(&position.floor) {
            return Err(PlacementError::FloorOutOfRange {
                floor: position.floor,
            });
        }
        if position.x >= self.width {
            return Err(PlacementError::OutsideTower);
        }
        if self.has_floor(position) {
            return Err(PlacementError::FloorAlreadyExists);
        }
        // The original Terrain/Floor placement gate permits no cantilever:
        // every cell above the first floor must have a deck cell directly
        // below it. This is the rule behind the executable's message
        // "Cannot place items wider than floor below!".
        if position.floor > 1
            && !self.has_floor(GridPosition {
                x: position.x,
                floor: position.floor - 1,
            })
        {
            return Err(PlacementError::FloorWiderThanBelow {
                floor: position.floor,
                x: position.x,
            });
        }
        if self.cash < FLOOR_CONSTRUCTION_COST {
            return Err(PlacementError::InsufficientFunds {
                required: FLOOR_CONSTRUCTION_COST,
                available: self.cash,
            });
        }
        Ok(())
    }

    pub fn place_floor(&mut self, position: GridPosition) -> Result<(), PlacementError> {
        self.can_place_floor(position)?;
        self.cash -= FLOOR_CONSTRUCTION_COST;
        self.floors.push(position);
        Ok(())
    }

    pub fn can_place(
        &self,
        kind: FacilityKind,
        position: GridPosition,
    ) -> Result<(), PlacementError> {
        let spec = kind.spec();
        if !(MIN_FLOOR..=MAX_FLOOR).contains(&position.floor) {
            return Err(PlacementError::FloorOutOfRange {
                floor: position.floor,
            });
        }
        if position.x.saturating_add(spec.width) > self.width {
            return Err(PlacementError::OutsideTower);
        }
        if kind == FacilityKind::Lobby && position.floor != 1 && position.floor.rem_euclid(15) != 0
        {
            return Err(PlacementError::InvalidLobbyFloor {
                floor: position.floor,
            });
        }
        self.check_facility_limit(kind, position)?;
        if (position.x..position.x + spec.width).any(|x| {
            !self.has_floor(GridPosition {
                x,
                floor: position.floor,
            })
        }) {
            return Err(PlacementError::MissingFloor);
        }
        let is_stairway = matches!(kind, FacilityKind::Stairs | FacilityKind::Escalator);
        let is_transport = is_transport_kind(kind);
        if is_stairway {
            // Displayed floor numbers skip zero: B1 (-1) connects directly
            // to the first floor (1). At every other level, "above" is +1.
            let upper_floor = if position.floor == -1 {
                1
            } else {
                position.floor + 1
            };
            if upper_floor > MAX_FLOOR
                || (position.x..position.x + spec.width).any(|x| {
                    !self.has_floor(GridPosition {
                        x,
                        floor: upper_floor,
                    })
                })
            {
                return Err(PlacementError::MissingConnectingFloor {
                    base_floor: position.floor,
                    upper_floor,
                });
            }
        }
        let placement_cost = self.placement_cost(kind, position);
        if self.cash < placement_cost {
            return Err(PlacementError::InsufficientFunds {
                required: placement_cost,
                available: self.cash,
            });
        }

        let candidate_end = position.x + spec.width;
        if self.facilities.iter().any(|existing| {
            let overlaps_floor = if existing.kind == FacilityKind::Lobby {
                position.floor >= existing.position.floor
                    && position.floor < existing.position.floor + existing.stories as i16
            } else {
                existing.position.floor == position.floor
            };
            let overlaps = overlaps_floor
                && position.x < existing.end_x()
                && candidate_end > existing.position.x;
            if !overlaps {
                return false;
            }
            // Vertical transport overlays tenant/lobby art in the original;
            // the elevator window even has a Show toggle to reveal rooms
            // behind a shaft. Transport units still cannot overlap each other,
            // while ordinary facilities continue to conflict with tenants.
            is_transport == is_transport_kind(existing.kind)
        }) {
            return Err(PlacementError::Occupied);
        }
        Ok(())
    }

    fn check_facility_limit(
        &self,
        kind: FacilityKind,
        position: GridPosition,
    ) -> Result<(), PlacementError> {
        let count = |candidate| {
            self.facilities
                .iter()
                .filter(|facility| facility.kind == candidate)
                .count()
        };
        let combined_count = |kinds: &[FacilityKind]| {
            self.facilities
                .iter()
                .filter(|facility| kinds.contains(&facility.kind))
                .count()
        };
        let (current, limit, label) = match kind {
            FacilityKind::Elevator
            | FacilityKind::ServiceElevator
            | FacilityKind::ExpressElevator => {
                let extends_existing = self.is_elevator_extension(kind, position);
                if extends_existing {
                    return Ok(());
                }
                let mut shafts = Vec::new();
                for facility in self.facilities.iter().filter(|facility| {
                    matches!(
                        facility.kind,
                        FacilityKind::Elevator
                            | FacilityKind::ServiceElevator
                            | FacilityKind::ExpressElevator
                    )
                }) {
                    let key = (facility.kind, facility.position.x);
                    if !shafts.contains(&key) {
                        shafts.push(key);
                    }
                }
                (shafts.len(), MAX_ELEVATOR_SHAFTS, "elevator shafts")
            }
            FacilityKind::Stairs | FacilityKind::Escalator => (
                combined_count(&[FacilityKind::Stairs, FacilityKind::Escalator]),
                MAX_STAIRS_AND_ESCALATORS,
                "stairs and escalators",
            ),
            FacilityKind::Security => (count(kind), MAX_SECURITY_OFFICES, "security offices"),
            FacilityKind::Medical => (count(kind), MAX_MEDICAL_CENTERS, "medical centers"),
            FacilityKind::Cinema | FacilityKind::PartyHall => (
                combined_count(&[FacilityKind::Cinema, FacilityKind::PartyHall]),
                MAX_THEATRES_AND_PARTY_HALLS,
                "theatres and party halls",
            ),
            FacilityKind::FastFood | FacilityKind::Restaurant | FacilityKind::Shop => (
                combined_count(&[
                    FacilityKind::FastFood,
                    FacilityKind::Restaurant,
                    FacilityKind::Shop,
                ]),
                MAX_COMMERCIAL_TENANTS,
                "commercial tenants",
            ),
            FacilityKind::Parking => (count(kind), MAX_PARKING_SPACES, "parking spaces"),
            FacilityKind::Metro | FacilityKind::Cathedral => (count(kind), 1, kind.spec().name),
            FacilityKind::Ramp => {
                if self.facilities.iter().any(|facility| {
                    facility.kind == FacilityKind::Ramp && facility.position.x != position.x
                }) {
                    return Err(PlacementError::SingleRampColumn);
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        if current >= limit {
            Err(PlacementError::FacilityLimitReached { label, limit })
        } else {
            Ok(())
        }
    }

    pub fn place(
        &mut self,
        kind: FacilityKind,
        position: GridPosition,
    ) -> Result<u64, PlacementError> {
        if kind == FacilityKind::Lobby {
            return self.place_lobby(position, 1);
        }
        self.can_place(kind, position)?;
        let placement_cost = self.placement_cost(kind, position);
        let id = self.next_facility_id;
        self.next_facility_id += 1;
        self.cash -= placement_cost;
        let occupancy = if kind.has_tenant_occupancy() {
            FacilityOccupancy::VacantUntil(tenant_move_in_minute(self.clock, kind, id))
        } else {
            FacilityOccupancy::NotApplicable
        };
        self.facilities.push(Facility {
            id,
            kind,
            position,
            occupancy,
            price_level: 1,
            stories: kind.spec().height as u8,
        });
        self.discover_treasure_in_footprint(position, kind.spec().width);
        Ok(id)
    }

    /// Places one horizontal slice of the first-floor super lobby. Holding
    /// Control in the original selected two stories; Control+Shift selected
    /// three. Every story must already have empty structural floor beneath it.
    pub fn place_lobby(
        &mut self,
        position: GridPosition,
        stories: u8,
    ) -> Result<u64, PlacementError> {
        if !(1..=3).contains(&stories) {
            return Err(PlacementError::InvalidLobbyHeight { stories });
        }
        if stories == 1 {
            self.can_place(FacilityKind::Lobby, position)?;
        } else {
            if position.floor != 1 {
                return Err(PlacementError::SuperLobbyOnlyOnFirstFloor);
            }
            let existing_lobbies = self
                .facilities
                .iter()
                .filter(|facility| facility.kind == FacilityKind::Lobby)
                .collect::<Vec<_>>();
            if !existing_lobbies.is_empty()
                && (!existing_lobbies
                    .iter()
                    .all(|facility| facility.position.floor == 1 && facility.stories == stories)
                    || !existing_lobbies.iter().any(|facility| {
                        facility.position.x + 1 == position.x
                            || position.x + 1 == facility.position.x
                    }))
            {
                return Err(PlacementError::SuperLobbyMustBeFirst);
            }
            for story in 0..stories {
                let floor = position.floor + i16::from(story);
                if !self.has_floor(GridPosition {
                    x: position.x,
                    floor,
                }) {
                    return Err(PlacementError::MissingSuperLobbyFloor { floor });
                }
            }
            let required = FacilityKind::Lobby
                .spec()
                .construction_cost
                .saturating_mul(i64::from(stories));
            if self.cash < required {
                return Err(PlacementError::InsufficientFunds {
                    required,
                    available: self.cash,
                });
            }
            if self.facilities.iter().any(|existing| {
                position.x < existing.end_x()
                    && position.x + 1 > existing.position.x
                    && existing.position.floor >= position.floor
                    && existing.position.floor < position.floor + i16::from(stories)
                    && !is_transport_kind(existing.kind)
            }) {
                return Err(PlacementError::Occupied);
            }
        }

        let id = self.next_facility_id;
        self.next_facility_id += 1;
        self.cash -= FacilityKind::Lobby
            .spec()
            .construction_cost
            .saturating_mul(i64::from(stories));
        self.facilities.push(Facility {
            id,
            kind: FacilityKind::Lobby,
            position,
            occupancy: FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories,
        });
        Ok(id)
    }

    fn discover_treasure_in_footprint(&mut self, position: GridPosition, width: u16) {
        if self.treasure_found
            || position.floor != self.treasure_position.floor
            || self.treasure_position.x < position.x
            || self.treasure_position.x >= position.x.saturating_add(width)
        {
            return;
        }
        let amount = match self.star_rating() {
            2 => 200_000,
            3 => 300_000,
            4.. => 500_000,
            _ => return,
        };
        self.treasure_found = true;
        self.cash += amount;
        self.recent_secret = Some(SecretDiscovery {
            position: self.treasure_position,
            amount,
        });
    }

    /// Charge for an additional car placed into an existing elevator shaft.
    /// Shaft construction and car purchases are separate operations: extending
    /// a shaft never implies that another car was bought.
    pub fn purchase_elevator_car(&mut self, kind: FacilityKind) -> Result<(), PlacementError> {
        debug_assert!(matches!(
            kind,
            FacilityKind::Elevator | FacilityKind::ServiceElevator | FacilityKind::ExpressElevator
        ));
        let required = elevator_car_cost(kind);
        if self.cash < required {
            return Err(PlacementError::InsufficientFunds {
                required,
                available: self.cash,
            });
        }
        self.cash -= required;
        Ok(())
    }

    /// The build tool may extend an adjacent shaft and place an additional
    /// car in one gesture. Validate the combined price atomically so a failed
    /// car purchase cannot leave behind a free shaft segment.
    pub fn place_elevator_extension_with_car(
        &mut self,
        kind: FacilityKind,
        position: GridPosition,
    ) -> Result<u64, PlacementError> {
        if !self.is_elevator_extension(kind, position) {
            return self.place(kind, position);
        }
        self.can_place(kind, position)?;
        let required = self
            .placement_cost(kind, position)
            .saturating_add(elevator_car_cost(kind));
        if self.cash < required {
            return Err(PlacementError::InsufficientFunds {
                required,
                available: self.cash,
            });
        }
        let id = self.place(kind, position)?;
        self.cash -= elevator_car_cost(kind);
        Ok(id)
    }

    pub fn demolish(&mut self, id: u64) -> Result<Facility, DemolitionError> {
        let index = self
            .facilities
            .iter()
            .position(|facility| facility.id == id)
            .ok_or(DemolitionError::UnknownFacility(id))?;
        if self.facilities[index].kind == FacilityKind::Lobby {
            return Err(DemolitionError::LobbyIsPermanent);
        }
        Ok(self.facilities.remove(index))
    }

    /// Removes one structural floor cell after all objects drawn above it
    /// have been removed. A cell cannot be removed while it carries a room,
    /// transport connection, or another floor directly above it.
    pub fn demolish_floor(&mut self, position: GridPosition) -> Result<(), DemolitionError> {
        if !self.has_floor(position) {
            return Err(DemolitionError::UnknownFloor(position));
        }
        if self.facilities.iter().any(|facility| {
            let covers_x = position.x >= facility.position.x && position.x < facility.end_x();
            if !covers_x {
                return false;
            }
            let top_floor = facility.position.floor + facility.height() as i16 - 1;
            let occupies_story =
                position.floor >= facility.position.floor && position.floor <= top_floor;
            let connects_stairs = matches!(
                facility.kind,
                FacilityKind::Stairs | FacilityKind::Escalator
            ) && position.floor
                == if facility.position.floor == -1 {
                    1
                } else {
                    facility.position.floor + 1
                };
            occupies_story || connects_stairs
        }) {
            return Err(DemolitionError::FloorOccupied(position));
        }
        let floor_above = if position.floor == -1 {
            1
        } else {
            position.floor + 1
        };
        if floor_above <= MAX_FLOOR
            && self.has_floor(GridPosition {
                x: position.x,
                floor: floor_above,
            })
        {
            return Err(DemolitionError::FloorSupportsAbove {
                position,
                above: floor_above,
            });
        }
        let index = self
            .floors
            .iter()
            .position(|floor| *floor == position)
            .expect("existing floor must have an index");
        self.floors.remove(index);
        Ok(())
    }
}

fn move_in_income(kind: FacilityKind, price_level: u8) -> Option<i64> {
    match kind {
        FacilityKind::Office => Some(income_tier(price_level, [5_000, 10_000, 15_000, 20_000])),
        FacilityKind::Condo => Some(income_tier(
            price_level,
            [100_000, 150_000, 200_000, 250_000],
        )),
        _ => None,
    }
}

fn recurring_income(kind: FacilityKind, price_level: u8, visitors: u32) -> i64 {
    match kind {
        FacilityKind::Office => income_tier(price_level, [5_000, 10_000, 15_000, 20_000]),
        FacilityKind::HotelSingle => income_tier(price_level, [1_000, 2_000, 3_000, 4_000]),
        FacilityKind::HotelTwin => income_tier(price_level, [1_500, 3_000, 4_500, 6_000]),
        FacilityKind::HotelSuite => income_tier(price_level, [3_000, 6_000, 9_000, 12_000]),
        FacilityKind::FastFood => match visitors {
            0..=24 => -3_000,
            25..=39 => 3_000,
            _ => 5_000,
        },
        FacilityKind::Restaurant => match visitors {
            0..=24 => 4_000,
            25..=69 => 6_000,
            _ => 10_000,
        },
        FacilityKind::Shop => income_tier(price_level, [7_500, 15_000, 22_500, 30_000]),
        FacilityKind::Cinema => match visitors {
            0..=39 => 0,
            40..=79 => 2_000,
            _ => 10_000,
        },
        FacilityKind::PartyHall => 20_000,
        _ => 0,
    }
}

fn maintenance_cost(kind: FacilityKind, star_rating: u8) -> i64 {
    match kind {
        FacilityKind::Lobby => match star_rating {
            0..=2 => 0,
            3 => 300,
            _ => 1_000,
        },
        FacilityKind::Escalator => 5_000,
        FacilityKind::Housekeeping => 10_000,
        FacilityKind::Security => 20_000,
        FacilityKind::Ramp => 10_000,
        FacilityKind::Recycling => 50_000,
        FacilityKind::Metro => 100_000,
        _ => 0,
    }
}

const fn elevator_shaft_maintenance_cost(kind: FacilityKind) -> i64 {
    match kind {
        FacilityKind::Elevator | FacilityKind::ServiceElevator => 10_000,
        FacilityKind::ExpressElevator => 20_000,
        _ => 0,
    }
}

const fn elevator_car_maintenance_cost(kind: FacilityKind) -> i64 {
    match kind {
        FacilityKind::Elevator | FacilityKind::ServiceElevator => 10_000,
        FacilityKind::ExpressElevator => 20_000,
        _ => 0,
    }
}

/// Original tuning-resource price for adding a car to an existing shaft.
pub const fn elevator_car_cost(kind: FacilityKind) -> i64 {
    match kind {
        FacilityKind::Elevator => 80_000,
        FacilityKind::ServiceElevator => 50_000,
        FacilityKind::ExpressElevator => 150_000,
        _ => 0,
    }
}

const fn income_tier(price_level: u8, tiers: [i64; 4]) -> i64 {
    tiers[if price_level > 3 { 3 } else { price_level } as usize]
}

fn scheduled_payment_count(kind: FacilityKind, previous: u64, current: u64) -> u32 {
    if current <= previous {
        return 0;
    }
    let payment_minute = match kind {
        FacilityKind::Office | FacilityKind::Shop => 5 * 60,
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => 12 * 60,
        FacilityKind::PartyHall => 17 * 60,
        FacilityKind::Cinema => 20 * 60,
        FacilityKind::FastFood => 21 * 60,
        FacilityKind::Restaurant => 23 * 60,
        _ => return 0,
    };
    let first_day = previous / (24 * 60);
    let last_day = current / (24 * 60);
    (first_day..=last_day)
        .filter(|day| {
            !matches!(kind, FacilityKind::Office | FacilityKind::Shop) || day.is_multiple_of(3)
        })
        .filter(|day| {
            let event = day * 24 * 60 + payment_minute;
            previous < event && event <= current
        })
        .count() as u32
}

fn maintenance_payment_count(kind: FacilityKind, previous: u64, current: u64) -> u32 {
    if current <= previous || maintenance_cost(kind, 5) == 0 {
        return 0;
    }
    quarterly_payment_count(previous, current)
}

fn quarterly_payment_count(previous: u64, current: u64) -> u32 {
    if current <= previous {
        return 0;
    }
    let first_day = previous / (24 * 60);
    let last_day = current / (24 * 60);
    (first_day..=last_day)
        .filter(|day| day.is_multiple_of(3))
        .filter(|day| {
            let event = day * 24 * 60 + 5 * 60;
            previous < event && event <= current
        })
        .count() as u32
}

fn tenant_move_in_minute(clock: Clock, kind: FacilityKind, facility_id: u64) -> u64 {
    let current = clock.absolute_minute();
    let jitter = facility_id % 9;
    if kind == FacilityKind::Condo && !(6 * 60..22 * 60).contains(&u64::from(clock.minute_of_day)) {
        let day_start = current - u64::from(clock.minute_of_day);
        let next_morning = if clock.minute_of_day < 6 * 60 {
            day_start + 8 * 60
        } else {
            day_start + 24 * 60 + 8 * 60
        };
        return next_morning + jitter;
    }

    let base_delay = match kind {
        FacilityKind::Condo => 24,
        FacilityKind::Office => 16,
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => 12,
        FacilityKind::Restaurant
        | FacilityKind::FastFood
        | FacilityKind::Shop
        | FacilityKind::Cinema
        | FacilityKind::PartyHall => 10,
        _ => 0,
    };
    current + base_delay + jitter
}

const fn is_transport_kind(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::Stairs
            | FacilityKind::Escalator
            | FacilityKind::Elevator
            | FacilityKind::ServiceElevator
            | FacilityKind::ExpressElevator
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlacementError {
    FloorOutOfRange { floor: i16 },
    OutsideTower,
    InvalidLobbyFloor { floor: i16 },
    InvalidLobbyHeight { stories: u8 },
    SuperLobbyOnlyOnFirstFloor,
    SuperLobbyMustBeFirst,
    MissingSuperLobbyFloor { floor: i16 },
    FloorAlreadyExists,
    FloorWiderThanBelow { floor: i16, x: u16 },
    MissingFloor,
    MissingConnectingFloor { base_floor: i16, upper_floor: i16 },
    InsufficientFunds { required: i64, available: i64 },
    Occupied,
    FacilityLimitReached { label: &'static str, limit: usize },
    SingleRampColumn,
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FloorOutOfRange { floor } => write!(f, "floor {floor} is outside the tower"),
            Self::OutsideTower => write!(f, "facility extends outside the tower"),
            Self::InvalidLobbyFloor { floor } => {
                write!(f, "a lobby cannot be placed on floor {floor}")
            }
            Self::InvalidLobbyHeight { stories } => {
                write!(f, "a lobby cannot be {stories} stories tall")
            }
            Self::SuperLobbyOnlyOnFirstFloor => {
                write!(
                    f,
                    "two- and three-story lobbies are only available on the first floor"
                )
            }
            Self::SuperLobbyMustBeFirst => {
                write!(f, "only the tower's first lobby can be a multi-story lobby")
            }
            Self::MissingSuperLobbyFloor { floor } => {
                write!(
                    f,
                    "build empty floor space on floor {floor} before painting this lobby"
                )
            }
            Self::FloorAlreadyExists => write!(f, "floor already exists at the selected column"),
            Self::FloorWiderThanBelow { .. } => {
                write!(f, "cannot place items wider than the floor below")
            }
            Self::MissingFloor => write!(f, "build empty floor under the entire footprint first"),
            Self::MissingConnectingFloor { .. } => {
                write!(f, "stairs require a base floor and a floor directly above")
            }
            Self::InsufficientFunds {
                required,
                available,
            } => write!(
                f,
                "construction costs ${required}, but only ${available} is available"
            ),
            Self::Occupied => write!(f, "the selected space is occupied"),
            Self::FacilityLimitReached { label, limit } => {
                write!(f, "the tower limit for {label} is {limit}")
            }
            Self::SingleRampColumn => {
                write!(f, "parking ramps must remain in one vertical column")
            }
        }
    }
}

impl std::error::Error for PlacementError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DemolitionError {
    UnknownFacility(u64),
    LobbyIsPermanent,
    UnknownFloor(GridPosition),
    FloorOccupied(GridPosition),
    FloorSupportsAbove { position: GridPosition, above: i16 },
}

impl fmt::Display for DemolitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFacility(id) => write!(f, "facility {id} does not exist"),
            Self::LobbyIsPermanent => write!(f, "lobbies cannot be demolished"),
            Self::UnknownFloor(position) => write!(
                f,
                "there is no floor at level {}, column {}",
                position.floor, position.x
            ),
            Self::FloorOccupied(position) => write!(
                f,
                "remove the object above floor {}, column {} first",
                position.floor, position.x
            ),
            Self::FloorSupportsAbove { position, above } => write!(
                f,
                "floor {}, column {} is holding up floor {above}",
                position.floor, position.x
            ),
        }
    }
}

impl std::error::Error for DemolitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_supported_floor(tower: &mut Tower, floor: i16, columns: std::ops::Range<u16>) {
        for level in 1..=floor {
            for x in columns.clone() {
                tower.place_floor(GridPosition { x, floor: level }).unwrap();
            }
        }
    }

    fn add_rating_facility(tower: &mut Tower, kind: FacilityKind, id: u64) {
        tower.facilities.push(Facility {
            id,
            kind,
            position: GridPosition { x: 0, floor: 1 },
            occupancy: if kind.has_tenant_occupancy() {
                FacilityOccupancy::Occupied
            } else {
                FacilityOccupancy::NotApplicable
            },
            price_level: 1,
            stories: kind.spec().height as u8,
        });
    }

    #[test]
    fn construction_deducts_the_observed_price() {
        let mut tower = Tower::new(32, 100_000);
        build_supported_floor(&mut tower, 2, 0..9);
        let cash_before_office = tower.cash();
        tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 2 })
            .unwrap();
        assert_eq!(cash_before_office - tower.cash(), 40_000);
    }

    #[test]
    fn tenant_space_starts_vacant_then_moves_in_on_simulation_time() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..16);
        tower
            .place(FacilityKind::FastFood, GridPosition { x: 0, floor: 1 })
            .unwrap();
        assert!(!tower.facilities()[0].is_occupied());
        assert_eq!(tower.population(), 0);

        tower.advance_time(3.0);
        assert!(!tower.facilities()[0].is_occupied());
        tower.advance_time(10.0);
        assert!(tower.facilities()[0].is_occupied());
        assert_eq!(tower.population(), 48);
    }

    #[test]
    fn disconnected_tenant_stays_vacant_after_its_move_in_time() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..9);
        let office_id = tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 1 })
            .unwrap();

        tower.advance_time_with_connected_economy(30.0, &[], &[], Some(&[]));
        assert!(!tower.facilities()[0].is_occupied());

        tower.advance_time_with_connected_economy(0.0, &[], &[], Some(&[office_id]));
        assert!(tower.facilities()[0].is_occupied());
    }

    #[test]
    fn pausing_freezes_a_pending_tenant_move_in() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..16);
        tower
            .place(FacilityKind::FastFood, GridPosition { x: 0, floor: 1 })
            .unwrap();
        tower.clock.speed = SimulationSpeed::Paused;
        tower.advance_time(60.0);
        assert!(!tower.facilities()[0].is_occupied());
        assert_eq!(tower.population(), 0);
    }

    #[test]
    fn tenfold_speed_advances_twenty_game_minutes_per_second() {
        let mut clock = Clock::default();
        clock.speed = SimulationSpeed::Tenfold;
        clock.advance(3.0);
        assert_eq!(clock.minute_of_day, 9 * 60);
    }

    #[test]
    fn tenant_income_starts_only_after_move_in() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..9);
        tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 1 })
            .unwrap();
        let after_construction = tower.cash();

        assert!(tower.advance_time(1.0).is_empty());
        assert_eq!(tower.cash(), after_construction);

        let events = tower.advance_time(20.0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, FacilityKind::Office);
        assert_eq!(events[0].amount, 10_000);
        assert_eq!(tower.cash(), after_construction + 10_000);
    }

    #[test]
    fn condo_sale_is_paid_once_when_occupied() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..16);
        tower
            .place(FacilityKind::Condo, GridPosition { x: 0, floor: 1 })
            .unwrap();
        let after_construction = tower.cash();

        let events = tower.advance_time(30.0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].amount, 150_000);
        let after_sale = tower.cash();
        assert_eq!(after_sale, after_construction + 150_000);
        tower.advance_time(60.0);
        assert_eq!(tower.cash(), after_sale);
    }

    #[test]
    fn occupied_office_pays_again_at_the_next_quarter_boundary() {
        let mut tower = Tower::new(32, 1_000_000);
        add_rating_facility(&mut tower, FacilityKind::Office, 1);
        tower.clock.day = 4;
        tower.clock.minute_of_day = 4 * 60 + 59;
        let before = tower.cash();

        let events = tower.advance_time(0.5);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].amount, 10_000);
        assert_eq!(tower.cash(), before + 10_000);
    }

    #[test]
    fn occupied_shop_pays_once_per_quarter_instead_of_daily() {
        let mut tower = Tower::new(32, 1_000_000);
        add_rating_facility(&mut tower, FacilityKind::Shop, 1);
        tower.clock.day = 2;
        tower.clock.minute_of_day = 4 * 60 + 59;
        let before = tower.cash();

        let events = tower.advance_time(0.5);
        assert!(events.is_empty());
        assert_eq!(tower.cash(), before);

        tower.clock.day = 4;
        tower.clock.minute_of_day = 4 * 60 + 59;
        let events = tower.advance_time(0.5);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, FacilityKind::Shop);
        assert_eq!(events[0].amount, 15_000);
        assert_eq!(tower.cash(), before + 15_000);
    }

    #[test]
    fn vacant_tenant_never_pays_scheduled_income() {
        let mut tower = Tower::new(32, 1_000_000);
        tower.facilities.push(Facility {
            id: 1,
            kind: FacilityKind::Office,
            position: GridPosition { x: 0, floor: 1 },
            occupancy: FacilityOccupancy::VacantUntil(u64::MAX),
            price_level: 3,
            stories: 1,
        });
        tower.clock.day = 4;
        tower.clock.minute_of_day = 4 * 60 + 59;
        let before = tower.cash();

        assert!(tower.advance_time(0.5).is_empty());
        assert_eq!(tower.cash(), before);
    }

    #[test]
    fn every_revenue_tenant_has_an_income_rule() {
        for kind in [
            FacilityKind::Office,
            FacilityKind::HotelSingle,
            FacilityKind::HotelTwin,
            FacilityKind::HotelSuite,
            FacilityKind::FastFood,
            FacilityKind::Restaurant,
            FacilityKind::Shop,
            FacilityKind::Cinema,
            FacilityKind::PartyHall,
        ] {
            assert!(
                recurring_income(kind, 1, 100) > 0,
                "missing income for {kind:?}"
            );
        }
        assert_eq!(move_in_income(FacilityKind::Condo, 1), Some(150_000));
        assert_eq!(recurring_income(FacilityKind::Condo, 1, 0), 0);
    }

    #[test]
    fn commercial_receipts_follow_the_documented_patronage_bands() {
        assert_eq!(recurring_income(FacilityKind::FastFood, 1, 24), -3_000);
        assert_eq!(recurring_income(FacilityKind::FastFood, 1, 25), 3_000);
        assert_eq!(recurring_income(FacilityKind::FastFood, 1, 35), 3_000);
        assert_eq!(recurring_income(FacilityKind::FastFood, 1, 40), 5_000);
        assert_eq!(recurring_income(FacilityKind::Restaurant, 1, 24), 4_000);
        assert_eq!(recurring_income(FacilityKind::Restaurant, 1, 25), 6_000);
        assert_eq!(recurring_income(FacilityKind::Restaurant, 1, 70), 10_000);
        assert_eq!(recurring_income(FacilityKind::Cinema, 1, 39), 0);
        assert_eq!(recurring_income(FacilityKind::Cinema, 1, 80), 10_000);
    }

    #[test]
    fn original_operating_costs_are_expenses() {
        assert_eq!(maintenance_cost(FacilityKind::Lobby, 2), 0);
        assert_eq!(maintenance_cost(FacilityKind::Lobby, 3), 300);
        assert_eq!(maintenance_cost(FacilityKind::Lobby, 4), 1_000);
        assert_eq!(maintenance_cost(FacilityKind::Housekeeping, 3), 10_000);
        assert_eq!(maintenance_cost(FacilityKind::Security, 3), 20_000);
        assert_eq!(maintenance_cost(FacilityKind::Recycling, 3), 50_000);
        assert_eq!(maintenance_cost(FacilityKind::Metro, 5), 100_000);
        assert_eq!(
            elevator_car_maintenance_cost(FacilityKind::Elevator),
            10_000
        );
        assert_eq!(
            elevator_car_maintenance_cost(FacilityKind::ExpressElevator),
            20_000
        );
        assert_eq!(
            elevator_shaft_maintenance_cost(FacilityKind::Elevator),
            10_000
        );
    }

    #[test]
    fn elevator_maintenance_is_charged_per_shaft_and_car_not_per_floor() {
        let mut tower = Tower::new(32, 1_000_000);
        add_rating_facility(&mut tower, FacilityKind::Elevator, 7);
        let before = tower.cash();

        let events =
            tower.advance_time_with_economy(2_071.0, &[], &[(7, FacilityKind::Elevator, 2)]);

        assert_eq!(tower.cash(), before - 30_000);
        assert!(events.iter().any(|event| event.facility_id == 7
            && event.kind == FacilityKind::Elevator
            && event.amount == -30_000));
    }

    #[test]
    fn original_facility_limits_are_enforced() {
        let mut tower = Tower::new(64, 100_000_000);
        for id in 1..=MAX_SECURITY_OFFICES {
            add_rating_facility(&mut tower, FacilityKind::Security, id as u64);
        }
        for x in 0..16 {
            tower.place_floor(GridPosition { x, floor: -1 }).unwrap();
        }
        assert_eq!(
            tower.place(FacilityKind::Security, GridPosition { x: 0, floor: -1 }),
            Err(PlacementError::FacilityLimitReached {
                label: "security offices",
                limit: MAX_SECURITY_OFFICES,
            })
        );

        let mut metro_tower = Tower::new(64, 100_000_000);
        add_rating_facility(&mut metro_tower, FacilityKind::Metro, 1);
        for x in 0..30 {
            metro_tower
                .place_floor(GridPosition { x, floor: -1 })
                .unwrap();
        }
        assert_eq!(
            metro_tower.place(FacilityKind::Metro, GridPosition { x: 0, floor: -1 }),
            Err(PlacementError::FacilityLimitReached {
                label: "Metro station",
                limit: 1,
            })
        );
    }

    #[test]
    fn default_site_is_wide_enough_for_horizontal_expansion() {
        assert_eq!(Tower::default().width(), 256);
    }

    #[test]
    fn rating_uses_original_population_capacity() {
        let mut tower = Tower::new(128, 10_000_000);
        build_supported_floor(&mut tower, 1, 0..128);
        assert_eq!(tower.star_rating(), 1);
        for x in [0, 16, 32, 48, 64, 80, 96] {
            tower
                .place(FacilityKind::FastFood, GridPosition { x, floor: 1 })
                .unwrap();
        }
        tower.advance_time(30.0);
        assert_eq!(tower.population(), 336);
        assert_eq!(tower.star_rating(), 2);
    }

    #[test]
    fn three_stars_require_both_population_and_security() {
        let mut tower = Tower::new(256, 100_000_000);
        for id in 1..=21 {
            add_rating_facility(&mut tower, FacilityKind::FastFood, id);
        }
        assert_eq!(tower.population(), 1_008);
        assert_eq!(tower.star_rating(), 2);
        add_rating_facility(&mut tower, FacilityKind::Security, 22);
        assert_eq!(tower.star_rating(), 3);
    }

    #[test]
    fn four_and_five_stars_require_the_original_milestones() {
        let mut tower = Tower::new(256, 100_000_000);
        for id in 1..=84 {
            add_rating_facility(&mut tower, FacilityKind::Cinema, id);
        }
        assert!(tower.population() >= 10_000);
        add_rating_facility(&mut tower, FacilityKind::Security, 85);
        assert_eq!(tower.star_rating(), 3);

        for (id, kind) in [
            (86, FacilityKind::HotelSuite),
            (87, FacilityKind::HotelSuite),
            (88, FacilityKind::Medical),
            (89, FacilityKind::Recycling),
            (90, FacilityKind::Parking),
        ] {
            add_rating_facility(&mut tower, kind, id);
        }
        assert_eq!(tower.star_rating(), 4);

        add_rating_facility(&mut tower, FacilityKind::Metro, 91);
        assert_eq!(tower.star_rating(), 5);
    }

    #[test]
    fn facilities_cannot_overlap() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 2, 4..29);
        tower
            .place(FacilityKind::Office, GridPosition { x: 4, floor: 2 })
            .unwrap();
        let result = tower.place(FacilityKind::Security, GridPosition { x: 12, floor: 2 });
        assert_eq!(result, Err(PlacementError::Occupied));
    }

    #[test]
    fn adjacency_is_not_an_overlap() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 2, 0..29);
        tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 2 })
            .unwrap();
        assert!(
            tower
                .place(FacilityKind::Security, GridPosition { x: 9, floor: 2 },)
                .is_ok()
        );
    }

    #[test]
    fn lobbies_are_restricted_to_first_and_fifteenth_floors() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 15, 0..1);
        assert!(
            tower
                .can_place(FacilityKind::Lobby, GridPosition { x: 0, floor: 1 },)
                .is_ok()
        );
        assert!(
            tower
                .can_place(FacilityKind::Lobby, GridPosition { x: 0, floor: 15 },)
                .is_ok()
        );
        assert!(matches!(
            tower.can_place(FacilityKind::Lobby, GridPosition { x: 0, floor: 14 },),
            Err(PlacementError::InvalidLobbyFloor { floor: 14 })
        ));
    }

    #[test]
    fn click_placed_lobby_is_one_structural_slice() {
        let mut tower = Tower::new(32, 1_000_000);
        tower.place_floor(GridPosition { x: 3, floor: 1 }).unwrap();
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 3, floor: 1 })
            .unwrap();
        let lobby = &tower.facilities()[0];
        assert_eq!(lobby.kind, FacilityKind::Lobby);
        assert_eq!(lobby.end_x() - lobby.position.x, 1);
    }

    #[test]
    fn original_lobby_segments_cost_1250_and_cannot_be_demolished() {
        let mut tower = Tower::new(16, 10_000);
        tower.place_floor(GridPosition { x: 4, floor: 1 }).unwrap();
        let before = tower.cash();
        let lobby = tower
            .place(FacilityKind::Lobby, GridPosition { x: 4, floor: 1 })
            .unwrap();
        assert_eq!(tower.cash(), before - 1_250);
        assert_eq!(
            tower.demolish(lobby),
            Err(DemolitionError::LobbyIsPermanent)
        );
    }

    #[test]
    fn control_lobbies_use_two_or_three_original_story_layers() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 3, 0..9);
        let first = tower
            .place_lobby(GridPosition { x: 0, floor: 1 }, 3)
            .unwrap();
        tower
            .place_lobby(GridPosition { x: 1, floor: 1 }, 3)
            .unwrap();
        let lobby = tower
            .facilities()
            .iter()
            .find(|facility| facility.id == first)
            .unwrap();
        assert_eq!(lobby.height(), 3);
        assert_eq!(lobby.stories, 3);
        assert_eq!(
            tower.cash(),
            1_000_000 - 27 * FLOOR_CONSTRUCTION_COST - 7_500
        );
        assert_eq!(
            tower.place(FacilityKind::Office, GridPosition { x: 0, floor: 2 }),
            Err(PlacementError::Occupied)
        );
    }

    #[test]
    fn super_lobby_must_be_the_first_lobby_and_have_upper_floor_space() {
        let mut tower = Tower::new(32, 1_000_000);
        for x in 0..4 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        assert_eq!(
            tower.place_lobby(GridPosition { x: 0, floor: 1 }, 2),
            Err(PlacementError::MissingSuperLobbyFloor { floor: 2 })
        );
        for x in 0..4 {
            tower.place_floor(GridPosition { x, floor: 2 }).unwrap();
        }
        tower
            .place_lobby(GridPosition { x: 0, floor: 1 }, 1)
            .unwrap();
        assert_eq!(
            tower.place_lobby(GridPosition { x: 1, floor: 1 }, 2),
            Err(PlacementError::SuperLobbyMustBeFirst)
        );
    }

    #[test]
    fn lower_left_lobby_secret_doubles_fresh_tower_funds_only_once() {
        let mut tower = Tower::default();
        tower.place_floor(GridPosition { x: 0, floor: 1 }).unwrap();
        assert!(tower.activate_starting_cash_cheat());
        assert_eq!(tower.cash(), 4_000_000);
        assert!(!tower.activate_starting_cash_cheat());
        assert_eq!(tower.cash(), 4_000_000);
    }

    #[test]
    fn underground_construction_can_reveal_the_one_time_treasure() {
        let mut tower = Tower::new(64, 2_000_000);
        for id in 1..=50 {
            add_rating_facility(&mut tower, FacilityKind::Office, id);
        }
        assert_eq!(tower.star_rating(), 2);
        let start = tower.treasure_position.x.saturating_sub(8);
        for x in start..start + 16 {
            tower.place_floor(GridPosition { x, floor: -3 }).unwrap();
        }
        let before = tower.cash();
        tower
            .place(
                FacilityKind::Security,
                GridPosition {
                    x: start,
                    floor: -3,
                },
            )
            .unwrap();
        let discovery = tower.take_secret_discovery().unwrap();
        assert_eq!(discovery.amount, 200_000);
        assert_eq!(tower.cash(), before - 100_000 + 200_000);
        assert!(tower.take_secret_discovery().is_none());
    }

    #[test]
    fn elevator_shaft_extensions_and_extra_cars_have_distinct_prices() {
        let mut tower = Tower::new(16, 1_000_000);
        for x in 0..4 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
            tower.place_floor(GridPosition { x, floor: 2 }).unwrap();
            tower.place_floor(GridPosition { x, floor: 3 }).unwrap();
        }

        let before_shaft = tower.cash();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 0, floor: 1 })
            .unwrap();
        assert_eq!(tower.cash(), before_shaft - 200_000);

        let before_extension = tower.cash();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 0, floor: 2 })
            .unwrap();
        assert_eq!(tower.cash(), before_extension);

        let before_extension_and_car = tower.cash();
        tower
            .place_elevator_extension_with_car(
                FacilityKind::Elevator,
                GridPosition { x: 0, floor: 3 },
            )
            .unwrap();
        assert_eq!(tower.cash(), before_extension_and_car - 80_000);

        assert_eq!(elevator_car_cost(FacilityKind::Elevator), 80_000);
        assert_eq!(elevator_car_cost(FacilityKind::ServiceElevator), 50_000);
        assert_eq!(elevator_car_cost(FacilityKind::ExpressElevator), 150_000);
    }

    #[test]
    fn facilities_require_empty_floor_under_their_entire_footprint() {
        let mut tower = Tower::new(32, 1_000_000);
        build_supported_floor(&mut tower, 2, 0..8);
        assert_eq!(
            tower.place(FacilityKind::Office, GridPosition { x: 0, floor: 2 }),
            Err(PlacementError::MissingFloor)
        );

        tower.place_floor(GridPosition { x: 8, floor: 1 }).unwrap();
        tower.place_floor(GridPosition { x: 8, floor: 2 }).unwrap();
        assert!(
            tower
                .place(FacilityKind::Office, GridPosition { x: 0, floor: 2 })
                .is_ok()
        );
    }

    #[test]
    fn floor_is_a_separate_paintable_layer() {
        let mut tower = Tower::new(8, 10_000);
        let position = GridPosition { x: 2, floor: 1 };
        tower.place_floor(position).unwrap();

        assert!(tower.has_floor(position));
        assert_eq!(tower.facilities().len(), 0);
        assert_eq!(tower.cash(), 10_000 - FLOOR_CONSTRUCTION_COST);
        assert_eq!(
            tower.place_floor(position),
            Err(PlacementError::FloorAlreadyExists)
        );
    }

    #[test]
    fn demolition_removes_clear_floor_cells_but_not_structural_supports() {
        let mut tower = Tower::new(8, 1_000_000);
        tower.place_floor(GridPosition { x: 2, floor: 1 }).unwrap();
        tower.place_floor(GridPosition { x: 2, floor: 2 }).unwrap();

        assert_eq!(
            tower.demolish_floor(GridPosition { x: 2, floor: 1 }),
            Err(DemolitionError::FloorSupportsAbove {
                position: GridPosition { x: 2, floor: 1 },
                above: 2,
            })
        );
        tower
            .demolish_floor(GridPosition { x: 2, floor: 2 })
            .unwrap();
        tower
            .demolish_floor(GridPosition { x: 2, floor: 1 })
            .unwrap();
        assert!(!tower.has_floor(GridPosition { x: 2, floor: 1 }));
    }

    #[test]
    fn demolition_requires_objects_to_be_removed_before_their_floor() {
        let mut tower = Tower::new(16, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..9);
        let office = tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 1 })
            .unwrap();
        assert!(matches!(
            tower.demolish_floor(GridPosition { x: 4, floor: 1 }),
            Err(DemolitionError::FloorOccupied(_))
        ));
        tower.demolish(office).unwrap();
        tower
            .demolish_floor(GridPosition { x: 4, floor: 1 })
            .unwrap();
    }

    #[test]
    fn connecting_floor_cannot_be_removed_beneath_stairs() {
        let mut tower = Tower::new(8, 1_000_000);
        build_supported_floor(&mut tower, 2, 0..8);
        tower
            .place(FacilityKind::Stairs, GridPosition { x: 0, floor: 1 })
            .unwrap();
        assert!(matches!(
            tower.demolish_floor(GridPosition { x: 3, floor: 2 }),
            Err(DemolitionError::FloorOccupied(_))
        ));
    }

    #[test]
    fn upper_floors_cannot_overhang_the_floor_below() {
        let mut tower = Tower::new(16, 100_000);
        for x in 4..12 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }

        assert!(tower.place_floor(GridPosition { x: 4, floor: 2 }).is_ok());
        assert!(tower.place_floor(GridPosition { x: 11, floor: 2 }).is_ok());
        assert_eq!(
            tower.place_floor(GridPosition { x: 3, floor: 2 }),
            Err(PlacementError::FloorWiderThanBelow { floor: 2, x: 3 })
        );
        assert_eq!(
            tower.place_floor(GridPosition { x: 12, floor: 2 }),
            Err(PlacementError::FloorWiderThanBelow { floor: 2, x: 12 })
        );
    }

    #[test]
    fn stairs_overlay_first_floor_but_require_the_floor_above() {
        let mut tower = Tower::new(16, 1_000_000);
        for x in 0..8 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 0, floor: 1 })
            .unwrap();
        assert_eq!(
            tower.place(FacilityKind::Stairs, GridPosition { x: 0, floor: 1 }),
            Err(PlacementError::MissingConnectingFloor {
                base_floor: 1,
                upper_floor: 2,
            })
        );

        for x in 0..8 {
            tower.place_floor(GridPosition { x, floor: 2 }).unwrap();
        }
        assert!(
            tower
                .place(FacilityKind::Stairs, GridPosition { x: 0, floor: 1 })
                .is_ok()
        );
    }

    #[test]
    fn basement_stairs_connect_toward_the_surface() {
        let mut tower = Tower::new(16, 1_000_000);
        for x in 0..8 {
            tower.place_floor(GridPosition { x, floor: -1 }).unwrap();
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        assert!(
            tower
                .place(FacilityKind::Stairs, GridPosition { x: 0, floor: -1 })
                .is_ok()
        );
    }

    #[test]
    fn burned_tenant_stays_unusable_until_demolished() {
        let mut tower = Tower::new(16, 1_000_000);
        build_supported_floor(&mut tower, 1, 0..9);
        let office = tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 1 })
            .unwrap();
        tower.facilities[0].occupancy = FacilityOccupancy::Occupied;
        assert_eq!(tower.population(), 6);

        assert!(tower.mark_facility_burned(office));
        assert!(tower.facilities()[0].is_burned());
        assert_eq!(tower.population(), 0);
        tower.advance_time(100.0);
        assert!(tower.facilities()[0].is_burned());

        tower.demolish(office).unwrap();
        assert!(tower.facilities().is_empty());
    }

    #[test]
    fn emergency_charge_never_overdraws_the_tower() {
        let mut tower = Tower::new(8, 300_000);
        assert!(tower.try_spend(300_000));
        assert_eq!(tower.cash(), 0);
        assert!(!tower.try_spend(1));
        assert!(!tower.try_spend(-1));
        assert_eq!(tower.cash(), 0);
    }

    #[test]
    fn clock_wraps_at_midnight() {
        let mut clock = Clock {
            day: 3,
            minute_of_day: 23 * 60 + 59,
            speed: SimulationSpeed::Normal,
            ..Clock::default()
        };
        clock.advance(1.0);
        assert_eq!(clock.day, 4);
        assert_eq!(clock.minute_of_day, 1);
    }

    #[test]
    fn clock_accumulates_frame_sized_steps() {
        let mut clock = Clock::default();
        for _ in 0..30 {
            clock.advance(1.0 / 60.0);
        }
        assert_eq!(clock.minute_of_day, 8 * 60 + 1);
    }
}
