use slotmap::{SlotMap, new_key_type};

#[derive(Default)]
pub struct Simulation {
    pub(crate) entities: SlotMap<EntityId, EntityData>,
    pub(crate) locations: SlotMap<LocationId, LocationData>,
    pub(crate) parties: SlotMap<PartyId, PartyData>,
}

impl Simulation {
    pub fn new() -> Simulation {
        let mut sim = Simulation::default();
        init(&mut sim);
        sim
    }
    pub fn tick(&mut self, request: TickRequest) -> SimView {
        self::tick(self, request)
    }
}

new_key_type! { pub struct EntityId; }
new_key_type! { pub(crate) struct LocationId; }
new_key_type! { pub(crate) struct PartyId; }

#[derive(Default)]
pub(crate) struct EntityData {
    pub name: String,
    pub party: Option<PartyId>,
    pub location: Option<LocationId>,
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

impl V2 {
    pub const MIN: V2 = V2::splat(f32::MIN);
    pub const MAX: V2 = V2::splat(f32::MAX);

    pub const ZERO: V2 = V2::splat(0.);

    pub const fn splat(v: f32) -> Self {
        Self::new(v, v)
    }

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<V2> for (f32, f32) {
    fn from(value: V2) -> Self {
        (value.x, value.y)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Extents {
    pub top_left: V2,
    pub bottom_right: V2,
}

impl Default for Extents {
    fn default() -> Self {
        Self {
            top_left: V2::MIN,
            bottom_right: V2::MAX,
        }
    }
}

impl Extents {
    fn contains(&self, point: V2) -> bool {
        point.x >= self.top_left.x
            && point.y >= self.top_left.y
            && point.x <= self.bottom_right.x
            && point.y <= self.bottom_right.y
    }
}

#[derive(Default)]
pub(crate) struct LocationData {
    pub entity: EntityId,
}

pub(crate) struct PartyData {
    pub entity: EntityId,
    pub pos: V2,
    pub size: f32,
}

#[derive(Default)]
pub struct TickRequest {
    pub map_viewport: Extents,
}

fn init(sim: &mut Simulation) {
    {
        let entity = sim.entities.insert(EntityData {
            name: "Rome".to_string(),
            ..Default::default()
        });

        let party = sim.parties.insert(PartyData {
            entity,
            pos: V2::ZERO,
            size: 2.,
        });

        let location = sim.locations.insert(LocationData { entity });

        let entity = &mut sim.entities[entity];
        entity.party = Some(party);
        entity.location = Some(location);
    }
}

fn tick(sim: &mut Simulation, request: TickRequest) -> SimView {
    let mut view = SimView::default();
    view.map_items = map_view_items(sim, request.map_viewport);
    view
}

#[derive(Default)]
pub struct SimView {
    pub map_items: Vec<MapItem>,
}

fn map_view_items(sim: &Simulation, viewport: Extents) -> Vec<MapItem> {
    sim.parties
        .values()
        .filter(|party| viewport.contains(party.pos))
        .map(|party| {
            let entity = &sim.entities[party.entity];
            MapItem {
                id: party.entity,
                name: entity.name.clone(),
                pos: party.pos,
                size: party.size,
            }
        })
        .collect()
}

pub struct MapItem {
    pub id: EntityId,
    pub name: String,
    pub pos: V2,
    pub size: f32,
}
