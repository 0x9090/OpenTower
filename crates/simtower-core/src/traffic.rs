use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::{Clock, Facility, FacilityKind, GridPosition, Tower};

pub const ELEVATOR_CAPACITY: usize = 17;
pub const EXPRESS_ELEVATOR_CAPACITY: usize = 34;
pub const MAX_ELEVATOR_CARS: usize = 8;
pub const MAX_SIMULATED_PEOPLE: usize = 160;
pub const MAX_ELEVATOR_QUEUE_PER_FLOOR: usize = 12;
pub const CONCERNED_WAIT_SECONDS: f32 = 10.0;
pub const ANGRY_WAIT_SECONDS: f32 = 20.0;
pub const ABANDON_ELEVATOR_WAIT_SECONDS: f32 = 45.0;
/// The original game treats stairs as short-range transport. Beyond four
/// flights tenants expect an elevator connection.
pub const MAX_STAIR_FLIGHTS: u16 = 4;
/// Escalators are useful for only three consecutive flights in either
/// direction before their route is considered impractical.
pub const MAX_ESCALATOR_FLIGHTS: u16 = 3;
const QUEUE_ABANDON_WALK_SECONDS: f32 = 4.0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PersonMood {
    Calm,
    Concerned,
    Angry,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PersonActivity {
    Walking,
    WaitingForElevator { shaft_id: u64 },
    RidingElevator { shaft_id: u64 },
    UsingStairs,
    Visiting,
    LeavingElevatorQueue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ElevatorDirection {
    Idle,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ElevatorMode {
    Local,
    ExpressToTop,
    ExpressToBottom,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ElevatorSchedule {
    pub mode: ElevatorMode,
    pub waiting_car_response: u8,
    pub departure_seconds: u8,
}

impl Default for ElevatorSchedule {
    fn default() -> Self {
        Self {
            mode: ElevatorMode::Local,
            waiting_car_response: 5,
            departure_seconds: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElevatorCar {
    pub floor_position: f32,
    pub home_floor: i16,
    pub direction: ElevatorDirection,
    pub target_floor: Option<i16>,
    pub passengers: Vec<u64>,
    dwell_remaining: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElevatorShaft {
    pub id: u64,
    pub kind: FacilityKind,
    pub x: u16,
    pub served_floors: Vec<i16>,
    pub cars: Vec<ElevatorCar>,
    pub schedules: [ElevatorSchedule; 12],
    pub visible: bool,
}

impl ElevatorShaft {
    pub fn current_schedule(&self, clock: Clock) -> ElevatorSchedule {
        self.schedules[schedule_index(clock)]
    }

    pub fn current_schedule_mut(&mut self, clock: Clock) -> &mut ElevatorSchedule {
        &mut self.schedules[schedule_index(clock)]
    }

    pub fn waiting_count(&self, people: &[Person]) -> usize {
        people
            .iter()
            .filter(|person| {
                matches!(
                    person.activity,
                    PersonActivity::WaitingForElevator { shaft_id } if shaft_id == self.id
                )
            })
            .count()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Person {
    pub id: u64,
    /// The original simulation distinguishes residents and staff by role.
    /// Keeping that identity on the lightweight active traveler lets the UI
    /// select the correct artwork without creating a permanent object for
    /// every member of a large tower's population.
    #[serde(default)]
    pub role: PersonRole,
    pub home_facility_id: u64,
    pub destination_facility_id: u64,
    pub x: f32,
    /// Continuous vertical position in display-floor ordinals. B1 is -1,
    /// floor 1 is 0, floor 2 is 1, and so on.
    pub floor_position: f32,
    pub current_floor: i16,
    pub mood: PersonMood,
    pub activity: PersonActivity,
    pub wait_seconds: f32,
    origin: Endpoint,
    destination: Endpoint,
    route: TravelMode,
    going_home: bool,
    walking_from_transport: bool,
    visit_remaining: f32,
    visit_duration: f32,
    stair_progress: f32,
    route_leg: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum PersonRole {
    #[default]
    Visitor,
    Salesman,
    Woman,
    Child,
    Security,
    WomanWithChild,
    Homebody,
    Housekeeper,
}

impl PersonRole {
    const fn is_staff(self) -> bool {
        matches!(self, Self::Security | Self::Housekeeper)
    }
}

impl Person {
    pub const fn is_returning_home(&self) -> bool {
        self.going_home
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
struct Endpoint {
    facility_id: u64,
    x: f32,
    floor: i16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
enum TravelMode {
    SameFloor,
    Elevator {
        shaft_id: u64,
        x: f32,
    },
    ElevatorTransfer {
        first_id: u64,
        first_x: f32,
        transfer_floor: i16,
        second_id: u64,
        second_x: f32,
    },
    Stairs {
        x: f32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ElevatorAccess {
    Public,
    Housekeeping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TopologySync {
    Full,
    Additive,
    RemovedFloor(GridPosition),
    RemovedWalkway {
        kind: FacilityKind,
        x: u16,
        floor: i16,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrafficSimulation {
    people: Vec<Person>,
    elevators: Vec<ElevatorShaft>,
    arrived_facilities: HashSet<u64>,
    #[serde(default)]
    reachable_facilities: HashSet<u64>,
    #[serde(default)]
    routes_from_lobby: HashMap<u64, TravelMode>,
    #[serde(default)]
    connected_parking: HashSet<u64>,
    #[serde(default)]
    topology_signature: u64,
    #[serde(default)]
    known_destinations: HashSet<u64>,
    #[serde(default)]
    current_visitors: HashMap<u64, usize>,
    #[serde(default)]
    daily_visits: HashMap<u64, u32>,
    #[serde(default)]
    patronage_day: u32,
    /// One compact day stamp per condo/role replaces persistent resident
    /// agents. Only residents who are currently making a trip consume one of
    /// the capped detailed-person slots.
    #[serde(default)]
    #[serde(alias = "resident_trip_days")]
    scheduled_trip_days: HashMap<u64, u32>,
    /// Long-lived, low-cost commercial demand. This grows independently of
    /// the capped set of detailed people so a large tower does not lose its
    /// economy merely because all visible-agent slots are occupied.
    #[serde(default)]
    commercial_patronage: HashMap<u64, u32>,
    #[serde(default)]
    last_resident_dispatch_bucket: u64,
    #[serde(default)]
    scheduled_home_cursor: usize,
    next_person_id: u64,
    spawn_accumulator: f32,
    next_spawn_seconds: f32,
}

impl Default for TrafficSimulation {
    fn default() -> Self {
        Self {
            people: Vec::new(),
            elevators: Vec::new(),
            arrived_facilities: HashSet::new(),
            reachable_facilities: HashSet::new(),
            routes_from_lobby: HashMap::new(),
            connected_parking: HashSet::new(),
            topology_signature: 0,
            known_destinations: HashSet::new(),
            current_visitors: HashMap::new(),
            daily_visits: HashMap::new(),
            patronage_day: 0,
            scheduled_trip_days: HashMap::new(),
            commercial_patronage: HashMap::new(),
            last_resident_dispatch_bucket: 0,
            scheduled_home_cursor: 0,
            next_person_id: 1,
            spawn_accumulator: 0.0,
            next_spawn_seconds: 1.0,
        }
    }
}

impl TrafficSimulation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn people(&self) -> &[Person] {
        &self.people
    }

    pub fn elevators(&self) -> &[ElevatorShaft] {
        &self.elevators
    }

    pub fn validate_save_state(&self, tower: &Tower) -> Result<(), String> {
        if self.people.len() > MAX_SIMULATED_PEOPLE
            || !self.spawn_accumulator.is_finite()
            || self.spawn_accumulator < 0.0
            || !self.next_spawn_seconds.is_finite()
            || self.next_spawn_seconds <= 0.0
        {
            return Err("save contains invalid traffic timing data".to_owned());
        }
        let facility_ids = tower
            .facilities()
            .iter()
            .map(|facility| facility.id)
            .collect::<HashSet<_>>();
        let shaft_ids = self
            .elevators
            .iter()
            .map(|shaft| shaft.id)
            .collect::<HashSet<_>>();
        for shaft in &self.elevators {
            if !matches!(
                shaft.kind,
                FacilityKind::Elevator
                    | FacilityKind::ServiceElevator
                    | FacilityKind::ExpressElevator
            ) || shaft.cars.is_empty()
                || shaft.cars.len() > MAX_ELEVATOR_CARS
                || shaft.x >= tower.width()
                || shaft.served_floors.is_empty()
                || shaft
                    .served_floors
                    .iter()
                    .any(|floor| !(crate::MIN_FLOOR..=crate::MAX_FLOOR).contains(floor))
                || shaft.cars.iter().any(|car| {
                    !car.floor_position.is_finite()
                        || !shaft.served_floors.contains(&car.home_floor)
                })
            {
                return Err("save contains invalid elevator state".to_owned());
            }
        }
        let mut person_ids = HashSet::new();
        for person in &self.people {
            if person.id == 0
                || !person_ids.insert(person.id)
                || !person.x.is_finite()
                || !person.floor_position.is_finite()
                || !person.wait_seconds.is_finite()
                || (person.home_facility_id != 0
                    && !facility_ids.contains(&person.home_facility_id))
                || (person.destination_facility_id != 0
                    && !facility_ids.contains(&person.destination_facility_id))
                || match person.activity {
                    PersonActivity::WaitingForElevator { shaft_id }
                    | PersonActivity::RidingElevator { shaft_id } => !shaft_ids.contains(&shaft_id),
                    _ => false,
                }
            {
                return Err("save contains invalid person state".to_owned());
            }
        }
        Ok(())
    }

    /// Number of simulated people who have actually reached this facility and
    /// are currently inside it. People walking toward it, waiting for
    /// transport, or riding an elevator are deliberately not counted.
    pub fn visitors_at(&self, facility_id: u64) -> usize {
        self.current_visitors
            .get(&facility_id)
            .copied()
            .unwrap_or(0)
    }

    /// True after at least one simulated person has completed the journey
    /// from outside, through a first-floor lobby, to this tenant.
    pub fn has_received_visitor(&self, facility_id: u64) -> bool {
        self.arrived_facilities.contains(&facility_id)
    }

    /// People who completed a trip into this tenant during the current game
    /// day. Economic settlement uses daily patronage, while `visitors_at`
    /// remains the number physically inside for crowd artwork.
    pub fn daily_visitors_at(&self, facility_id: u64) -> u32 {
        self.daily_visits.get(&facility_id).copied().unwrap_or(0)
    }

    /// Stable patronage generated by a commercial tenant's own advertising
    /// and reputation. Weekend multipliers match the original traffic study;
    /// detailed travelers are layered on top by the desktop economy.
    pub fn dedicated_patronage_at(
        &self,
        facility_id: u64,
        kind: FacilityKind,
        weekend: bool,
    ) -> u32 {
        let base = self
            .commercial_patronage
            .get(&facility_id)
            .copied()
            .unwrap_or_else(|| commercial_patronage_initial(kind));
        match kind {
            FacilityKind::FastFood | FacilityKind::Restaurant if weekend => {
                base.saturating_mul(48).div_ceil(35).min(48)
            }
            FacilityKind::Shop if weekend => base.saturating_mul(30).div_ceil(25).min(30),
            _ => base,
        }
    }

    /// Starts a new cinema program. New releases draw a large opening crowd,
    /// then settle toward the cinema's twenty-person baseline over time.
    pub fn change_cinema_program(&mut self, facility_id: u64) -> bool {
        self.commercial_patronage.insert(facility_id, 120);
        true
    }

    /// Evaluation penalty for the final walk between transport and a tenant.
    /// Express service is exempt because its purpose is to bridge large tower
    /// sections; ordinary tenants progressively dislike long corridors.
    pub fn access_distance_penalty(&self, tower: &Tower, facility_id: u64) -> u8 {
        let Some(facility) = tower
            .facilities()
            .iter()
            .find(|facility| facility.id == facility_id)
        else {
            return 0;
        };
        let Some(route) = self.routes_from_lobby.get(&facility_id).copied() else {
            return 0;
        };
        let uses_express = match route {
            TravelMode::Elevator { shaft_id, .. } => self
                .elevators
                .iter()
                .any(|shaft| shaft.id == shaft_id && shaft.kind == FacilityKind::ExpressElevator),
            TravelMode::ElevatorTransfer {
                first_id,
                second_id,
                ..
            } => self.elevators.iter().any(|shaft| {
                (shaft.id == first_id || shaft.id == second_id)
                    && shaft.kind == FacilityKind::ExpressElevator
            }),
            _ => false,
        };
        if uses_express {
            return 0;
        }

        let destination_x = endpoint(facility).x;
        let transport_x = match route {
            TravelMode::Elevator { x, .. } | TravelMode::Stairs { x } => Some(x),
            TravelMode::ElevatorTransfer { second_x, .. } => Some(second_x),
            TravelMode::SameFloor => tower
                .facilities()
                .iter()
                .filter(|candidate| {
                    candidate.kind == FacilityKind::Lobby
                        && candidate.position.floor == facility.position.floor
                })
                .map(|candidate| endpoint(candidate).x)
                .min_by(|left, right| {
                    (destination_x - *left)
                        .abs()
                        .total_cmp(&(destination_x - *right).abs())
                }),
        };
        let Some(transport_x) = transport_x else {
            return 0;
        };
        match (destination_x - transport_x).abs() {
            distance if distance > 32.0 => 24,
            distance if distance > 24.0 => 16,
            distance if distance > 16.0 => 8,
            _ => 0,
        }
    }

    /// Sends everyone currently inside a facility back toward the entrance.
    /// Emergency events call this every simulation tick, which also prevents
    /// a late arrival from remaining inside while the facility is unsafe.
    pub fn evacuate_facility(&mut self, facility_id: u64) {
        for person in self.people.iter_mut().filter(|person| {
            person.destination_facility_id == facility_id
                && !person.going_home
                && person.activity == PersonActivity::Visiting
        }) {
            person.going_home = true;
            person.current_floor = person.destination.floor;
            person.floor_position = f32::from(floor_ordinal(person.current_floor));
            person.route = reverse_route(person.route);
            person.route_leg = 0;
            person.walking_from_transport = person.route == TravelMode::SameFloor;
            person.activity = PersonActivity::Walking;
            person.visit_remaining = 0.0;
        }
        self.refresh_current_visitors();
    }

    /// Adds a distinguished outside visitor to one specific tenant while
    /// still using the same route, elevator queues, stress, and exit logic as
    /// every ordinary Sim. The desktop uses this for the four-star VIP visit.
    pub fn spawn_special_visitor(
        &mut self,
        tower: &Tower,
        facility_id: u64,
        visit_seconds: f32,
    ) -> Option<u64> {
        if self.people.len() >= MAX_SIMULATED_PEOPLE {
            return None;
        }
        let facility = tower
            .facilities()
            .iter()
            .find(|facility| facility.id == facility_id && facility.is_occupied())?;
        let route = self.routes_from_lobby.get(&facility_id).copied()?;
        let seed = mix64(self.next_person_id ^ (u64::from(tower.clock.day) << 32));
        let origin = Endpoint {
            facility_id: 0,
            x: exterior_spawn_x(tower, seed),
            floor: 1,
        };
        let destination = endpoint(facility);
        let person_id = self.next_person_id;
        let mut person = Person {
            id: person_id,
            role: PersonRole::Visitor,
            home_facility_id: 0,
            destination_facility_id: facility_id,
            x: origin.x,
            floor_position: f32::from(floor_ordinal(1)),
            current_floor: 1,
            mood: PersonMood::Calm,
            activity: PersonActivity::Walking,
            wait_seconds: 0.0,
            origin,
            destination,
            route,
            going_home: false,
            walking_from_transport: route == TravelMode::SameFloor,
            visit_remaining: 0.0,
            visit_duration: visit_seconds.max(1.0),
            stair_progress: 0.0,
            route_leg: 0,
        };
        start_trip(&mut person);
        self.next_person_id += 1;
        self.people.push(person);
        Some(person_id)
    }

    /// Tenant ids having a complete pedestrian/transport route from a
    /// first-floor lobby. The economy uses this to hold disconnected spaces
    /// vacant even after their normal move-in delay expires.
    pub fn reachable_facility_ids(&self, tower: &Tower) -> Vec<u64> {
        debug_assert_eq!(self.topology_signature, tower_topology_signature(tower));
        self.reachable_facilities.iter().copied().collect()
    }

    /// Cached tenant reachability. `sync_with_tower` refreshes this only when
    /// floors or transport topology change; querying it is constant time.
    pub fn reachable_facility_set(&self) -> &HashSet<u64> {
        &self.reachable_facilities
    }

    /// Whether a housekeeper can walk from a housekeeping office to a room.
    /// Cross-floor routes deliberately consider service elevators only; this
    /// keeps housekeeping staff out of passenger lifts and escalators.
    pub fn housekeeping_route_available(
        &self,
        tower: &Tower,
        housekeeping_id: u64,
        room_id: u64,
    ) -> bool {
        let Some(origin) = tower.facilities().iter().find(|facility| {
            facility.id == housekeeping_id && facility.kind == FacilityKind::Housekeeping
        }) else {
            return false;
        };
        let Some(destination) = tower.facilities().iter().find(|facility| {
            facility.id == room_id
                && matches!(
                    facility.kind,
                    FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite
                )
        }) else {
            return false;
        };
        choose_route_scored_for(
            endpoint(origin),
            endpoint(destination),
            &self.elevators,
            tower.facilities(),
            tower.floors(),
            ElevatorAccess::Housekeeping,
        )
        .is_some()
    }

    /// Dispatches one of the six workers assigned to a housekeeping office.
    /// The worker follows the same walking and elevator state machine as every
    /// other person, but cross-floor trips are restricted to service lifts.
    pub fn dispatch_housekeeper(
        &mut self,
        tower: &Tower,
        housekeeping_id: u64,
        room_id: u64,
        cleaning_seconds: f32,
    ) -> Option<u64> {
        if self.people.len() >= MAX_SIMULATED_PEOPLE
            || self
                .people
                .iter()
                .filter(|person| person.home_facility_id == housekeeping_id)
                .count()
                >= 6
            || self.people.iter().any(|person| {
                person.destination_facility_id == room_id && person.home_facility_id != 0
            })
        {
            return None;
        }
        let origin = tower.facilities().iter().find(|facility| {
            facility.id == housekeeping_id && facility.kind == FacilityKind::Housekeeping
        })?;
        let destination = tower.facilities().iter().find(|facility| {
            facility.id == room_id
                && matches!(
                    facility.kind,
                    FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite
                )
        })?;
        let origin = endpoint(origin);
        let destination = endpoint(destination);
        let route = choose_route_scored_for(
            origin,
            destination,
            &self.elevators,
            tower.facilities(),
            tower.floors(),
            ElevatorAccess::Housekeeping,
        )?
        .0;
        let person_id = self.next_person_id;
        let mut person = Person {
            id: person_id,
            role: PersonRole::Housekeeper,
            home_facility_id: housekeeping_id,
            destination_facility_id: room_id,
            x: origin.x,
            floor_position: f32::from(floor_ordinal(origin.floor)),
            current_floor: origin.floor,
            mood: PersonMood::Calm,
            activity: PersonActivity::Walking,
            wait_seconds: 0.0,
            origin,
            destination,
            route,
            going_home: false,
            walking_from_transport: route == TravelMode::SameFloor,
            visit_remaining: 0.0,
            visit_duration: cleaning_seconds.max(1.0),
            stair_progress: 0.0,
            route_leg: 0,
        };
        start_trip(&mut person);
        self.next_person_id += 1;
        self.people.push(person);
        Some(person_id)
    }

    pub fn elevator_mut(&mut self, shaft_id: u64) -> Option<&mut ElevatorShaft> {
        self.elevators.iter_mut().find(|shaft| shaft.id == shaft_id)
    }

    pub fn elevator_at(&self, kind: FacilityKind, x: u16) -> Option<&ElevatorShaft> {
        self.elevators
            .iter()
            .find(|shaft| shaft.kind == kind && shaft.x == x)
    }

    pub fn elevator_can_stop_at(&self, tower: &Tower, shaft_id: u64, floor: i16) -> bool {
        self.elevators
            .iter()
            .find(|shaft| shaft.id == shaft_id)
            .is_some_and(|shaft| shaft_stops_at(shaft, floor, tower.facilities()))
    }

    pub fn add_elevator_car(&mut self, tower: &Tower, shaft_id: u64, home_floor: i16) -> bool {
        if !self.elevator_can_stop_at(tower, shaft_id, home_floor) {
            return false;
        }
        let Some(shaft) = self.elevator_mut(shaft_id) else {
            return false;
        };
        if shaft.cars.len() >= MAX_ELEVATOR_CARS {
            return false;
        }
        shaft.cars.push(ElevatorCar {
            floor_position: f32::from(floor_ordinal(home_floor)),
            home_floor,
            direction: ElevatorDirection::Idle,
            target_floor: None,
            passengers: Vec::new(),
            dwell_remaining: 0.0,
        });
        true
    }

    pub fn sync_with_tower(&mut self, tower: &Tower) {
        self.sync_with_tower_internal(tower, TopologySync::Full);
    }

    /// Synchronizes after construction that only adds route infrastructure.
    /// Existing routes remain valid, so only newly placed or previously
    /// unreachable destinations need pathfinding. Destructive edits continue
    /// to use `sync_with_tower` and rebuild every route.
    pub fn sync_with_tower_after_addition(&mut self, tower: &Tower) {
        self.sync_with_tower_internal(tower, TopologySync::Additive);
    }

    /// Synchronizes after removing one facility. A stair or escalator flight
    /// can invalidate only routes that use its column and cross that flight,
    /// so preserve every unrelated cached tenant route instead of rebuilding
    /// the entire tower synchronously on the demolition click.
    pub fn sync_with_tower_after_removal(&mut self, tower: &Tower, removed: &Facility) {
        let mode = if matches!(removed.kind, FacilityKind::Stairs | FacilityKind::Escalator) {
            TopologySync::RemovedWalkway {
                kind: removed.kind,
                x: removed.position.x,
                floor: removed.position.floor,
            }
        } else {
            TopologySync::Full
        };
        self.sync_with_tower_internal(tower, mode);
    }

    /// Synchronizes after removing one structural floor cell. Floor removal
    /// cannot create a new route and does not alter elevator shafts, so only
    /// destinations whose final horizontal walk crossed that cell need fresh
    /// pathfinding.
    pub fn sync_with_tower_after_floor_removal(&mut self, tower: &Tower, removed: GridPosition) {
        self.sync_with_tower_internal(tower, TopologySync::RemovedFloor(removed));
    }

    fn sync_with_tower_internal(&mut self, tower: &Tower, mode: TopologySync) {
        let signature = tower_topology_signature(tower);
        let destinations = tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind.has_tenant_occupancy())
            .collect::<Vec<_>>();
        let destination_ids = destinations
            .iter()
            .map(|facility| facility.id)
            .collect::<HashSet<_>>();
        let topology_changed = self.topology_signature != signature;
        let destinations_changed = self.known_destinations != destination_ids;
        if !topology_changed && !destinations_changed {
            normalize_elevator_home_floors(&mut self.elevators, tower.facilities());
            return;
        }

        if topology_changed && !matches!(mode, TopologySync::RemovedFloor(_)) {
            let mut groups: Vec<(FacilityKind, u16, u64, Vec<i16>)> = Vec::new();
            for facility in tower.facilities().iter().filter(|facility| {
                matches!(
                    facility.kind,
                    FacilityKind::Elevator
                        | FacilityKind::ServiceElevator
                        | FacilityKind::ExpressElevator
                )
            }) {
                if let Some(group) = groups
                    .iter_mut()
                    .find(|group| group.0 == facility.kind && group.1 == facility.position.x)
                {
                    group.2 = group.2.min(facility.id);
                    if !group.3.contains(&facility.position.floor) {
                        group.3.push(facility.position.floor);
                    }
                } else {
                    groups.push((
                        facility.kind,
                        facility.position.x,
                        facility.id,
                        vec![facility.position.floor],
                    ));
                }
            }

            let mut synchronized = Vec::with_capacity(groups.len());
            for (kind, x, id, mut served_floors) in groups {
                served_floors.sort_by_key(|floor| floor_ordinal(*floor));
                if let Some(mut existing) = self
                    .elevators
                    .iter()
                    .find(|shaft| shaft.kind == kind && shaft.x == x)
                    .cloned()
                {
                    existing.id = id;
                    existing.served_floors = served_floors;
                    let landing_floors = landing_floors(
                        existing.kind,
                        existing.x,
                        &existing.served_floors,
                        tower.facilities(),
                    );
                    for car in &mut existing.cars {
                        if !landing_floors.contains(&car.home_floor) {
                            car.home_floor = default_home_floor(&landing_floors);
                        }
                    }
                    synchronized.push(existing);
                } else {
                    let landing_floors =
                        landing_floors(kind, x, &served_floors, tower.facilities());
                    let home_floor = default_home_floor(&landing_floors);
                    synchronized.push(ElevatorShaft {
                        id,
                        kind,
                        x,
                        served_floors,
                        cars: vec![ElevatorCar {
                            floor_position: f32::from(floor_ordinal(home_floor)),
                            home_floor,
                            direction: ElevatorDirection::Idle,
                            target_floor: None,
                            passengers: Vec::new(),
                            dwell_remaining: 0.0,
                        }],
                        schedules: [ElevatorSchedule::default(); 12],
                        visible: true,
                    });
                }
            }
            self.elevators = synchronized;
        }

        if topology_changed {
            self.connected_parking = connected_parking_facility_ids(tower.facilities());
        }

        if topology_changed && mode == TopologySync::Full {
            self.routes_from_lobby = destinations
                .iter()
                .filter_map(|facility| {
                    route_from_first_floor_lobby(
                        endpoint(facility),
                        &self.elevators,
                        tower.facilities(),
                        tower.floors(),
                    )
                    .map(|route| (facility.id, route))
                })
                .collect();
        } else {
            // Tenant placement does not change corridors or transport, and
            // additive construction cannot invalidate a working path. A
            // removed walkway invalidates only routes that cross that exact
            // flight. Preserve everything else and retry only new, affected,
            // or previously unreachable destinations.
            self.routes_from_lobby
                .retain(|facility_id, _| destination_ids.contains(facility_id));
            let mut affected = destinations
                .iter()
                .filter(|facility| !self.known_destinations.contains(&facility.id))
                .map(|facility| facility.id)
                .collect::<HashSet<_>>();
            match mode {
                TopologySync::Additive => {
                    // Additions may make any previously unreachable tenant
                    // reachable, so retry only the entries absent from cache.
                    affected.extend(
                        destinations
                            .iter()
                            .filter(|facility| !self.routes_from_lobby.contains_key(&facility.id))
                            .map(|facility| facility.id),
                    );
                }
                TopologySync::RemovedFloor(removed) => {
                    affected.extend(destinations.iter().filter_map(|facility| {
                        self.routes_from_lobby
                            .get(&facility.id)
                            .is_some_and(|route| {
                                route_uses_removed_floor(*route, facility, removed)
                            })
                            .then_some(facility.id)
                    }));
                }
                TopologySync::RemovedWalkway { kind, x, floor } => {
                    affected.extend(destinations.iter().filter_map(|facility| {
                        self.routes_from_lobby
                            .get(&facility.id)
                            .is_some_and(|route| {
                                route_uses_removed_walkway(
                                    *route,
                                    facility.position.floor,
                                    kind,
                                    x,
                                    floor,
                                )
                            })
                            .then_some(facility.id)
                    }));
                }
                TopologySync::Full => {}
            }
            self.routes_from_lobby
                .retain(|facility_id, _| !affected.contains(facility_id));
            for facility in &destinations {
                if !affected.contains(&facility.id) {
                    continue;
                }
                if let Some(route) = route_from_first_floor_lobby(
                    endpoint(facility),
                    &self.elevators,
                    tower.facilities(),
                    tower.floors(),
                ) {
                    self.routes_from_lobby.insert(facility.id, route);
                }
            }
        }
        self.reachable_facilities = self.routes_from_lobby.keys().copied().collect();
        self.topology_signature = signature;
        self.known_destinations = destination_ids;

        let live_facilities: HashSet<u64> = tower
            .facilities()
            .iter()
            .map(|facility| facility.id)
            .collect();
        self.people.retain(|person| {
            (person.home_facility_id == 0 || live_facilities.contains(&person.home_facility_id))
                && (person.destination_facility_id == 0
                    || live_facilities.contains(&person.destination_facility_id))
        });
        self.scheduled_trip_days
            .retain(|key, _| live_facilities.contains(&(key >> 5)));
        self.commercial_patronage
            .retain(|facility_id, _| live_facilities.contains(facility_id));
        for facility in tower.facilities().iter().filter(|facility| {
            facility.is_occupied() && commercial_patronage_target(facility.kind).is_some()
        }) {
            self.commercial_patronage
                .entry(facility.id)
                .or_insert_with(|| commercial_patronage_initial(facility.kind));
        }
        self.arrived_facilities
            .retain(|facility_id| live_facilities.contains(facility_id));
        // Condominiums represent residents who already live in the tower;
        // they do not need a generic outside visitor to validate occupancy.
        self.arrived_facilities.extend(
            tower
                .facilities()
                .iter()
                .filter(|facility| {
                    facility.kind == FacilityKind::Condo
                        && facility.is_occupied()
                        && self.reachable_facilities.contains(&facility.id)
                })
                .map(|facility| facility.id),
        );
        self.daily_visits
            .retain(|facility_id, _| live_facilities.contains(facility_id));
        self.refresh_current_visitors();
    }

    pub fn advance(&mut self, tower: &Tower, elapsed_seconds: f32) {
        if elapsed_seconds <= 0.0 {
            return;
        }
        self.sync_with_tower(tower);
        self.advance_after_sync(tower, elapsed_seconds);
    }

    /// Advances traffic after the caller has already synchronized tower
    /// topology for this frame. The desktop needs reachability before its
    /// economy tick, so this avoids hashing and checking the same tower twice.
    pub fn advance_after_sync(&mut self, tower: &Tower, elapsed_seconds: f32) {
        if elapsed_seconds <= 0.0 {
            return;
        }
        if self.patronage_day != tower.clock.day {
            self.patronage_day = tower.clock.day;
            self.daily_visits.clear();
            self.advance_commercial_patronage(tower);
        }
        self.dispatch_scheduled_trips(tower);
        self.spawn_accumulator += elapsed_seconds;
        let population_limit = (usize::from(tower.star_rating()) * 32).min(MAX_SIMULATED_PEOPLE);
        while self.spawn_accumulator >= self.next_spawn_seconds
            && self.people.len() < population_limit
        {
            self.spawn_accumulator -= self.next_spawn_seconds;
            self.next_spawn_seconds = next_spawn_interval(tower.star_rating(), self.next_person_id);
            if !self.spawn_person(tower) {
                break;
            }
        }

        let previously_visiting = self
            .people
            .iter()
            .filter(|person| !person.going_home && person.activity == PersonActivity::Visiting)
            .map(|person| person.id)
            .collect::<HashSet<_>>();
        advance_people(&mut self.people, elapsed_seconds);
        for person in self.people.iter().filter(|person| {
            person.destination_facility_id != 0
                && !person.role.is_staff()
                && !person.going_home
                && person.activity == PersonActivity::Visiting
                && !previously_visiting.contains(&person.id)
        }) {
            *self
                .daily_visits
                .entry(person.destination_facility_id)
                .or_default() += 1;
        }
        enforce_elevator_queue_limits(&mut self.people);
        self.arrived_facilities.extend(
            self.people
                .iter()
                .filter(|person| {
                    person.destination_facility_id != 0
                        && !person.role.is_staff()
                        && !person.going_home
                        && person.activity == PersonActivity::Visiting
                })
                .map(|person| person.destination_facility_id),
        );
        for shaft in &mut self.elevators {
            advance_shaft(
                shaft,
                &mut self.people,
                tower.facilities(),
                tower.clock,
                elapsed_seconds,
            );
        }
        self.people.retain(|person| {
            !(person.visit_remaining <= 0.0
                && (person.activity == PersonActivity::LeavingElevatorQueue
                    || (person.going_home && person.activity == PersonActivity::Visiting)))
        });
        self.refresh_current_visitors();
    }

    fn advance_commercial_patronage(&mut self, tower: &Tower) {
        for facility in tower.facilities().iter().filter(|facility| {
            facility.is_occupied() && self.reachable_facilities.contains(&facility.id)
        }) {
            let Some(target) = commercial_patronage_target(facility.kind) else {
                continue;
            };
            let patronage = self
                .commercial_patronage
                .entry(facility.id)
                .or_insert_with(|| commercial_patronage_initial(facility.kind));
            if facility.kind == FacilityKind::Cinema {
                *patronage = patronage.saturating_sub(12).max(target);
                continue;
            }
            let daily_growth = target.div_ceil(8).max(1);
            *patronage = patronage.saturating_add(daily_growth).min(target);
        }
    }

    fn refresh_current_visitors(&mut self) {
        self.current_visitors.clear();
        for person in self.people.iter().filter(|person| {
            person.destination_facility_id != 0
                && !person.role.is_staff()
                && person.activity == PersonActivity::Visiting
                && !person.going_home
        }) {
            *self
                .current_visitors
                .entry(person.destination_facility_id)
                .or_default() += 1;
        }
    }

    fn spawn_person(&mut self, tower: &Tower) -> bool {
        let destinations: Vec<(&Facility, TravelMode)> = tower
            .facilities()
            .iter()
            .filter_map(|facility| {
                if !facility.is_occupied()
                    || !is_person_destination(facility.kind)
                    || !facility_is_open_at(facility.kind, tower.clock)
                {
                    return None;
                }
                self.routes_from_lobby
                    .get(&facility.id)
                    .copied()
                    .map(|route| (facility, route))
            })
            .collect();
        if destinations.is_empty() {
            return false;
        }

        let seed = mix64(self.next_person_id ^ (u64::from(tower.clock.day) << 32));
        // Balance arrivals by each tenant's assigned/capacity ratio. The old
        // weighted lottery could send several people to one office while an
        // equally reachable office stayed empty throughout the workday.
        let mut assigned_by_facility = HashMap::new();
        for person in self.people.iter().filter(|person| !person.going_home) {
            *assigned_by_facility
                .entry(person.destination_facility_id)
                .or_insert(0_usize) += 1;
        }
        let mut least_loaded = Vec::new();
        let mut best_load = None::<(usize, usize)>;
        for &(facility, route) in &destinations {
            let assigned = assigned_by_facility.get(&facility.id).copied().unwrap_or(0);
            let capacity = facility.kind.population_capacity().max(1) as usize;
            if assigned >= capacity {
                continue;
            }
            let ordering = best_load.map(|(best_assigned, best_capacity)| {
                (assigned * best_capacity).cmp(&(best_assigned * capacity))
            });
            match ordering {
                None | Some(std::cmp::Ordering::Less) => {
                    best_load = Some((assigned, capacity));
                    least_loaded.clear();
                    least_loaded.push((facility, route));
                }
                Some(std::cmp::Ordering::Equal) => least_loaded.push((facility, route)),
                Some(std::cmp::Ordering::Greater) => {}
            }
        }
        if least_loaded.is_empty() {
            return false;
        }
        let selected = least_loaded[seed.rotate_left(23) as usize % least_loaded.len()];
        let (destination, lobby_route) = selected;
        // Every simulated person has a complete entrance-to-exit lifecycle.
        // Nobody materializes in an office, residence, hotel room, or business.
        let destination_kind = destination.kind;
        let destination = endpoint(destination);
        let (origin, route) =
            self.arrival_origin(tower, destination_kind, destination, lobby_route, seed);

        let mut person = Person {
            id: self.next_person_id,
            role: outside_visitor_role(destination_kind, self.next_person_id),
            home_facility_id: origin.facility_id,
            destination_facility_id: destination.facility_id,
            x: origin.x,
            floor_position: f32::from(floor_ordinal(origin.floor)),
            current_floor: origin.floor,
            mood: PersonMood::Calm,
            activity: PersonActivity::Walking,
            wait_seconds: 0.0,
            origin,
            destination,
            route,
            going_home: false,
            walking_from_transport: route == TravelMode::SameFloor,
            visit_remaining: 0.0,
            visit_duration: visit_duration_seconds(destination_kind),
            stair_progress: 0.0,
            route_leg: 0,
        };
        start_trip(&mut person);
        self.next_person_id += 1;
        self.people.push(person);
        true
    }

    fn arrival_origin(
        &self,
        tower: &Tower,
        destination_kind: FacilityKind,
        destination: Endpoint,
        lobby_route: TravelMode,
        seed: u64,
    ) -> (Endpoint, TravelMode) {
        let parking_trip = matches!(
            destination_kind,
            FacilityKind::Office | FacilityKind::HotelSuite
        ) && tower.star_rating() >= 3
            && (destination_kind == FacilityKind::HotelSuite || seed.is_multiple_of(3));
        if parking_trip
            && let Some((origin, route, _)) = tower
                .facilities()
                .iter()
                .filter(|facility| {
                    facility.kind == FacilityKind::Parking
                        && self.connected_parking.contains(&facility.id)
                })
                .filter_map(|facility| {
                    let origin = endpoint(facility);
                    choose_route_scored(
                        origin,
                        destination,
                        &self.elevators,
                        tower.facilities(),
                        tower.floors(),
                    )
                    .map(|(route, score)| (origin, route, score))
                })
                .min_by(|left, right| left.2.total_cmp(&right.2))
        {
            return (origin, route);
        }

        if seed.is_multiple_of(2)
            && let Some((origin, route, _)) = tower
                .facilities()
                .iter()
                .filter(|facility| facility.kind == FacilityKind::Metro)
                .filter_map(|facility| {
                    let origin = facility_top_endpoint(facility);
                    choose_route_scored(
                        origin,
                        destination,
                        &self.elevators,
                        tower.facilities(),
                        tower.floors(),
                    )
                    .map(|(route, score)| (origin, route, score))
                })
                .min_by(|left, right| left.2.total_cmp(&right.2))
        {
            return (origin, route);
        }

        (
            Endpoint {
                facility_id: 0,
                x: exterior_spawn_x(tower, seed),
                floor: 1,
            },
            lobby_route,
        )
    }

    fn dispatch_scheduled_trips(&mut self, tower: &Tower) {
        const DISPATCH_BUCKET_MINUTES: u64 = 5;
        const MAX_DISPATCHES_PER_BUCKET: usize = 12;
        const MAX_HOMES_INSPECTED_PER_BUCKET: usize = 256;

        let bucket = tower.clock.absolute_minute() / DISPATCH_BUCKET_MINUTES;
        if bucket == self.last_resident_dispatch_bucket {
            return;
        }
        self.last_resident_dispatch_bucket = bucket;
        if self.people.len() >= MAX_SIMULATED_PEOPLE || !scheduled_trip_window_active(tower.clock) {
            return;
        }

        let facilities = tower.facilities();
        if facilities.is_empty() {
            return;
        }
        let start = self.scheduled_home_cursor % facilities.len();
        let inspection_limit = facilities.len().min(MAX_HOMES_INSPECTED_PER_BUCKET);
        let mut dispatched = 0;
        for offset in 0..inspection_limit {
            let index = (start + offset) % facilities.len();
            self.scheduled_home_cursor = (index + 1) % facilities.len();
            let home = &facilities[index];
            if !home.is_occupied()
                || !self.reachable_facilities.contains(&home.id)
                || !matches!(
                    home.kind,
                    FacilityKind::Condo
                        | FacilityKind::Office
                        | FacilityKind::HotelSingle
                        | FacilityKind::HotelTwin
                        | FacilityKind::HotelSuite
                        | FacilityKind::Cinema
                )
                || (home.kind != FacilityKind::Condo && self.visitors_at(home.id) == 0)
            {
                continue;
            }
            for trip in scheduled_trip_specs(tower.clock, home, tower.star_rating()) {
                if self.people.len() >= MAX_SIMULATED_PEOPLE
                    || dispatched >= MAX_DISPATCHES_PER_BUCKET
                {
                    return;
                }
                let stamp_key = scheduled_trip_stamp_key(home.id, trip.event);
                if self.scheduled_trip_days.get(&stamp_key) == Some(&tower.clock.day) {
                    continue;
                }
                if self.spawn_scheduled_trip(tower, home, trip) {
                    self.scheduled_trip_days.insert(stamp_key, tower.clock.day);
                    dispatched += 1;
                }
            }
        }
    }

    fn spawn_scheduled_trip(
        &mut self,
        tower: &Tower,
        home: &Facility,
        trip: ScheduledTripSpec,
    ) -> bool {
        let origin = endpoint(home);
        let (destination, route) = match trip.purpose {
            ScheduledTripPurpose::Outside => {
                let Some(route) = self.routes_from_lobby.get(&home.id).copied() else {
                    return false;
                };
                let seed = mix64(self.next_person_id ^ home.id ^ u64::from(tower.clock.day));
                (
                    Endpoint {
                        facility_id: 0,
                        x: exterior_spawn_x(tower, seed),
                        floor: 1,
                    },
                    reverse_route(route),
                )
            }
            purpose => {
                let target = tower
                    .facilities()
                    .iter()
                    .filter(|facility| {
                        facility.is_occupied()
                            && facility.id != home.id
                            && resident_purpose_accepts(purpose, facility.kind)
                            && facility_is_open(facility.kind, tower.clock.minute_of_day)
                    })
                    .filter_map(|facility| {
                        choose_route_scored(
                            origin,
                            endpoint(facility),
                            &self.elevators,
                            tower.facilities(),
                            tower.floors(),
                        )
                        .map(|(route, score)| (facility, route, score))
                    })
                    .min_by(|left, right| left.2.total_cmp(&right.2));
                let Some((target, route, _)) = target else {
                    // The original residents leave the tower when the service
                    // they want is unavailable nearby.
                    let Some(route) = self.routes_from_lobby.get(&home.id).copied() else {
                        return false;
                    };
                    let seed = mix64(self.next_person_id ^ home.id ^ u64::from(tower.clock.day));
                    return self.push_scheduled_person(
                        home,
                        Endpoint {
                            facility_id: 0,
                            x: exterior_spawn_x(tower, seed),
                            floor: 1,
                        },
                        reverse_route(route),
                        trip,
                    );
                };
                (endpoint(target), route)
            }
        };
        self.push_scheduled_person(home, destination, route, trip)
    }

    fn push_scheduled_person(
        &mut self,
        home: &Facility,
        destination: Endpoint,
        route: TravelMode,
        trip: ScheduledTripSpec,
    ) -> bool {
        let origin = endpoint(home);
        let mut person = Person {
            id: self.next_person_id,
            role: trip.role,
            home_facility_id: home.id,
            destination_facility_id: destination.facility_id,
            x: origin.x,
            floor_position: f32::from(floor_ordinal(origin.floor)),
            current_floor: origin.floor,
            mood: PersonMood::Calm,
            activity: PersonActivity::Walking,
            wait_seconds: 0.0,
            origin,
            destination,
            route,
            going_home: false,
            walking_from_transport: route == TravelMode::SameFloor,
            visit_remaining: 0.0,
            visit_duration: trip.visit_seconds,
            stair_progress: 0.0,
            route_leg: 0,
        };
        start_trip(&mut person);
        self.next_person_id += 1;
        self.people.push(person);
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScheduledTripPurpose {
    Outside,
    OfficeCall,
    Shopping,
    Meal,
    Medical,
    Commercial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScheduledTripSpec {
    event: ScheduledTripEvent,
    role: PersonRole,
    purpose: ScheduledTripPurpose,
    visit_seconds: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScheduledTripEvent {
    ChildSchool,
    ResidentWork,
    ResidentShopping,
    WeekendMotherMeal,
    WeekendChildMeal,
    WeekendAdultMeal,
    OfficeSalesCall,
    OfficeLunch,
    OfficeMedical,
    HotelDinner,
    CinemaMatineeDispersal,
    CinemaEveningDispersal,
}

fn scheduled_trip_specs(clock: Clock, home: &Facility, star_rating: u8) -> Vec<ScheduledTripSpec> {
    let minute = clock.minute_of_day;
    let weekend = is_weekend(clock);
    let home_id = home.id;
    let mut trips = Vec::with_capacity(3);
    if home.kind == FacilityKind::Condo && weekend {
        let departure = 11 * 60 + (home_id % 75) as u16;
        if (departure..13 * 60 + 30).contains(&minute) {
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::WeekendMotherMeal,
                role: PersonRole::WomanWithChild,
                purpose: ScheduledTripPurpose::Meal,
                visit_seconds: 24.0,
            });
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::WeekendChildMeal,
                role: PersonRole::Child,
                purpose: ScheduledTripPurpose::Meal,
                visit_seconds: 24.0,
            });
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::WeekendAdultMeal,
                role: PersonRole::Homebody,
                purpose: ScheduledTripPurpose::Meal,
                visit_seconds: 24.0,
            });
        }
        return trips;
    }

    if home.kind == FacilityKind::Office && !weekend {
        let sales_departure = 10 * 60 + (home_id % 75) as u16;
        if (sales_departure..11 * 60 + 45).contains(&minute) {
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::OfficeSalesCall,
                role: PersonRole::Salesman,
                purpose: ScheduledTripPurpose::OfficeCall,
                visit_seconds: 18.0,
            });
        }
        let lunch_departure = 12 * 60 + (home_id % 45) as u16;
        if (lunch_departure..13 * 60).contains(&minute) {
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::OfficeLunch,
                role: PersonRole::Salesman,
                purpose: ScheduledTripPurpose::Meal,
                visit_seconds: 18.0,
            });
        }
        let medical_departure = 14 * 60 + (home_id % 90) as u16;
        if star_rating >= 3
            && (home_id + u64::from(clock.day)).is_multiple_of(3)
            && (medical_departure..16 * 60).contains(&minute)
        {
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::OfficeMedical,
                role: PersonRole::Salesman,
                purpose: ScheduledTripPurpose::Medical,
                visit_seconds: 24.0,
            });
        }
        return trips;
    }

    if matches!(
        home.kind,
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite
    ) {
        let departure = 18 * 60 + (home_id % 90) as u16;
        if (departure..20 * 60 + 30).contains(&minute) {
            trips.push(ScheduledTripSpec {
                event: ScheduledTripEvent::HotelDinner,
                role: PersonRole::Visitor,
                purpose: ScheduledTripPurpose::Meal,
                visit_seconds: 35.0,
            });
        }
        return trips;
    }

    if home.kind == FacilityKind::Cinema {
        let event = if (15 * 60 + 50..16 * 60 + 15).contains(&minute) {
            Some(ScheduledTripEvent::CinemaMatineeDispersal)
        } else if (19 * 60 + 50..20 * 60 + 15).contains(&minute) {
            Some(ScheduledTripEvent::CinemaEveningDispersal)
        } else {
            None
        };
        if let Some(event) = event {
            trips.push(ScheduledTripSpec {
                event,
                role: PersonRole::Visitor,
                purpose: ScheduledTripPurpose::Commercial,
                visit_seconds: 22.0,
            });
        }
        return trips;
    }

    if home.kind != FacilityKind::Condo {
        return trips;
    }

    let child_departure = 7 * 60 + 20 + (home_id % 40) as u16;
    if (child_departure..9 * 60).contains(&minute) {
        trips.push(ScheduledTripSpec {
            event: ScheduledTripEvent::ChildSchool,
            role: PersonRole::Child,
            purpose: ScheduledTripPurpose::Outside,
            // Six game hours at normal speed: schoolchildren return in the
            // early afternoon, before the working adult.
            visit_seconds: 180.0,
        });
    }
    let worker_departure = 8 * 60 + (home_id % 45) as u16;
    if (worker_departure..9 * 60 + 45).contains(&minute) {
        trips.push(ScheduledTripSpec {
            event: ScheduledTripEvent::ResidentWork,
            role: PersonRole::Salesman,
            purpose: ScheduledTripPurpose::Outside,
            visit_seconds: 270.0,
        });
    }
    let shopping_departure = 10 * 60 + (home_id % 60) as u16;
    if (shopping_departure..12 * 60).contains(&minute) {
        trips.push(ScheduledTripSpec {
            event: ScheduledTripEvent::ResidentShopping,
            role: PersonRole::Woman,
            purpose: ScheduledTripPurpose::Shopping,
            visit_seconds: 28.0,
        });
    }
    trips
}

fn scheduled_trip_window_active(clock: Clock) -> bool {
    if is_weekend(clock) {
        (11 * 60..20 * 60 + 15).contains(&clock.minute_of_day)
    } else {
        (7 * 60 + 20..20 * 60 + 15).contains(&clock.minute_of_day)
    }
}

fn resident_purpose_accepts(purpose: ScheduledTripPurpose, kind: FacilityKind) -> bool {
    match purpose {
        ScheduledTripPurpose::Outside => false,
        ScheduledTripPurpose::OfficeCall => kind == FacilityKind::Office,
        ScheduledTripPurpose::Shopping => kind == FacilityKind::Shop,
        ScheduledTripPurpose::Meal => {
            matches!(kind, FacilityKind::FastFood | FacilityKind::Restaurant)
        }
        ScheduledTripPurpose::Medical => kind == FacilityKind::Medical,
        ScheduledTripPurpose::Commercial => matches!(
            kind,
            FacilityKind::FastFood | FacilityKind::Restaurant | FacilityKind::Shop
        ),
    }
}

const fn scheduled_trip_stamp_key(home_id: u64, event: ScheduledTripEvent) -> u64 {
    (home_id << 5) | event as u64
}

const fn outside_visitor_role(kind: FacilityKind, person_id: u64) -> PersonRole {
    match kind {
        FacilityKind::Office => PersonRole::Salesman,
        FacilityKind::Condo => match person_id % 3 {
            0 => PersonRole::Homebody,
            1 => PersonRole::Woman,
            _ => PersonRole::Child,
        },
        _ => PersonRole::Visitor,
    }
}

fn advance_people(people: &mut [Person], elapsed_seconds: f32) {
    for person in people {
        match person.activity {
            PersonActivity::Walking => {
                let target = if person.walking_from_transport {
                    trip_destination(person).x
                } else {
                    match person.route {
                        TravelMode::SameFloor => trip_destination(person).x,
                        TravelMode::Elevator { x, .. } | TravelMode::Stairs { x } => x,
                        TravelMode::ElevatorTransfer {
                            first_x, second_x, ..
                        } => {
                            if person.route_leg == 0 {
                                first_x
                            } else {
                                second_x
                            }
                        }
                    }
                };
                person.x = move_toward(person.x, target, elapsed_seconds * 8.0);
                if (person.x - target).abs() < 0.01 {
                    if person.walking_from_transport {
                        arrive_at_facility(person);
                    } else {
                        match person.route {
                            TravelMode::SameFloor => arrive_at_facility(person),
                            TravelMode::Elevator { shaft_id, .. } => {
                                person.activity = PersonActivity::WaitingForElevator { shaft_id };
                            }
                            TravelMode::ElevatorTransfer {
                                first_id,
                                second_id,
                                ..
                            } => {
                                person.activity = PersonActivity::WaitingForElevator {
                                    shaft_id: if person.route_leg == 0 {
                                        first_id
                                    } else {
                                        second_id
                                    },
                                };
                            }
                            TravelMode::Stairs { .. } => {
                                person.activity = PersonActivity::UsingStairs;
                                person.stair_progress = 0.0;
                            }
                        }
                    }
                }
            }
            PersonActivity::WaitingForElevator { .. } => {
                person.wait_seconds += elapsed_seconds;
                person.mood = if person.wait_seconds >= ANGRY_WAIT_SECONDS {
                    PersonMood::Angry
                } else if person.wait_seconds >= CONCERNED_WAIT_SECONDS {
                    PersonMood::Concerned
                } else {
                    PersonMood::Calm
                };
            }
            PersonActivity::RidingElevator { .. } => {}
            PersonActivity::UsingStairs => {
                person.stair_progress = (person.stair_progress + elapsed_seconds * 0.7).min(1.0);
                let destination = trip_destination(person);
                let start = f32::from(floor_ordinal(person.current_floor));
                let end = f32::from(floor_ordinal(destination.floor));
                person.floor_position = start + (end - start) * person.stair_progress;
                if person.stair_progress >= 1.0 {
                    person.current_floor = destination.floor;
                    person.floor_position = end;
                    person.walking_from_transport = true;
                    person.activity = PersonActivity::Walking;
                }
            }
            PersonActivity::Visiting => {
                person.visit_remaining -= elapsed_seconds;
                if person.visit_remaining <= 0.0 && !person.going_home {
                    person.going_home = true;
                    person.current_floor = person.destination.floor;
                    person.floor_position = f32::from(floor_ordinal(person.current_floor));
                    person.route = reverse_route(person.route);
                    person.route_leg = 0;
                    person.walking_from_transport = person.route == TravelMode::SameFloor;
                    person.activity = PersonActivity::Walking;
                }
            }
            PersonActivity::LeavingElevatorQueue => {
                person.visit_remaining -= elapsed_seconds;
                person.x = move_toward(person.x, person.origin.x, elapsed_seconds * 8.0);
            }
        }
    }
}

fn enforce_elevator_queue_limits(people: &mut [Person]) {
    let mut queue_lengths: HashMap<(u64, i16), usize> = HashMap::new();
    for person in people {
        let PersonActivity::WaitingForElevator { shaft_id } = person.activity else {
            continue;
        };
        let queue_length = queue_lengths
            .entry((shaft_id, person.current_floor))
            .or_default();
        if person.wait_seconds >= ABANDON_ELEVATOR_WAIT_SECONDS
            || *queue_length >= MAX_ELEVATOR_QUEUE_PER_FLOOR
        {
            person.activity = PersonActivity::LeavingElevatorQueue;
            person.mood = PersonMood::Angry;
            person.visit_remaining = QUEUE_ABANDON_WALK_SECONDS;
            continue;
        }
        *queue_length += 1;
    }
}

fn advance_shaft(
    shaft: &mut ElevatorShaft,
    people: &mut [Person],
    facilities: &[Facility],
    clock: Clock,
    elapsed_seconds: f32,
) {
    let schedule = shaft.current_schedule(clock);
    let waiting_assignments = assign_waiting_calls(shaft, people, schedule.waiting_car_response);
    let landing_floors = landing_floors(shaft.kind, shaft.x, &shaft.served_floors, facilities);
    let top_floor = landing_floors.last().copied();
    let bottom_floor = landing_floors.first().copied();
    for (car_index, car) in shaft.cars.iter_mut().enumerate() {
        if car.dwell_remaining > 0.0 {
            car.dwell_remaining -= elapsed_seconds;
            board_waiting_people(shaft.id, shaft.kind, shaft.x, car, people);
            if car.dwell_remaining > 0.0 {
                continue;
            }
        }

        let stops = requested_stops(shaft.id, people, car, car_index, &waiting_assignments);
        if stops.is_empty() {
            if (car.floor_position - f32::from(floor_ordinal(car.home_floor))).abs() < 0.01 {
                car.direction = ElevatorDirection::Idle;
                car.target_floor = None;
                continue;
            }
            car.target_floor = Some(car.home_floor);
        } else if !car.passengers.is_empty() && schedule.mode == ElevatorMode::ExpressToTop {
            car.target_floor = top_floor;
        } else if !car.passengers.is_empty() && schedule.mode == ElevatorMode::ExpressToBottom {
            car.target_floor = bottom_floor;
        } else {
            car.target_floor = choose_next_stop(car.floor_position, car.direction, &stops);
        }

        let Some(target_floor) = car.target_floor else {
            continue;
        };
        let target = f32::from(floor_ordinal(target_floor));
        car.direction = if target > car.floor_position {
            ElevatorDirection::Up
        } else if target < car.floor_position {
            ElevatorDirection::Down
        } else {
            car.direction
        };
        car.floor_position = move_toward(car.floor_position, target, elapsed_seconds * 2.2);

        for person in people.iter_mut().filter(|person| {
            matches!(
                person.activity,
                PersonActivity::RidingElevator { shaft_id } if shaft_id == shaft.id
            ) && car.passengers.contains(&person.id)
        }) {
            person.floor_position = car.floor_position;
            person.x = f32::from(shaft.x) + f32::from(shaft.kind.spec().width) * 0.5;
        }

        if (car.floor_position - target).abs() < 0.01 {
            car.floor_position = target;
            car.target_floor = None;
            unload_people(shaft.id, shaft.x, target_floor, car, people);
            board_waiting_people(shaft.id, shaft.kind, shaft.x, car, people);
            car.dwell_remaining = f32::from(schedule.departure_seconds).max(0.6);
        }
    }
}

fn board_waiting_people(
    shaft_id: u64,
    kind: FacilityKind,
    shaft_x: u16,
    car: &mut ElevatorCar,
    people: &mut [Person],
) {
    let floor = floor_from_ordinal(car.floor_position.round() as i16);
    for person in people.iter_mut().filter(|person| {
        person.current_floor == floor
            && matches!(
                person.activity,
                PersonActivity::WaitingForElevator { shaft_id: waiting_for } if waiting_for == shaft_id
            )
    }) {
        if car.passengers.len() >= elevator_capacity(kind) {
            break;
        }
        person.activity = PersonActivity::RidingElevator { shaft_id };
        person.x = f32::from(shaft_x) + 2.0;
        car.passengers.push(person.id);
    }
}

pub const fn elevator_capacity(kind: FacilityKind) -> usize {
    if matches!(kind, FacilityKind::ExpressElevator) {
        EXPRESS_ELEVATOR_CAPACITY
    } else {
        ELEVATOR_CAPACITY
    }
}

fn unload_people(
    shaft_id: u64,
    shaft_x: u16,
    floor: i16,
    car: &mut ElevatorCar,
    people: &mut [Person],
) {
    let mut departing = Vec::new();
    for person in people.iter_mut().filter(|person| {
        matches!(
            person.activity,
            PersonActivity::RidingElevator { shaft_id: riding } if riding == shaft_id
        ) && desired_elevator_floor(person) == floor
            && car.passengers.contains(&person.id)
    }) {
        departing.push(person.id);
        person.current_floor = floor;
        person.floor_position = f32::from(floor_ordinal(floor));
        person.x = f32::from(shaft_x) + 2.0;
        if matches!(person.route, TravelMode::ElevatorTransfer { .. }) && person.route_leg == 0 {
            person.route_leg = 1;
            person.walking_from_transport = false;
        } else {
            person.walking_from_transport = true;
        }
        person.activity = PersonActivity::Walking;
    }
    car.passengers.retain(|id| !departing.contains(id));
}

fn requested_stops(
    shaft_id: u64,
    people: &[Person],
    car: &ElevatorCar,
    car_index: usize,
    waiting_assignments: &[(i16, usize)],
) -> Vec<i16> {
    let mut stops = Vec::new();
    for person in people {
        let floor = match person.activity {
            PersonActivity::WaitingForElevator {
                shaft_id: waiting_for,
            } if waiting_for == shaft_id
                && waiting_assignments.contains(&(person.current_floor, car_index)) =>
            {
                person.current_floor
            }
            PersonActivity::RidingElevator { shaft_id: riding }
                if riding == shaft_id && car.passengers.contains(&person.id) =>
            {
                desired_elevator_floor(person)
            }
            _ => continue,
        };
        if !stops.contains(&floor) {
            stops.push(floor);
        }
    }
    stops
}

fn assign_waiting_calls(
    shaft: &ElevatorShaft,
    people: &[Person],
    response_floors: u8,
) -> Vec<(i16, usize)> {
    let mut waiting_floors = Vec::new();
    for person in people {
        if matches!(
            person.activity,
            PersonActivity::WaitingForElevator { shaft_id } if shaft_id == shaft.id
        ) && !waiting_floors.contains(&person.current_floor)
        {
            waiting_floors.push(person.current_floor);
        }
    }

    waiting_floors
        .into_iter()
        .filter_map(|floor| {
            let floor_position = f32::from(floor_ordinal(floor));
            let desired_direction = people
                .iter()
                .find(|person| {
                    person.current_floor == floor
                        && matches!(
                            person.activity,
                            PersonActivity::WaitingForElevator { shaft_id }
                                if shaft_id == shaft.id
                        )
                })
                .map(|person| {
                    if floor_ordinal(desired_elevator_floor(person)) > floor_ordinal(floor) {
                        ElevatorDirection::Up
                    } else {
                        ElevatorDirection::Down
                    }
                })?;

            // Keep a car committed to a hall call once it starts travelling
            // toward that floor. Without this, the assignment can jump to a
            // different car every frame as their relative distances change.
            // A high-floor car then moves down a fraction of a floor, loses
            // the call, and immediately returns to its default waiting floor.
            let committed = shaft
                .cars
                .iter()
                .enumerate()
                .filter(|(_, car)| car.target_floor == Some(floor))
                .min_by(|left, right| {
                    (left.1.floor_position - floor_position)
                        .abs()
                        .total_cmp(&(right.1.floor_position - floor_position).abs())
                })
                .map(|(index, _)| index);

            let moving = shaft
                .cars
                .iter()
                .enumerate()
                .filter(|(_, car)| {
                    car.direction == desired_direction
                        && match desired_direction {
                            ElevatorDirection::Up => car.floor_position <= floor_position,
                            ElevatorDirection::Down => car.floor_position >= floor_position,
                            ElevatorDirection::Idle => false,
                        }
                        && (car.floor_position - floor_position).abs() <= f32::from(response_floors)
                })
                .min_by(|left, right| {
                    (left.1.floor_position - floor_position)
                        .abs()
                        .total_cmp(&(right.1.floor_position - floor_position).abs())
                })
                .map(|(index, _)| index);
            let waiting = shaft
                .cars
                .iter()
                .enumerate()
                .filter(|(_, car)| car.direction == ElevatorDirection::Idle)
                .min_by(|left, right| {
                    (left.1.floor_position - floor_position)
                        .abs()
                        .total_cmp(&(right.1.floor_position - floor_position).abs())
                })
                .map(|(index, _)| index);
            let nearest = shaft
                .cars
                .iter()
                .enumerate()
                .min_by(|left, right| {
                    (left.1.floor_position - floor_position)
                        .abs()
                        .total_cmp(&(right.1.floor_position - floor_position).abs())
                })
                .map(|(index, _)| index);
            committed
                .or(moving)
                .or(waiting)
                .or(nearest)
                .map(|index| (floor, index))
        })
        .collect()
}

fn choose_next_stop(position: f32, direction: ElevatorDirection, stops: &[i16]) -> Option<i16> {
    let mut ranked: Vec<(i16, f32)> = stops
        .iter()
        .copied()
        .map(|floor| (floor, f32::from(floor_ordinal(floor))))
        .collect();
    ranked.sort_by(|left, right| left.1.total_cmp(&right.1));
    match direction {
        ElevatorDirection::Up => ranked
            .iter()
            .find(|(_, ordinal)| *ordinal >= position - 0.01)
            .or_else(|| ranked.last())
            .map(|(floor, _)| *floor),
        ElevatorDirection::Down => ranked
            .iter()
            .rev()
            .find(|(_, ordinal)| *ordinal <= position + 0.01)
            .or_else(|| ranked.first())
            .map(|(floor, _)| *floor),
        ElevatorDirection::Idle => ranked
            .iter()
            .min_by(|left, right| {
                (left.1 - position)
                    .abs()
                    .total_cmp(&(right.1 - position).abs())
            })
            .map(|(floor, _)| *floor),
    }
}

#[cfg(test)]
fn choose_route(
    origin: Endpoint,
    destination: Endpoint,
    elevators: &[ElevatorShaft],
    facilities: &[Facility],
    floors: &[GridPosition],
) -> Option<TravelMode> {
    choose_route_scored(origin, destination, elevators, facilities, floors).map(|(route, _)| route)
}

fn route_from_first_floor_lobby(
    destination: Endpoint,
    elevators: &[ElevatorShaft],
    facilities: &[Facility],
    floors: &[GridPosition],
) -> Option<TravelMode> {
    facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Lobby && facility.position.floor == 1)
        .filter_map(|lobby| {
            choose_route_scored(endpoint(lobby), destination, elevators, facilities, floors)
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(route, _)| route)
}

fn route_uses_removed_walkway(
    route: TravelMode,
    destination_floor: i16,
    removed_kind: FacilityKind,
    removed_x: u16,
    removed_floor: i16,
) -> bool {
    let TravelMode::Stairs { x: route_x } = route else {
        return false;
    };
    let walkway_x = f32::from(removed_x) + f32::from(removed_kind.spec().width) * 0.5;
    if (route_x - walkway_x).abs() > f32::EPSILON {
        return false;
    }

    // A flight based on N joins ordinal N to N + 1. First floor is ordinal
    // zero, while B1 is -1, so this also handles the B1/first-floor join.
    let origin = floor_ordinal(1);
    let destination = floor_ordinal(destination_floor);
    let flight_base = floor_ordinal(removed_floor);
    origin.min(destination) <= flight_base && origin.max(destination) > flight_base
}

fn route_uses_removed_floor(
    route: TravelMode,
    destination: &Facility,
    removed: GridPosition,
) -> bool {
    // First-floor floor cells can participate in the entrance leg before any
    // vertical transport. The cached route does not retain which lobby slice
    // supplied that leg, so conservatively refresh all routes on floor one.
    if removed.floor == 1 {
        return true;
    }
    if destination.position.floor != removed.floor {
        return false;
    }

    let transport_x = match route {
        TravelMode::SameFloor => return true,
        TravelMode::Elevator { x, .. } | TravelMode::Stairs { x } => x,
        TravelMode::ElevatorTransfer { second_x, .. } => second_x,
    };
    let destination_x = endpoint(destination).x;
    let start = transport_x.min(destination_x).floor().max(0.0) as u16;
    let end = transport_x.max(destination_x).floor().max(0.0) as u16;
    (start..=end).contains(&removed.x)
}

fn tower_topology_signature(tower: &Tower) -> u64 {
    let mut signature = mix64(u64::from(tower.width()) ^ 0x544f_504f_4c4f_4759);
    for floor in tower.floors() {
        let floor_bits = u64::from(floor.x) | ((floor.floor as i64 as u64) << 16);
        signature = mix64(signature ^ floor_bits);
    }
    signature = mix64(signature ^ tower.floors().len() as u64);
    let route_infrastructure = tower
        .facilities()
        .iter()
        .filter(|facility| is_route_infrastructure(facility.kind));
    let mut infrastructure_count = 0_u64;
    for facility in route_infrastructure {
        infrastructure_count += 1;
        let position_bits = u64::from(facility.position.x)
            | ((facility.position.floor as i64 as u64) << 16)
            | (u64::from(facility.stories) << 32);
        signature = mix64(signature ^ facility.id ^ ((facility.kind as u64) << 48) ^ position_bits);
    }
    mix64(signature ^ infrastructure_count)
}

fn connected_parking_facility_ids(facilities: &[Facility]) -> HashSet<u64> {
    let mut valid_ramps = HashSet::<(u16, i16)>::new();
    let mut ramps = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Ramp && facility.position.floor < 0)
        .collect::<Vec<_>>();
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

const fn is_route_infrastructure(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::Lobby
            | FacilityKind::Stairs
            | FacilityKind::Escalator
            | FacilityKind::Elevator
            | FacilityKind::ServiceElevator
            | FacilityKind::ExpressElevator
            | FacilityKind::Parking
            | FacilityKind::Ramp
            | FacilityKind::Metro
    )
}

fn choose_route_scored(
    origin: Endpoint,
    destination: Endpoint,
    elevators: &[ElevatorShaft],
    facilities: &[Facility],
    floors: &[GridPosition],
) -> Option<(TravelMode, f32)> {
    choose_route_scored_for(
        origin,
        destination,
        elevators,
        facilities,
        floors,
        ElevatorAccess::Public,
    )
}

fn choose_route_scored_for(
    origin: Endpoint,
    destination: Endpoint,
    elevators: &[ElevatorShaft],
    facilities: &[Facility],
    floors: &[GridPosition],
    access: ElevatorAccess,
) -> Option<(TravelMode, f32)> {
    if origin.floor == destination.floor {
        return horizontal_floor_path(origin.floor, origin.x, destination.x, floors)
            .then_some((TravelMode::SameFloor, (origin.x - destination.x).abs()));
    }

    let flights = (floor_ordinal(destination.floor) - floor_ordinal(origin.floor)).unsigned_abs();
    let mut candidates = Vec::new();

    // A continuous column may contain several individual stair/escalator
    // flights. Their perceived cost rises quadratically, so people prefer a
    // nearby lift as the climb grows instead of treating every staircase as
    // equally attractive.
    if access == ElevatorAccess::Public {
        for (kind, x) in connected_vertical_walkways(origin.floor, destination.floor, facilities) {
            let limit = if kind == FacilityKind::Escalator {
                MAX_ESCALATOR_FLIGHTS
            } else {
                MAX_STAIR_FLIGHTS
            };
            if flights <= limit
                && horizontal_floor_path(origin.floor, origin.x, x, floors)
                && horizontal_floor_path(destination.floor, x, destination.x, floors)
            {
                let flight_penalty = if kind == FacilityKind::Escalator {
                    3.0
                } else {
                    6.0
                };
                let score = (origin.x - x).abs()
                    + (destination.x - x).abs()
                    + (flights * flights) as f32 * flight_penalty;
                candidates.push((TravelMode::Stairs { x }, score));
            }
        }
    }

    for shaft in elevators.iter().filter(|shaft| {
        let x = shaft_center(shaft);
        elevator_accepts(shaft.kind, access)
            && shaft_stops_at(shaft, origin.floor, facilities)
            && shaft_stops_at(shaft, destination.floor, facilities)
            && lobby_endpoint_connects_to_shaft(origin, shaft, facilities)
            && lobby_endpoint_connects_to_shaft(destination, shaft, facilities)
            && horizontal_floor_path(origin.floor, origin.x, x, floors)
            && horizontal_floor_path(destination.floor, x, destination.x, floors)
    }) {
        let x = shaft_center(shaft);
        candidates.push((
            TravelMode::Elevator {
                shaft_id: shaft.id,
                x,
            },
            shaft_distance(shaft, origin, destination) + 12.0 + flights as f32 * 0.5,
        ));
    }

    // Housekeepers walk on their current floor and take one direct service
    // elevator between floors. They never use passenger/express cars, stairs,
    // escalators, or public sky-lobby transfers.
    if access == ElevatorAccess::Housekeeping {
        return candidates
            .into_iter()
            .min_by(|left, right| left.1.total_cmp(&right.1));
    }

    let mut lobby_floors: Vec<i16> = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Lobby)
        .map(|facility| facility.position.floor)
        .collect();
    lobby_floors.sort_unstable();
    lobby_floors.dedup();
    for first in elevators.iter().filter(|shaft| {
        let x = shaft_center(shaft);
        elevator_accepts(shaft.kind, access)
            && shaft_stops_at(shaft, origin.floor, facilities)
            && lobby_endpoint_connects_to_shaft(origin, shaft, facilities)
            && horizontal_floor_path(origin.floor, origin.x, x, floors)
    }) {
        for second in elevators.iter().filter(|second| {
            let x = shaft_center(second);
            first.id != second.id
                && second.kind != FacilityKind::ServiceElevator
                && shaft_stops_at(second, destination.floor, facilities)
                && lobby_endpoint_connects_to_shaft(destination, second, facilities)
                && horizontal_floor_path(destination.floor, x, destination.x, floors)
        }) {
            let first_x = shaft_center(first);
            let second_x = shaft_center(second);
            for transfer_floor in lobby_floors.iter().copied().filter(|floor| {
                shaft_stops_at(first, *floor, facilities)
                    && shaft_stops_at(second, *floor, facilities)
                    && lobby_connects_shafts(*floor, first, second, facilities)
            }) {
                let first_flights =
                    (floor_ordinal(transfer_floor) - floor_ordinal(origin.floor)).unsigned_abs();
                let second_flights = (floor_ordinal(destination.floor)
                    - floor_ordinal(transfer_floor))
                .unsigned_abs();
                let score = (origin.x - first_x).abs()
                    + (first_x - second_x).abs()
                    + (destination.x - second_x).abs()
                    + 30.0
                    + (first_flights + second_flights) as f32 * 0.5
                    + second_flights as f32 * 0.01;
                candidates.push((
                    TravelMode::ElevatorTransfer {
                        first_id: first.id,
                        first_x,
                        transfer_floor,
                        second_id: second.id,
                        second_x,
                    },
                    score,
                ));
            }
        }
    }

    candidates
        .into_iter()
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn elevator_accepts(kind: FacilityKind, access: ElevatorAccess) -> bool {
    match access {
        ElevatorAccess::Public => kind != FacilityKind::ServiceElevator,
        ElevatorAccess::Housekeeping => kind == FacilityKind::ServiceElevator,
    }
}

fn shaft_center(shaft: &ElevatorShaft) -> f32 {
    f32::from(shaft.x) + f32::from(shaft.kind.spec().width) * 0.5
}

fn horizontal_floor_path(floor: i16, from_x: f32, to_x: f32, floors: &[GridPosition]) -> bool {
    // Unit routing tests that focus purely on transport may omit structural
    // floor data. Live tower routing always supplies the actual floor cells.
    if floors.is_empty() {
        return true;
    }
    let start = from_x.min(to_x).floor().max(0.0) as u16;
    let end = from_x.max(to_x).floor().max(0.0) as u16;
    (start..=end).all(|x| floors.contains(&GridPosition { x, floor }))
}

fn shaft_distance(shaft: &ElevatorShaft, origin: Endpoint, destination: Endpoint) -> f32 {
    let x = f32::from(shaft.x) + f32::from(shaft.kind.spec().width) * 0.5;
    (origin.x - x).abs() + (destination.x - x).abs()
}

fn shaft_stops_at(shaft: &ElevatorShaft, floor: i16, facilities: &[Facility]) -> bool {
    if !shaft.served_floors.contains(&floor)
        || floor_is_inside_multistory_lobby(floor, shaft.x, shaft.kind.spec().width, facilities)
    {
        return false;
    }
    if shaft.kind != FacilityKind::ExpressElevator || floor < 1 {
        return true;
    }

    // Express cars stop above ground only at a real ground/sky lobby that
    // physically touches the shaft. A bare floor at a fifteenth-storey mark
    // is not a stop in the original game.
    (floor == 1 || floor.rem_euclid(15) == 0)
        && lobby_components(floor, facilities)
            .into_iter()
            .any(|component| component_touches_shaft(component, shaft))
}

fn landing_floors(
    kind: FacilityKind,
    x: u16,
    served_floors: &[i16],
    facilities: &[Facility],
) -> Vec<i16> {
    let mut floors = served_floors
        .iter()
        .copied()
        .filter(|floor| !floor_is_inside_multistory_lobby(*floor, x, kind.spec().width, facilities))
        .collect::<Vec<_>>();
    if floors.is_empty() {
        // A shaft may be built top-down through a grand lobby before its base
        // landing is added. Keep the temporary car inside the physical shaft;
        // routing still rejects these floors as passenger stops.
        floors.extend_from_slice(served_floors);
    }
    floors
}

fn normalize_elevator_home_floors(elevators: &mut [ElevatorShaft], facilities: &[Facility]) {
    for shaft in elevators {
        let valid_floors = landing_floors(shaft.kind, shaft.x, &shaft.served_floors, facilities);
        let default_floor = default_home_floor(&valid_floors);
        for car in &mut shaft.cars {
            if !valid_floors.contains(&car.home_floor) {
                car.home_floor = default_floor;
            }
        }
    }
}

fn floor_is_inside_multistory_lobby(
    floor: i16,
    transport_x: u16,
    transport_width: u16,
    facilities: &[Facility],
) -> bool {
    let transport_end = transport_x.saturating_add(transport_width);
    facilities.iter().any(|lobby| {
        lobby.kind == FacilityKind::Lobby
            && lobby.stories > 1
            && floor > lobby.position.floor
            && floor < lobby.position.floor + i16::from(lobby.stories) - 1
            && lobby.position.x <= transport_end
            && lobby.end_x() >= transport_x
    })
}

pub fn elevator_landing_is_valid(
    kind: FacilityKind,
    x: u16,
    floor: i16,
    facilities: &[Facility],
) -> bool {
    matches!(
        kind,
        FacilityKind::Elevator | FacilityKind::ServiceElevator | FacilityKind::ExpressElevator
    ) && !floor_is_inside_multistory_lobby(floor, x, kind.spec().width, facilities)
}

fn lobby_endpoint_connects_to_shaft(
    endpoint: Endpoint,
    shaft: &ElevatorShaft,
    facilities: &[Facility],
) -> bool {
    let Some(lobby) = facilities.iter().find(|facility| {
        facility.id == endpoint.facility_id && facility.kind == FacilityKind::Lobby
    }) else {
        return true;
    };
    lobby_components(endpoint.floor, facilities)
        .into_iter()
        .any(|component| {
            component.0 <= lobby.position.x
                && lobby.position.x < component.1
                && component_touches_shaft(component, shaft)
        })
}

fn lobby_connects_shafts(
    floor: i16,
    first: &ElevatorShaft,
    second: &ElevatorShaft,
    facilities: &[Facility],
) -> bool {
    lobby_components(floor, facilities)
        .into_iter()
        .any(|component| {
            component_touches_shaft(component, first) && component_touches_shaft(component, second)
        })
}

/// Returns half-open horizontal runs of contiguous lobby slices. Elevator
/// shafts may be painted over lobby artwork, so touching or overlapping a run
/// both count as a connection.
fn lobby_components(floor: i16, facilities: &[Facility]) -> Vec<(u16, u16)> {
    let mut columns = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Lobby && facility.position.floor == floor)
        .map(|facility| facility.position.x)
        .collect::<Vec<_>>();
    columns.sort_unstable();
    columns.dedup();

    let mut components = Vec::new();
    let Some(mut start) = columns.first().copied() else {
        return components;
    };
    let mut end = start + 1;
    for column in columns.into_iter().skip(1) {
        if column == end {
            end += 1;
        } else {
            components.push((start, end));
            start = column;
            end = column + 1;
        }
    }
    components.push((start, end));
    components
}

fn component_touches_shaft(component: (u16, u16), shaft: &ElevatorShaft) -> bool {
    let shaft_start = shaft.x;
    let shaft_end = shaft.x.saturating_add(shaft.kind.spec().width);
    component.0 <= shaft_end && component.1 >= shaft_start
}

fn connected_vertical_walkways(
    origin_floor: i16,
    destination_floor: i16,
    facilities: &[Facility],
) -> Vec<(FacilityKind, f32)> {
    let mut connected_walkways = Vec::new();
    for kind in [FacilityKind::Escalator, FacilityKind::Stairs] {
        let mut columns: Vec<u16> = facilities
            .iter()
            .filter(|facility| facility.kind == kind)
            .map(|facility| facility.position.x)
            .collect();
        columns.sort_unstable();
        columns.dedup();
        for x in columns {
            if floor_is_inside_multistory_lobby(origin_floor, x, kind.spec().width, facilities)
                || floor_is_inside_multistory_lobby(
                    destination_floor,
                    x,
                    kind.spec().width,
                    facilities,
                )
            {
                continue;
            }
            let mut ordinal = floor_ordinal(origin_floor);
            let target = floor_ordinal(destination_floor);
            let direction = if target > ordinal { 1 } else { -1 };
            let mut connected = true;
            while ordinal != target {
                let base_ordinal = if direction > 0 { ordinal } else { ordinal - 1 };
                if !facilities.iter().any(|facility| {
                    facility.kind == kind
                        && facility.position.x == x
                        && walkway_covers_flight(facility, base_ordinal, facilities)
                }) {
                    connected = false;
                    break;
                }
                ordinal += direction;
            }
            if connected {
                connected_walkways.push((kind, f32::from(x) + f32::from(kind.spec().width) * 0.5));
            }
        }
    }
    connected_walkways
}

fn walkway_covers_flight(
    walkway: &Facility,
    flight_base_ordinal: i16,
    facilities: &[Facility],
) -> bool {
    let walkway_base = floor_ordinal(walkway.position.floor);
    let flights = vertical_walkway_story_span(walkway, facilities).saturating_sub(1) as i16;
    flight_base_ordinal >= walkway_base && flight_base_ordinal < walkway_base + flights
}

/// Rendered and routed height of stairs or an escalator. A transport placed
/// in the bottom of a grand lobby expands through the entire open lobby,
/// rather than requiring an unreachable landing in its middle story.
pub fn vertical_walkway_story_span(walkway: &Facility, facilities: &[Facility]) -> u16 {
    vertical_walkway_story_span_at(walkway.kind, walkway.position, facilities)
}

pub fn vertical_walkway_story_span_at(
    kind: FacilityKind,
    position: GridPosition,
    facilities: &[Facility],
) -> u16 {
    let normal_height = kind.spec().height;
    if !matches!(kind, FacilityKind::Stairs | FacilityKind::Escalator) {
        return normal_height;
    }
    let end_x = position.x.saturating_add(kind.spec().width);
    facilities
        .iter()
        .filter(|lobby| {
            lobby.kind == FacilityKind::Lobby
                && lobby.position.floor == position.floor
                && lobby.stories > 1
                && position.x < lobby.end_x()
                && end_x > lobby.position.x
        })
        .map(|lobby| u16::from(lobby.stories))
        .max()
        .unwrap_or(normal_height)
        .max(normal_height)
}

fn start_trip(person: &mut Person) {
    person.activity = PersonActivity::Walking;
    person.wait_seconds = 0.0;
    person.mood = PersonMood::Calm;
    person.route_leg = 0;
}

fn arrive_at_facility(person: &mut Person) {
    person.activity = PersonActivity::Visiting;
    person.visit_remaining = if person.going_home {
        0.0
    } else {
        person.visit_duration
    };
    person.current_floor = trip_destination(person).floor;
    person.floor_position = f32::from(floor_ordinal(person.current_floor));
}

fn trip_destination(person: &Person) -> Endpoint {
    if person.going_home {
        person.origin
    } else {
        person.destination
    }
}

fn desired_elevator_floor(person: &Person) -> i16 {
    match person.route {
        TravelMode::ElevatorTransfer { transfer_floor, .. } if person.route_leg == 0 => {
            transfer_floor
        }
        _ => trip_destination(person).floor,
    }
}

fn reverse_route(route: TravelMode) -> TravelMode {
    match route {
        TravelMode::ElevatorTransfer {
            first_id,
            first_x,
            transfer_floor,
            second_id,
            second_x,
        } => TravelMode::ElevatorTransfer {
            first_id: second_id,
            first_x: second_x,
            transfer_floor,
            second_id: first_id,
            second_x: first_x,
        },
        other => other,
    }
}

fn endpoint(facility: &Facility) -> Endpoint {
    Endpoint {
        facility_id: facility.id,
        x: f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5,
        floor: facility.position.floor,
    }
}

fn facility_top_endpoint(facility: &Facility) -> Endpoint {
    let top_ordinal = floor_ordinal(facility.position.floor)
        + facility.kind.spec().height.saturating_sub(1) as i16;
    Endpoint {
        facility_id: facility.id,
        x: f32::from(facility.position.x) + f32::from(facility.kind.spec().width) * 0.5,
        floor: floor_from_ordinal(top_ordinal),
    }
}

const fn commercial_patronage_target(kind: FacilityKind) -> Option<u32> {
    match kind {
        FacilityKind::FastFood | FacilityKind::Restaurant => Some(35),
        FacilityKind::Shop => Some(25),
        FacilityKind::Cinema => Some(20),
        FacilityKind::PartyHall => Some(50),
        _ => None,
    }
}

const fn commercial_patronage_initial(kind: FacilityKind) -> u32 {
    match kind {
        // These attractions open with a complete event/audience rather than
        // slowly accumulating ordinary retail customers.
        FacilityKind::Cinema => 120,
        FacilityKind::PartyHall => 50,
        FacilityKind::FastFood | FacilityKind::Restaurant | FacilityKind::Shop => 4,
        _ => 0,
    }
}

fn exterior_spawn_x(tower: &Tower, seed: u64) -> f32 {
    let mut occupied_extents = tower
        .facilities()
        .iter()
        .filter(|facility| facility.position.floor == 1)
        .map(|facility| (facility.position.x, facility.end_x()));
    let occupied_bounds = occupied_extents.next().map(|first| {
        occupied_extents.fold(first, |(left, right), (start, end)| {
            (left.min(start), right.max(end))
        })
    });
    let mut first_floor_columns = tower
        .floors()
        .iter()
        .filter(|position| position.floor == 1)
        .map(|position| position.x);
    let floor_bounds = first_floor_columns.next().map(|first_column| {
        first_floor_columns.fold((first_column, first_column + 1), |(left, right), column| {
            (left.min(column), right.max(column + 1))
        })
    });
    let Some((left, right)) = occupied_bounds.or(floor_bounds) else {
        return if seed & 1 == 0 {
            -12.0
        } else {
            f32::from(tower.width()) + 12.0
        };
    };
    if seed & 1 == 0 {
        f32::from(left) - 12.0
    } else {
        f32::from(right) + 12.0
    }
}

fn is_person_destination(kind: FacilityKind) -> bool {
    matches!(
        kind,
        FacilityKind::Office
            | FacilityKind::HotelSingle
            | FacilityKind::HotelTwin
            | FacilityKind::HotelSuite
            | FacilityKind::Restaurant
            | FacilityKind::FastFood
            | FacilityKind::Shop
            | FacilityKind::Cinema
            | FacilityKind::PartyHall
            | FacilityKind::Medical
    )
}

pub fn facility_is_open(kind: FacilityKind, minute: u16) -> bool {
    match kind {
        FacilityKind::Office => (9 * 60..17 * 60).contains(&minute),
        FacilityKind::FastFood | FacilityKind::Shop => (10 * 60..21 * 60).contains(&minute),
        FacilityKind::Restaurant => (17 * 60..23 * 60).contains(&minute),
        FacilityKind::Cinema => (13 * 60..20 * 60).contains(&minute),
        FacilityKind::PartyHall => (17 * 60..23 * 60).contains(&minute),
        FacilityKind::Medical => (8 * 60..18 * 60).contains(&minute),
        _ => true,
    }
}

pub fn facility_is_open_at(kind: FacilityKind, clock: Clock) -> bool {
    if kind == FacilityKind::Office && is_weekend(clock) {
        return false;
    }
    match kind {
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => {
            (16 * 60..22 * 60).contains(&clock.minute_of_day)
        }
        _ => facility_is_open(kind, clock.minute_of_day),
    }
}

pub const fn is_weekend(clock: Clock) -> bool {
    (clock.day.saturating_sub(1) % 7) + 1 >= 6
}

fn visit_duration_seconds(kind: FacilityKind) -> f32 {
    match kind {
        // At normal speed the simulation advances two game minutes per real
        // second. These stays therefore track the guide's daily schedules.
        FacilityKind::Office => 300.0,
        FacilityKind::Condo => 360.0,
        FacilityKind::HotelSingle | FacilityKind::HotelTwin | FacilityKind::HotelSuite => 240.0,
        FacilityKind::Restaurant => 35.0,
        FacilityKind::FastFood => 18.0,
        FacilityKind::Shop => 28.0,
        FacilityKind::Cinema | FacilityKind::PartyHall => 60.0,
        FacilityKind::Medical => 24.0,
        _ => 5.0,
    }
}

fn default_home_floor(floors: &[i16]) -> i16 {
    if floors.contains(&1) {
        1
    } else {
        floors.first().copied().unwrap_or(1)
    }
}

pub fn floor_ordinal(floor: i16) -> i16 {
    if floor > 0 { floor - 1 } else { floor }
}

pub fn floor_from_ordinal(ordinal: i16) -> i16 {
    if ordinal >= 0 { ordinal + 1 } else { ordinal }
}

pub fn schedule_period(minute_of_day: u16) -> usize {
    match minute_of_day {
        0..=419 => 5,
        420..=719 => 0,
        720..=779 => 1,
        780..=1019 => 2,
        1020..=1259 => 3,
        _ => 4,
    }
}

pub fn schedule_index(clock: Clock) -> usize {
    schedule_period(clock.minute_of_day) + if is_weekend(clock) { 6 } else { 0 }
}

fn move_toward(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn next_spawn_interval(star_rating: u8, seed: u64) -> f32 {
    let rating = f32::from(star_rating.clamp(1, 5));
    let random_fraction = (mix64(seed.rotate_left(11)) % 1_000) as f32 / 1_000.0;
    // More highly rated towers attract more people, while a broad deterministic
    // jitter prevents the entrances and elevators from receiving metronomic
    // batches. At one star this is roughly 3.6-8.5 seconds per arrival; at five
    // stars it is about 0.7-1.7 seconds.
    (5.5 / rating) * (0.65 + random_fraction * 0.9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FacilityOccupancy, GridPosition};

    fn test_shaft(id: u64, kind: FacilityKind, x: u16, served_floors: &[i16]) -> ElevatorShaft {
        ElevatorShaft {
            id,
            kind,
            x,
            served_floors: served_floors.to_vec(),
            cars: Vec::new(),
            schedules: [ElevatorSchedule::default(); 12],
            visible: true,
        }
    }

    fn test_facility(id: u64, kind: FacilityKind, x: u16, floor: i16) -> Facility {
        Facility {
            id,
            kind,
            position: GridPosition { x, floor },
            occupancy: FacilityOccupancy::NotApplicable,
            price_level: 1,
            stories: kind.spec().height as u8,
        }
    }

    fn place_floor_run(tower: &mut Tower, floor: i16, width: u16) {
        for level in 1..=floor {
            for x in 0..width {
                if !tower.has_floor(GridPosition { x, floor: level }) {
                    tower.place_floor(GridPosition { x, floor: level }).unwrap();
                }
            }
        }
    }

    fn traffic_tower(with_upper_elevator: bool) -> Tower {
        let mut tower = Tower::new(64, 10_000_000);
        place_floor_run(&mut tower, 2, 64);
        tower
            .place(FacilityKind::Office, GridPosition { x: 0, floor: 2 })
            .unwrap();
        tower
            .place(FacilityKind::FastFood, GridPosition { x: 32, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 24, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 20, floor: 1 })
            .unwrap();
        if with_upper_elevator {
            tower
                .place(FacilityKind::Elevator, GridPosition { x: 20, floor: 2 })
                .unwrap();
        }
        tower.advance_time(60.0);
        tower
    }

    #[test]
    fn floor_ordinals_skip_zero_without_adding_travel_distance() {
        assert_eq!(floor_ordinal(-1), -1);
        assert_eq!(floor_ordinal(1), 0);
        assert_eq!(floor_ordinal(2), 1);
        assert_eq!(floor_from_ordinal(-1), -1);
        assert_eq!(floor_from_ordinal(0), 1);
    }

    #[test]
    fn contiguous_elevator_segments_form_one_shaft() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert_eq!(traffic.elevators.len(), 1);
        assert_eq!(traffic.elevators[0].served_floors, vec![1, 2]);
        assert_eq!(traffic.elevators[0].cars.len(), 1);
        assert_eq!(traffic.elevators[0].cars[0].home_floor, 1);
    }

    #[test]
    fn lobby_routes_are_cached_until_tower_topology_changes() {
        let mut tower = traffic_tower(true);
        let office_id = tower
            .facilities()
            .iter()
            .find(|facility| facility.kind == FacilityKind::Office)
            .unwrap()
            .id;
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert!(matches!(
            traffic.routes_from_lobby.get(&office_id),
            Some(TravelMode::Elevator { .. })
        ));

        // A sentinel route survives a no-op sync, demonstrating that the
        // costly path search is not repeated on every simulation frame.
        traffic
            .routes_from_lobby
            .insert(office_id, TravelMode::SameFloor);
        traffic.sync_with_tower(&tower);
        assert_eq!(
            traffic.routes_from_lobby.get(&office_id),
            Some(&TravelMode::SameFloor)
        );

        // Adding another tenant computes only that destination. Existing
        // routes stay untouched because corridors and transport did not
        // change.
        let second_office = tower
            .place(FacilityKind::Office, GridPosition { x: 9, floor: 2 })
            .unwrap();
        traffic.sync_with_tower(&tower);
        assert_eq!(
            traffic.routes_from_lobby.get(&office_id),
            Some(&TravelMode::SameFloor)
        );
        assert!(matches!(
            traffic.routes_from_lobby.get(&second_office),
            Some(TravelMode::Elevator { .. })
        ));

        for x in 20..24 {
            tower.place_floor(GridPosition { x, floor: 3 }).unwrap();
        }
        traffic.sync_with_tower_after_addition(&tower);
        assert_eq!(
            traffic.routes_from_lobby.get(&office_id),
            Some(&TravelMode::SameFloor)
        );

        let extension = tower
            .place(FacilityKind::Elevator, GridPosition { x: 20, floor: 3 })
            .unwrap();
        traffic.sync_with_tower_after_addition(&tower);
        assert_eq!(
            traffic.routes_from_lobby.get(&office_id),
            Some(&TravelMode::SameFloor)
        );
        assert_eq!(traffic.elevators[0].served_floors, vec![1, 2, 3]);

        tower.demolish(extension).unwrap();
        traffic.sync_with_tower(&tower);
        assert!(matches!(
            traffic.routes_from_lobby.get(&office_id),
            Some(TravelMode::Elevator { .. })
        ));
    }

    #[test]
    fn stair_demolition_rebuilds_only_routes_that_cross_the_removed_flight() {
        let mut tower = Tower::new(64, 10_000_000);
        place_floor_run(&mut tower, 1, 64);
        place_floor_run(&mut tower, 2, 64);
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 0, floor: 1 })
            .unwrap();
        let stairs = tower
            .place(FacilityKind::Stairs, GridPosition { x: 8, floor: 1 })
            .unwrap();
        let lower_office = tower
            .place(FacilityKind::Office, GridPosition { x: 20, floor: 1 })
            .unwrap();
        let upper_office = tower
            .place(FacilityKind::Office, GridPosition { x: 20, floor: 2 })
            .unwrap();

        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert!(matches!(
            traffic.routes_from_lobby.get(&upper_office),
            Some(TravelMode::Stairs { .. })
        ));

        // A sentinel on an unrelated route makes an unnecessary global
        // rebuild observable rather than relying on timing in the test.
        let sentinel = TravelMode::Elevator {
            shaft_id: u64::MAX,
            x: 999.0,
        };
        traffic.routes_from_lobby.insert(lower_office, sentinel);

        let removed = tower.demolish(stairs).unwrap();
        traffic.sync_with_tower_after_removal(&tower, &removed);

        assert_eq!(
            traffic.routes_from_lobby.get(&lower_office),
            Some(&sentinel)
        );
        assert!(!traffic.routes_from_lobby.contains_key(&upper_office));
        assert!(traffic.reachable_facilities.contains(&lower_office));
        assert!(!traffic.reachable_facilities.contains(&upper_office));
    }

    #[test]
    fn basement_floor_demolition_rebuilds_only_routes_crossing_that_cell() {
        let mut tower = Tower::new(64, 10_000_000);
        for x in 8..14 {
            tower.place_floor(GridPosition { x, floor: 1 }).unwrap();
        }
        for x in 8..64 {
            tower.place_floor(GridPosition { x, floor: -1 }).unwrap();
        }
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 12, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 8, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 8, floor: -1 })
            .unwrap();
        let near_office = tower
            .place(FacilityKind::Office, GridPosition { x: 16, floor: -1 })
            .unwrap();
        let far_office = tower
            .place(FacilityKind::Office, GridPosition { x: 40, floor: -1 })
            .unwrap();

        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert!(traffic.routes_from_lobby.contains_key(&near_office));
        assert!(traffic.routes_from_lobby.contains_key(&far_office));

        // A sentinel makes an accidental global route rebuild observable.
        let sentinel = TravelMode::Elevator {
            shaft_id: u64::MAX,
            x: 10.0,
        };
        traffic.routes_from_lobby.insert(near_office, sentinel);
        let removed = GridPosition { x: 30, floor: -1 };
        tower.demolish_floor(removed).unwrap();
        traffic.sync_with_tower_after_floor_removal(&tower, removed);

        assert_eq!(traffic.routes_from_lobby.get(&near_office), Some(&sentinel));
        assert!(!traffic.routes_from_lobby.contains_key(&far_office));
        assert!(traffic.reachable_facilities.contains(&near_office));
        assert!(!traffic.reachable_facilities.contains(&far_office));
    }

    #[test]
    fn first_elevator_car_starts_at_the_placement_floor() {
        let mut tower = Tower::new(32, 10_000_000);
        place_floor_run(&mut tower, 5, 32);
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 8, floor: 5 })
            .unwrap();

        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let car = &traffic.elevators[0].cars[0];
        assert_eq!(traffic.elevators[0].cars.len(), 1);
        assert_eq!(car.home_floor, 5);
        assert_eq!(car.floor_position, f32::from(floor_ordinal(5)));
    }

    #[test]
    fn people_visit_businesses_via_an_elevator() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        for _ in 0..240 {
            traffic.advance(&tower, 0.25);
        }
        assert!(!traffic.people.is_empty());
        assert!(traffic.people.iter().any(|person| {
            person.current_floor == 1
                || matches!(person.activity, PersonActivity::RidingElevator { .. })
        }));
    }

    #[test]
    fn a_waiting_person_progresses_from_calm_to_concerned_to_angry() {
        let mut tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        traffic.spawn_person(&tower);
        let shaft_id = traffic.elevators[0].id;
        let person = &mut traffic.people[0];
        person.activity = PersonActivity::WaitingForElevator { shaft_id };
        advance_people(&mut traffic.people, CONCERNED_WAIT_SECONDS + 0.1);
        assert_eq!(traffic.people[0].mood, PersonMood::Concerned);
        advance_people(
            &mut traffic.people,
            ANGRY_WAIT_SECONDS - CONCERNED_WAIT_SECONDS,
        );
        assert_eq!(traffic.people[0].mood, PersonMood::Angry);

        tower.clock.speed = crate::SimulationSpeed::Paused;
    }

    #[test]
    fn elevator_queues_are_capped_per_shaft_and_floor() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let shaft_id = traffic.elevators[0].id;
        for _ in 0..MAX_ELEVATOR_QUEUE_PER_FLOOR + 3 {
            assert!(traffic.spawn_person(&tower));
        }
        for person in &mut traffic.people {
            person.activity = PersonActivity::WaitingForElevator { shaft_id };
            person.current_floor = 1;
        }

        enforce_elevator_queue_limits(&mut traffic.people);

        assert_eq!(
            traffic
                .people
                .iter()
                .filter(|person| matches!(
                    person.activity,
                    PersonActivity::WaitingForElevator { .. }
                ))
                .count(),
            MAX_ELEVATOR_QUEUE_PER_FLOOR
        );
        assert_eq!(
            traffic
                .people
                .iter()
                .filter(|person| person.activity == PersonActivity::LeavingElevatorQueue)
                .count(),
            3
        );
    }

    #[test]
    fn angry_elevator_waiters_eventually_abandon_the_queue() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert!(traffic.spawn_person(&tower));
        let shaft_id = traffic.elevators[0].id;
        let person = &mut traffic.people[0];
        person.activity = PersonActivity::WaitingForElevator { shaft_id };
        person.wait_seconds = ABANDON_ELEVATOR_WAIT_SECONDS;

        enforce_elevator_queue_limits(&mut traffic.people);

        assert_eq!(
            traffic.people[0].activity,
            PersonActivity::LeavingElevatorQueue
        );
        assert_eq!(traffic.people[0].mood, PersonMood::Angry);
        advance_people(&mut traffic.people, QUEUE_ABANDON_WALK_SECONDS + 0.1);
        assert!(traffic.people[0].visit_remaining <= 0.0);
    }

    #[test]
    fn original_default_schedule_values_are_exposed() {
        let schedule = ElevatorSchedule::default();
        assert_eq!(schedule.mode, ElevatorMode::Local);
        assert_eq!(schedule.waiting_car_response, 5);
        assert_eq!(schedule.departure_seconds, 0);
        assert_eq!(ELEVATOR_CAPACITY, 17);
        assert_eq!(EXPRESS_ELEVATOR_CAPACITY, 34);
        assert_eq!(elevator_capacity(FacilityKind::ExpressElevator), 34);
    }

    #[test]
    fn express_elevators_only_stop_at_basements_ground_and_sky_lobbies() {
        let shaft = test_shaft(1, FacilityKind::ExpressElevator, 8, &[-2, -1, 1, 2, 15, 30]);
        let facilities = [
            test_facility(1, FacilityKind::Lobby, 8, 1),
            test_facility(2, FacilityKind::Lobby, 8, 15),
        ];
        assert!(shaft_stops_at(&shaft, -2, &facilities));
        assert!(shaft_stops_at(&shaft, 1, &facilities));
        assert!(shaft_stops_at(&shaft, 15, &facilities));
        assert!(!shaft_stops_at(&shaft, 2, &facilities));
        assert!(!shaft_stops_at(&shaft, 30, &facilities));
    }

    #[test]
    fn routing_chooses_the_closest_direct_elevator() {
        let elevators = [
            test_shaft(1, FacilityKind::Elevator, 40, &[1, 2]),
            test_shaft(2, FacilityKind::Elevator, 5, &[1, 2]),
        ];
        let route = choose_route(
            Endpoint {
                facility_id: 0,
                x: 0.0,
                floor: 1,
            },
            Endpoint {
                facility_id: 9,
                x: 12.0,
                floor: 2,
            },
            &elevators,
            &[],
            &[],
        );
        assert!(matches!(
            route,
            Some(TravelMode::Elevator { shaft_id: 2, .. })
        ));
    }

    #[test]
    fn housekeeping_routes_are_exclusive_to_service_elevators() {
        let public = test_shaft(1, FacilityKind::Elevator, 4, &[1, 2]);
        let express = test_shaft(2, FacilityKind::ExpressElevator, 10, &[1, 2]);
        let service = test_shaft(3, FacilityKind::ServiceElevator, 20, &[1, 2]);
        let origin = Endpoint {
            facility_id: 10,
            x: 0.0,
            floor: 1,
        };
        let destination = Endpoint {
            facility_id: 11,
            x: 30.0,
            floor: 2,
        };

        let housekeeping_route = choose_route_scored_for(
            origin,
            destination,
            &[public.clone(), express.clone(), service.clone()],
            &[],
            &[],
            ElevatorAccess::Housekeeping,
        )
        .map(|(route, _)| route);
        assert!(matches!(
            housekeeping_route,
            Some(TravelMode::Elevator { shaft_id: 3, .. })
        ));

        assert!(
            choose_route_scored_for(
                origin,
                destination,
                &[public, express],
                &[],
                &[],
                ElevatorAccess::Housekeeping,
            )
            .is_none()
        );
        let public_route = choose_route_scored_for(
            origin,
            destination,
            &[service],
            &[],
            &[],
            ElevatorAccess::Public,
        );
        assert!(public_route.is_none());
    }

    #[test]
    fn dispatched_housekeeper_uses_the_dedicated_worker_lifecycle() {
        let mut tower = Tower::new(64, 10_000_000);
        for floor in [1, 2] {
            for x in 0..64 {
                tower.place_floor(GridPosition { x, floor }).unwrap();
            }
        }
        let housekeeping_id = tower
            .place(FacilityKind::Housekeeping, GridPosition { x: 0, floor: 1 })
            .unwrap();
        tower
            .place(
                FacilityKind::ServiceElevator,
                GridPosition { x: 20, floor: 1 },
            )
            .unwrap();
        tower
            .place(
                FacilityKind::ServiceElevator,
                GridPosition { x: 20, floor: 2 },
            )
            .unwrap();
        let room_id = tower
            .place(FacilityKind::HotelSingle, GridPosition { x: 30, floor: 2 })
            .unwrap();
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);

        let worker_id = traffic
            .dispatch_housekeeper(&tower, housekeeping_id, room_id, 8.0)
            .expect("a reachable room should receive a housekeeping worker");
        let worker = traffic
            .people()
            .iter()
            .find(|person| person.id == worker_id)
            .unwrap();
        assert_eq!(worker.home_facility_id, housekeeping_id);
        assert_eq!(worker.destination_facility_id, room_id);
        let TravelMode::Elevator { shaft_id, .. } = worker.route else {
            panic!("housekeeping should use the service elevator");
        };
        assert_eq!(
            traffic
                .elevators()
                .iter()
                .find(|shaft| shaft.id == shaft_id)
                .unwrap()
                .kind,
            FacilityKind::ServiceElevator
        );
        assert!(
            traffic
                .dispatch_housekeeper(&tower, housekeeping_id, room_id, 8.0)
                .is_none(),
            "a dirty room must not receive duplicate workers"
        );

        let worker = traffic
            .people
            .iter_mut()
            .find(|person| person.id == worker_id)
            .unwrap();
        worker.route = TravelMode::SameFloor;
        worker.x = worker.destination.x;
        worker.current_floor = worker.destination.floor;
        worker.floor_position = f32::from(floor_ordinal(worker.destination.floor));
        worker.walking_from_transport = true;
        worker.activity = PersonActivity::Walking;
        traffic.advance_after_sync(&tower, 0.1);
        assert_eq!(traffic.people[0].activity, PersonActivity::Visiting);
        assert_eq!(traffic.visitors_at(room_id), 0);
        assert_eq!(traffic.daily_visitors_at(room_id), 0);
        assert!(!traffic.has_received_visitor(room_id));
    }

    #[test]
    fn routing_allows_one_change_at_a_lobby() {
        let elevators = [
            test_shaft(1, FacilityKind::ExpressElevator, 8, &[1, 15]),
            test_shaft(2, FacilityKind::Elevator, 24, &[15, 16, 17]),
        ];
        let mut lobbies = vec![test_facility(3, FacilityKind::Lobby, 8, 1)];
        lobbies.extend(
            (12..=24).map(|x| test_facility(100 + u64::from(x), FacilityKind::Lobby, x, 15)),
        );
        let route = choose_route(
            Endpoint {
                facility_id: 0,
                x: -12.0,
                floor: 1,
            },
            Endpoint {
                facility_id: 4,
                x: 40.0,
                floor: 17,
            },
            &elevators,
            &lobbies,
            &[],
        );
        assert!(matches!(
            route,
            Some(TravelMode::ElevatorTransfer {
                first_id: 1,
                transfer_floor: 15,
                second_id: 2,
                ..
            })
        ));
    }

    #[test]
    fn disconnected_sky_lobby_does_not_enable_an_elevator_transfer() {
        let elevators = [
            test_shaft(1, FacilityKind::ExpressElevator, 8, &[1, 15]),
            test_shaft(2, FacilityKind::Elevator, 24, &[15, 16, 17]),
        ];
        let lobbies = [
            test_facility(3, FacilityKind::Lobby, 8, 1),
            test_facility(4, FacilityKind::Lobby, 8, 15),
            test_facility(5, FacilityKind::Lobby, 24, 15),
        ];
        let route = choose_route(
            Endpoint {
                facility_id: 3,
                x: 8.5,
                floor: 1,
            },
            Endpoint {
                facility_id: 9,
                x: 40.0,
                floor: 17,
            },
            &elevators,
            &lobbies,
            &[],
        );
        assert!(route.is_none());
    }

    #[test]
    fn express_elevator_needs_a_real_lobby_at_each_above_ground_stop() {
        let elevators = [test_shaft(1, FacilityKind::ExpressElevator, 8, &[1, 15])];
        let ground_lobby = test_facility(3, FacilityKind::Lobby, 8, 1);
        let route = choose_route(
            endpoint(&ground_lobby),
            Endpoint {
                facility_id: 9,
                x: 20.0,
                floor: 15,
            },
            &elevators,
            &[ground_lobby],
            &[],
        );
        assert!(route.is_none());
    }

    #[test]
    fn elevators_skip_the_open_middle_of_a_grand_lobby() {
        let shaft = test_shaft(1, FacilityKind::Elevator, 8, &[1, 2, 3, 4]);
        let mut lobby = test_facility(2, FacilityKind::Lobby, 12, 1);
        lobby.stories = 3;
        assert!(shaft_stops_at(&shaft, 1, &[lobby.clone()]));
        assert!(!shaft_stops_at(&shaft, 2, &[lobby.clone()]));
        assert!(shaft_stops_at(&shaft, 3, &[lobby.clone()]));
        assert!(shaft_stops_at(&shaft, 4, &[lobby.clone()]));

        let mut saved_shaft = shaft;
        saved_shaft.cars.push(ElevatorCar {
            floor_position: f32::from(floor_ordinal(2)),
            home_floor: 2,
            direction: ElevatorDirection::Idle,
            target_floor: None,
            passengers: Vec::new(),
            dwell_remaining: 0.0,
        });
        normalize_elevator_home_floors(std::slice::from_mut(&mut saved_shaft), &[lobby]);
        assert_eq!(saved_shaft.cars[0].home_floor, 1);
    }

    #[test]
    fn one_walkway_expands_through_a_three_story_lobby() {
        let walkway = test_facility(1, FacilityKind::Escalator, 8, 1);
        let mut lobby = test_facility(2, FacilityKind::Lobby, 8, 1);
        lobby.stories = 3;
        let facilities = [walkway.clone(), lobby];
        assert_eq!(vertical_walkway_story_span(&walkway, &facilities), 3);
        assert!(
            connected_vertical_walkways(1, 3, &facilities)
                .iter()
                .any(|(kind, _)| *kind == FacilityKind::Escalator)
        );
        assert!(connected_vertical_walkways(1, 2, &facilities).is_empty());
    }

    #[test]
    fn stairs_cover_four_flights_and_escalators_cover_three() {
        let stairs = (1..=5)
            .map(|floor| test_facility(floor as u64, FacilityKind::Stairs, 8, floor))
            .collect::<Vec<_>>();
        assert!(matches!(
            choose_route(
                Endpoint {
                    facility_id: 0,
                    x: 0.0,
                    floor: 1,
                },
                Endpoint {
                    facility_id: 8,
                    x: 20.0,
                    floor: 5,
                },
                &[],
                &stairs,
                &[],
            ),
            Some(TravelMode::Stairs { .. })
        ));
        assert!(
            choose_route(
                Endpoint {
                    facility_id: 0,
                    x: 0.0,
                    floor: 1,
                },
                Endpoint {
                    facility_id: 8,
                    x: 20.0,
                    floor: 6,
                },
                &[],
                &stairs,
                &[],
            )
            .is_none()
        );

        let escalators = (1..=4)
            .map(|floor| test_facility(floor as u64, FacilityKind::Escalator, 16, floor))
            .collect::<Vec<_>>();
        assert!(matches!(
            choose_route(
                Endpoint {
                    facility_id: 0,
                    x: 0.0,
                    floor: 1,
                },
                Endpoint {
                    facility_id: 10,
                    x: 20.0,
                    floor: 4,
                },
                &[],
                &escalators,
                &[],
            ),
            Some(TravelMode::Stairs { .. })
        ));
        assert!(
            choose_route(
                Endpoint {
                    facility_id: 0,
                    x: 0.0,
                    floor: 1,
                },
                Endpoint {
                    facility_id: 10,
                    x: 20.0,
                    floor: 5,
                },
                &[],
                &escalators,
                &[],
            )
            .is_none()
        );
    }

    #[test]
    fn people_prefer_an_elevator_as_the_stair_climb_gets_longer() {
        let stairs = [
            test_facility(1, FacilityKind::Stairs, 8, 1),
            test_facility(2, FacilityKind::Stairs, 8, 2),
            test_facility(3, FacilityKind::Stairs, 8, 3),
        ];
        let elevator = [test_shaft(9, FacilityKind::Elevator, 8, &[1, 2, 3, 4])];
        let route = choose_route(
            Endpoint {
                facility_id: 0,
                x: 0.0,
                floor: 1,
            },
            Endpoint {
                facility_id: 10,
                x: 20.0,
                floor: 4,
            },
            &elevator,
            &stairs,
            &[],
        );
        assert!(matches!(
            route,
            Some(TravelMode::Elevator { shaft_id: 9, .. })
        ));
    }

    #[test]
    fn tenant_reachability_requires_a_first_floor_lobby_and_complete_path() {
        let mut tower = Tower::new(64, 10_000_000);
        place_floor_run(&mut tower, 2, 64);
        let office_id = tower
            .place(FacilityKind::Office, GridPosition { x: 40, floor: 2 })
            .unwrap();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 32, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 32, floor: 2 })
            .unwrap();
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert!(!traffic.reachable_facility_ids(&tower).contains(&office_id));

        for x in 24..=32 {
            tower
                .place(FacilityKind::Lobby, GridPosition { x, floor: 1 })
                .unwrap();
        }
        traffic.sync_with_tower(&tower);
        assert!(traffic.reachable_facility_ids(&tower).contains(&office_id));

        let mut split_tower = Tower::new(64, 10_000_000);
        for x in 0..16 {
            split_tower
                .place_floor(GridPosition { x, floor: 1 })
                .unwrap();
        }
        for x in 32..64 {
            split_tower
                .place_floor(GridPosition { x, floor: 1 })
                .unwrap();
            split_tower
                .place_floor(GridPosition { x, floor: 2 })
                .unwrap();
        }
        split_tower
            .place(FacilityKind::Lobby, GridPosition { x: 4, floor: 1 })
            .unwrap();
        let split_office = split_tower
            .place(FacilityKind::Office, GridPosition { x: 40, floor: 2 })
            .unwrap();
        split_tower
            .place(FacilityKind::Elevator, GridPosition { x: 32, floor: 1 })
            .unwrap();
        split_tower
            .place(FacilityKind::Elevator, GridPosition { x: 32, floor: 2 })
            .unwrap();
        traffic.sync_with_tower(&split_tower);
        assert!(
            !traffic
                .reachable_facility_ids(&split_tower)
                .contains(&split_office)
        );
    }

    #[test]
    fn businesses_only_generate_arrivals_during_open_hours() {
        assert!(!facility_is_open(FacilityKind::FastFood, 9 * 60 + 59));
        assert!(facility_is_open(FacilityKind::FastFood, 10 * 60));
        assert!(facility_is_open(FacilityKind::FastFood, 20 * 60 + 59));
        assert!(!facility_is_open(FacilityKind::FastFood, 21 * 60));
        assert!(!facility_is_open(FacilityKind::Office, 8 * 60 + 59));
        assert!(facility_is_open(FacilityKind::Office, 9 * 60));
        assert!(!facility_is_open(FacilityKind::Office, 17 * 60));
        assert!(!facility_is_open(FacilityKind::Restaurant, 16 * 60 + 59));
        assert!(facility_is_open(FacilityKind::Restaurant, 17 * 60));
        assert!(facility_is_open(FacilityKind::Restaurant, 22 * 60));
        assert!(!facility_is_open(FacilityKind::Restaurant, 23 * 60));
    }

    #[test]
    fn a_shaft_can_have_multiple_independent_cars() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let shaft_id = traffic.elevators[0].id;
        assert!(traffic.add_elevator_car(&tower, shaft_id, 2));
        assert_eq!(traffic.elevators[0].cars.len(), 2);
        assert_eq!(traffic.elevators[0].cars[0].home_floor, 1);
        assert_eq!(traffic.elevators[0].cars[1].home_floor, 2);
        assert!(!traffic.add_elevator_car(&tower, shaft_id, 3));
    }

    #[test]
    fn a_car_keeps_its_assigned_hall_call_while_approaching() {
        let tower = traffic_tower(true);
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let shaft_id = traffic.elevators[0].id;
        assert!(traffic.add_elevator_car(&tower, shaft_id, 2));
        assert!(traffic.spawn_person(&tower));

        let person = &mut traffic.people[0];
        person.current_floor = 1;
        person.floor_position = f32::from(floor_ordinal(1));
        person.destination.floor = 2;
        person.route = TravelMode::Elevator {
            shaft_id,
            x: f32::from(traffic.elevators[0].x),
        };
        person.activity = PersonActivity::WaitingForElevator { shaft_id };

        let cars = &mut traffic.elevators[0].cars;
        cars[0].direction = ElevatorDirection::Up;
        cars[0].target_floor = Some(2);
        cars[1].direction = ElevatorDirection::Down;
        cars[1].target_floor = Some(1);

        let assignments = assign_waiting_calls(&traffic.elevators[0], &traffic.people, 5);
        assert_eq!(assignments, vec![(1, 1)]);
    }

    #[test]
    fn an_elevator_shaft_accepts_at_most_eight_cars() {
        let mut tower = Tower::new(32, 10_000_000);
        place_floor_run(&mut tower, 8, 32);
        for floor in 1..=8 {
            tower
                .place(FacilityKind::Elevator, GridPosition { x: 8, floor })
                .unwrap();
        }
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        let shaft_id = traffic.elevators[0].id;
        for floor in 2..=8 {
            assert!(traffic.add_elevator_car(&tower, shaft_id, floor));
        }
        assert_eq!(traffic.elevators[0].cars.len(), MAX_ELEVATOR_CARS);
        assert!(!traffic.add_elevator_car(&tower, shaft_id, 8));
        assert_eq!(traffic.elevators[0].cars.len(), MAX_ELEVATOR_CARS);
    }

    #[test]
    fn extending_an_existing_shaft_does_not_create_another_car() {
        let mut tower = Tower::new(32, 10_000_000);
        place_floor_run(&mut tower, 2, 32);
        tower
            .place(FacilityKind::Elevator, GridPosition { x: 8, floor: 1 })
            .unwrap();
        let mut traffic = TrafficSimulation::new();
        traffic.sync_with_tower(&tower);
        assert_eq!(traffic.elevators[0].cars.len(), 1);

        tower
            .place(FacilityKind::Elevator, GridPosition { x: 8, floor: 2 })
            .unwrap();
        traffic.sync_with_tower(&tower);
        assert_eq!(traffic.elevators[0].served_floors, vec![1, 2]);
        assert_eq!(traffic.elevators[0].cars.len(), 1);
    }

    #[test]
    fn every_person_enters_from_outside_and_eventually_leaves() {
        let mut tower = Tower::new(64, 10_000_000);
        place_floor_run(&mut tower, 1, 64);
        tower
            .place(FacilityKind::FastFood, GridPosition { x: 24, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 20, floor: 1 })
            .unwrap();
        tower.advance_time(60.0);
        let mut traffic = TrafficSimulation::new();
        for _ in 0..32 {
            traffic.advance(&tower, 0.25);
        }
        assert!(!traffic.people.is_empty());
        assert!(
            traffic
                .people
                .iter()
                .all(|person| person.home_facility_id == 0)
        );

        for _ in 0..1_600 {
            traffic.advance(&tower, 0.25);
        }
        assert!(traffic.people.len() <= usize::from(tower.star_rating()) * 32);
    }

    #[test]
    fn occupied_offices_receive_people_from_outside() {
        let mut tower = Tower::new(32, 10_000_000);
        place_floor_run(&mut tower, 1, 32);
        let office_id = tower
            .place(FacilityKind::Office, GridPosition { x: 12, floor: 1 })
            .unwrap();
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 4, floor: 1 })
            .unwrap();
        tower.advance_time(30.0);

        let mut traffic = TrafficSimulation::new();
        for _ in 0..800 {
            traffic.advance(&tower, 0.25);
        }

        assert!(traffic.visitors_at(office_id) > 0);
        assert!(traffic.has_received_visitor(office_id));
        assert!(
            traffic
                .people()
                .iter()
                .all(|person| person.home_facility_id == 0)
        );
    }

    #[test]
    fn weekday_office_arrivals_are_balanced_across_reachable_tenants() {
        let mut tower = Tower::new(160, 100_000_000);
        place_floor_run(&mut tower, 1, 160);
        tower
            .place(FacilityKind::Lobby, GridPosition { x: 4, floor: 1 })
            .unwrap();
        let office_ids: Vec<_> = (0..12)
            .map(|index| {
                tower
                    .place(
                        FacilityKind::Office,
                        GridPosition {
                            x: 16 + index * 10,
                            floor: 1,
                        },
                    )
                    .unwrap()
            })
            .collect();
        tower.advance_time(30.0);
        tower.clock.day = 2;
        tower.clock.minute_of_day = 12 * 60;

        let mut traffic = TrafficSimulation::new();
        for _ in 0..480 {
            traffic.advance(&tower, 0.25);
        }

        for office_id in office_ids {
            assert!(
                traffic.visitors_at(office_id) > 0,
                "reachable occupied office {office_id} should have weekday workers by noon"
            );
        }
        assert!(
            traffic.people().iter().all(|person| {
                person.origin.x < 4.0 || person.origin.x > 16.0 + 11.0 * 10.0 + 9.0
            })
        );
    }

    #[test]
    fn higher_ratings_generate_shorter_but_jittered_arrival_intervals() {
        let low = next_spawn_interval(1, 10);
        let high = next_spawn_interval(5, 10);
        assert!(high < low);
        assert_ne!(next_spawn_interval(3, 10), next_spawn_interval(3, 11));
    }

    #[test]
    fn condo_family_roles_follow_the_original_weekday_and_weekend_routine() {
        let home = test_facility(1, FacilityKind::Condo, 8, 2);
        let weekday_morning = Clock {
            day: 1,
            minute_of_day: 8 * 60 + 30,
            ..Clock::default()
        };
        let morning = scheduled_trip_specs(weekday_morning, &home, 1);
        assert!(morning.iter().any(|trip| {
            trip.role == PersonRole::Child && trip.purpose == ScheduledTripPurpose::Outside
        }));
        assert!(morning.iter().any(|trip| {
            trip.role == PersonRole::Salesman && trip.purpose == ScheduledTripPurpose::Outside
        }));

        let shopping = scheduled_trip_specs(
            Clock {
                day: 1,
                minute_of_day: 10 * 60 + 30,
                ..Clock::default()
            },
            &home,
            1,
        );
        assert_eq!(
            shopping,
            vec![ScheduledTripSpec {
                event: ScheduledTripEvent::ResidentShopping,
                role: PersonRole::Woman,
                purpose: ScheduledTripPurpose::Shopping,
                visit_seconds: 28.0,
            }]
        );

        let weekend = scheduled_trip_specs(
            Clock {
                day: 6,
                minute_of_day: 12 * 60,
                ..Clock::default()
            },
            &home,
            1,
        );
        assert_eq!(
            weekend.iter().map(|trip| trip.role).collect::<Vec<_>>(),
            [
                PersonRole::WomanWithChild,
                PersonRole::Child,
                PersonRole::Homebody,
            ]
        );
        assert!(
            weekend
                .iter()
                .all(|trip| trip.purpose == ScheduledTripPurpose::Meal)
        );
    }

    #[test]
    fn compact_schedule_stamps_keep_each_event_independent() {
        let home = 42;
        assert_ne!(
            scheduled_trip_stamp_key(home, ScheduledTripEvent::WeekendMotherMeal),
            scheduled_trip_stamp_key(home, ScheduledTripEvent::WeekendChildMeal)
        );
        assert_eq!(
            scheduled_trip_stamp_key(home, ScheduledTripEvent::WeekendChildMeal) >> 5,
            home
        );
    }

    #[test]
    fn offices_close_on_weekends_and_commercial_demand_matures() {
        let saturday = Clock {
            day: 6,
            minute_of_day: 10 * 60,
            ..Clock::default()
        };
        assert!(!facility_is_open_at(FacilityKind::Office, saturday));

        let mut traffic = TrafficSimulation::new();
        traffic.commercial_patronage.insert(7, 35);
        assert_eq!(
            traffic.dedicated_patronage_at(7, FacilityKind::FastFood, false),
            35
        );
        assert_eq!(
            traffic.dedicated_patronage_at(7, FacilityKind::FastFood, true),
            48
        );
        traffic.commercial_patronage.insert(8, 25);
        assert_eq!(
            traffic.dedicated_patronage_at(8, FacilityKind::Shop, true),
            30
        );
    }

    #[test]
    fn offices_hotels_and_cinemas_generate_their_secondary_trips() {
        let office = test_facility(1, FacilityKind::Office, 8, 4);
        let sales = scheduled_trip_specs(
            Clock {
                day: 1,
                minute_of_day: 10 * 60 + 30,
                ..Clock::default()
            },
            &office,
            3,
        );
        assert!(sales.iter().any(|trip| {
            trip.event == ScheduledTripEvent::OfficeSalesCall
                && trip.purpose == ScheduledTripPurpose::OfficeCall
        }));

        let hotel = test_facility(2, FacilityKind::HotelSuite, 16, 10);
        assert!(
            scheduled_trip_specs(
                Clock {
                    day: 6,
                    minute_of_day: 19 * 60,
                    ..Clock::default()
                },
                &hotel,
                3,
            )
            .iter()
            .any(|trip| trip.event == ScheduledTripEvent::HotelDinner)
        );

        let cinema = test_facility(3, FacilityKind::Cinema, 24, 8);
        assert!(
            scheduled_trip_specs(
                Clock {
                    day: 6,
                    minute_of_day: 20 * 60,
                    ..Clock::default()
                },
                &cinema,
                3,
            )
            .iter()
            .any(|trip| trip.event == ScheduledTripEvent::CinemaEveningDispersal)
        );
    }

    #[test]
    fn a_new_cinema_program_restores_opening_demand() {
        let mut traffic = TrafficSimulation::new();
        traffic.commercial_patronage.insert(9, 20);
        assert!(traffic.change_cinema_program(9));
        assert_eq!(
            traffic.dedicated_patronage_at(9, FacilityKind::Cinema, false),
            120
        );
    }
}
