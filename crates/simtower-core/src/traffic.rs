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
pub const MAX_STAIR_FLIGHTS: u16 = 6;
pub const MAX_ESCALATOR_FLIGHTS: u16 = 8;
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
    topology_signature: u64,
    #[serde(default)]
    current_visitors: HashMap<u64, usize>,
    #[serde(default)]
    daily_visits: HashMap<u64, u32>,
    #[serde(default)]
    patronage_day: u32,
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
            topology_signature: 0,
            current_visitors: HashMap::new(),
            daily_visits: HashMap::new(),
            patronage_day: 0,
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
                || !facility_ids.contains(&person.destination_facility_id)
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

    pub fn elevator_mut(&mut self, shaft_id: u64) -> Option<&mut ElevatorShaft> {
        self.elevators.iter_mut().find(|shaft| shaft.id == shaft_id)
    }

    pub fn elevator_at(&self, kind: FacilityKind, x: u16) -> Option<&ElevatorShaft> {
        self.elevators
            .iter()
            .find(|shaft| shaft.kind == kind && shaft.x == x)
    }

    pub fn add_elevator_car(&mut self, shaft_id: u64, home_floor: i16) -> bool {
        let Some(shaft) = self.elevator_mut(shaft_id) else {
            return false;
        };
        if shaft.cars.len() >= MAX_ELEVATOR_CARS || !shaft.served_floors.contains(&home_floor) {
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
        let signature = tower_topology_signature(tower);
        let route_cache_complete = self.routes_from_lobby.len() == self.reachable_facilities.len()
            && self
                .reachable_facilities
                .iter()
                .all(|facility_id| self.routes_from_lobby.contains_key(facility_id));
        if self.topology_signature == signature && route_cache_complete {
            return;
        }

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
                for car in &mut existing.cars {
                    if !existing.served_floors.contains(&car.home_floor) {
                        car.home_floor = default_home_floor(&existing.served_floors);
                    }
                }
                synchronized.push(existing);
            } else {
                let home_floor = default_home_floor(&served_floors);
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

        self.routes_from_lobby = tower
            .facilities()
            .iter()
            .filter(|facility| facility.kind.has_tenant_occupancy())
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
        self.reachable_facilities = self.routes_from_lobby.keys().copied().collect();
        self.topology_signature = signature;

        let live_facilities: HashSet<u64> = tower
            .facilities()
            .iter()
            .map(|facility| facility.id)
            .collect();
        self.people.retain(|person| {
            (person.home_facility_id == 0 || live_facilities.contains(&person.home_facility_id))
                && live_facilities.contains(&person.destination_facility_id)
        });
        self.arrived_facilities
            .retain(|facility_id| live_facilities.contains(facility_id));
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
        }
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
            !person.going_home
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
                .filter(|person| !person.going_home && person.activity == PersonActivity::Visiting)
                .map(|person| person.destination_facility_id),
        );
        for shaft in &mut self.elevators {
            advance_shaft(shaft, &mut self.people, tower.clock, elapsed_seconds);
        }
        self.people.retain(|person| {
            !(person.going_home
                && person.activity == PersonActivity::Visiting
                && person.visit_remaining <= 0.0)
                && !(person.activity == PersonActivity::LeavingElevatorQueue
                    && person.visit_remaining <= 0.0)
        });
        self.refresh_current_visitors();
    }

    fn refresh_current_visitors(&mut self) {
        self.current_visitors.clear();
        for person in self
            .people
            .iter()
            .filter(|person| person.activity == PersonActivity::Visiting && !person.going_home)
        {
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
                    || !facility_is_open(facility.kind, tower.clock.minute_of_day)
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
        let (destination, route) = selected;
        // Every simulated person has a complete entrance-to-exit lifecycle.
        // Nobody materializes in an office, residence, hotel room, or business.
        let origin = Endpoint {
            facility_id: 0,
            x: exterior_spawn_x(tower, seed),
            floor: 1,
        };
        let destination_kind = destination.kind;
        let destination = endpoint(destination);

        let mut person = Person {
            id: self.next_person_id,
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
    clock: Clock,
    elapsed_seconds: f32,
) {
    let schedule = shaft.current_schedule(clock);
    let waiting_assignments = assign_waiting_calls(shaft, people, schedule.waiting_car_response);
    let top_floor = shaft.served_floors.last().copied();
    let bottom_floor = shaft.served_floors.first().copied();
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
            moving.or(waiting).or(nearest).map(|index| (floor, index))
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

fn tower_topology_signature(tower: &Tower) -> u64 {
    let mut signature = mix64(u64::from(tower.width()) ^ 0x544f_504f_4c4f_4759);
    for floor in tower.floors() {
        let floor_bits = u64::from(floor.x) | ((floor.floor as i64 as u64) << 16);
        signature = mix64(signature ^ floor_bits);
    }
    signature = mix64(signature ^ tower.floors().len() as u64);
    for facility in tower.facilities() {
        let position_bits = u64::from(facility.position.x)
            | ((facility.position.floor as i64 as u64) << 16)
            | (u64::from(facility.stories) << 32);
        signature = mix64(signature ^ facility.id ^ ((facility.kind as u64) << 48) ^ position_bits);
    }
    mix64(signature ^ tower.facilities().len() as u64)
}

fn choose_route_scored(
    origin: Endpoint,
    destination: Endpoint,
    elevators: &[ElevatorShaft],
    facilities: &[Facility],
    floors: &[GridPosition],
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

    for shaft in elevators.iter().filter(|shaft| {
        let x = shaft_center(shaft);
        shaft.kind != FacilityKind::ServiceElevator
            && shaft_stops_at(shaft, origin.floor)
            && shaft_stops_at(shaft, destination.floor)
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

    let lobby_floors: Vec<i16> = facilities
        .iter()
        .filter(|facility| facility.kind == FacilityKind::Lobby)
        .map(|facility| facility.position.floor)
        .collect();
    for first in elevators.iter().filter(|shaft| {
        let x = shaft_center(shaft);
        shaft.kind != FacilityKind::ServiceElevator
            && shaft_stops_at(shaft, origin.floor)
            && horizontal_floor_path(origin.floor, origin.x, x, floors)
    }) {
        for second in elevators.iter().filter(|second| {
            let x = shaft_center(second);
            first.id != second.id
                && second.kind != FacilityKind::ServiceElevator
                && shaft_stops_at(second, destination.floor)
                && horizontal_floor_path(destination.floor, x, destination.x, floors)
        }) {
            let Some(transfer_floor) = lobby_floors.iter().copied().find(|floor| {
                shaft_stops_at(first, *floor)
                    && shaft_stops_at(second, *floor)
                    && horizontal_floor_path(
                        *floor,
                        shaft_center(first),
                        shaft_center(second),
                        floors,
                    )
            }) else {
                continue;
            };
            let first_x = shaft_center(first);
            let second_x = shaft_center(second);
            let score = (origin.x - first_x).abs()
                + (first_x - second_x).abs()
                + (destination.x - second_x).abs()
                + 30.0
                + flights as f32 * 0.5;
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

    candidates
        .into_iter()
        .min_by(|left, right| left.1.total_cmp(&right.1))
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

fn shaft_stops_at(shaft: &ElevatorShaft, floor: i16) -> bool {
    shaft.served_floors.contains(&floor)
        && (shaft.kind != FacilityKind::ExpressElevator
            || floor < 1
            || floor == 1
            || floor.rem_euclid(15) == 0)
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
            let mut ordinal = floor_ordinal(origin_floor);
            let target = floor_ordinal(destination_floor);
            let direction = if target > ordinal { 1 } else { -1 };
            let mut connected = true;
            while ordinal != target {
                let base_ordinal = if direction > 0 { ordinal } else { ordinal - 1 };
                let base = floor_from_ordinal(base_ordinal);
                if !facilities.iter().any(|facility| {
                    facility.kind == kind
                        && facility.position.x == x
                        && facility.position.floor == base
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
            | FacilityKind::Condo
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
    let weekend = clock.day.is_multiple_of(3);
    schedule_period(clock.minute_of_day) + if weekend { 6 } else { 0 }
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

        tower.place_floor(GridPosition { x: 63, floor: 3 }).unwrap();
        traffic.sync_with_tower(&tower);
        assert!(matches!(
            traffic.routes_from_lobby.get(&office_id),
            Some(TravelMode::Elevator { .. })
        ));
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
        assert!(shaft_stops_at(&shaft, -2));
        assert!(shaft_stops_at(&shaft, 1));
        assert!(shaft_stops_at(&shaft, 15));
        assert!(!shaft_stops_at(&shaft, 2));
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
    fn routing_allows_one_change_at_a_lobby() {
        let elevators = [
            test_shaft(1, FacilityKind::ExpressElevator, 8, &[1, 15]),
            test_shaft(2, FacilityKind::Elevator, 24, &[15, 16, 17]),
        ];
        let lobby = test_facility(3, FacilityKind::Lobby, 0, 15);
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
            &[lobby],
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
    fn connected_stairs_cover_six_flights_and_escalators_cover_eight() {
        let stairs = (1..=7)
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
                    floor: 7,
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
                    floor: 8,
                },
                &[],
                &stairs,
                &[],
            )
            .is_none()
        );

        let escalators = (1..=9)
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
                    floor: 9,
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
                    floor: 10,
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

        tower
            .place(FacilityKind::Lobby, GridPosition { x: 24, floor: 1 })
            .unwrap();
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
        assert!(traffic.add_elevator_car(shaft_id, 2));
        assert_eq!(traffic.elevators[0].cars.len(), 2);
        assert_eq!(traffic.elevators[0].cars[0].home_floor, 1);
        assert_eq!(traffic.elevators[0].cars[1].home_floor, 2);
        assert!(!traffic.add_elevator_car(shaft_id, 3));
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
            assert!(traffic.add_elevator_car(shaft_id, floor));
        }
        assert_eq!(traffic.elevators[0].cars.len(), MAX_ELEVATOR_CARS);
        assert!(!traffic.add_elevator_car(shaft_id, 8));
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
}
