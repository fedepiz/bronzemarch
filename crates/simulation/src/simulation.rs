use slotmap::{SlotMap, new_key_type};

#[derive(Default)]
pub struct Simulation {
    pub(crate) sites: Sites,
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct SiteId(usize);

#[derive(Default)]
pub(crate) struct SiteData {
    pub tag: String,
    pub pos: V2,
    pub neighbours: Vec<SiteId>,
}

#[derive(Default)]
pub(crate) struct Sites {
    entries: Vec<SiteData>,
}

impl Sites {
    pub fn define(&mut self, tag: impl Into<String>, pos: V2) -> SiteId {
        let id = SiteId(self.entries.len());
        self.entries.push(SiteData {
            tag: tag.into(),
            pos,
            neighbours: vec![],
        });
        id
    }

    pub fn connect(&mut self, id1: SiteId, id2: SiteId) {
        Self::insert_no_repeat(&mut self.entries[id1.0].neighbours, id2);
        Self::insert_no_repeat(&mut self.entries[id2.0].neighbours, id1);
    }

    fn insert_no_repeat(vs: &mut Vec<SiteId>, id: SiteId) {
        if !vs.contains(&id) {
            vs.push(id);
        }
    }

    pub fn lookup<'a>(&'a self, tag: &str) -> Option<(SiteId, &'a SiteData)> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, data)| data.tag.as_str() == tag)
            .map(|(id, data)| (SiteId(id), data))
    }

    pub fn get(&self, id: SiteId) -> Option<&SiteData> {
        self.entries.get(id.0)
    }
}

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

impl From<(f32, f32)> for V2 {
    fn from((x, y): (f32, f32)) -> Self {
        Self::new(x, y)
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
    pub site: SiteId,
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
    // Init sites
    {
        const DESCS: &[(&str, (f32, f32))] = &[("rome", (0., 0.)), ("florence", (-5., -10.))];
        for &(tag, pos) in DESCS {
            sim.sites.define(tag, pos.into());
        }

        const CONNECTIONS: &[(&str, &str)] = &[("rome", "florence")];

        for (tag1, tag2) in CONNECTIONS {
            let id1 = match sim.sites.lookup(&tag1) {
                Some((id, _)) => id,
                None => {
                    println!("Unknown site '{tag1}'");
                    continue;
                }
            };
            let id2 = match sim.sites.lookup(&tag2) {
                Some((id, _)) => id,
                None => {
                    println!("Unknown site '{tag2}'");
                    continue;
                }
            };
            sim.sites.connect(id1, id2);
        }
    }

    // Init some settlements
    {
        struct Desc<'a> {
            name: &'a str,
            site: &'a str,
        }

        let descs = [
            Desc {
                name: "Rome",
                site: "rome",
            },
            Desc {
                name: "Florence",
                site: "florence",
            },
        ];

        for desc in descs {
            let (site_id, site_data) = match sim.sites.lookup(desc.site) {
                Some(site) => site,
                None => {
                    println!("Unknown site '{}'", desc.site);
                    continue;
                }
            };

            let entity = sim.entities.insert(EntityData {
                name: desc.name.to_string(),
                ..Default::default()
            });

            let party = sim.parties.insert(PartyData {
                entity,
                pos: site_data.pos,
                size: 2.,
            });

            let location = sim.locations.insert(LocationData {
                entity,
                site: site_id,
            });

            let entity = &mut sim.entities[entity];
            entity.party = Some(party);
            entity.location = Some(location);
        }
    }
}

fn tick(sim: &mut Simulation, request: TickRequest) -> SimView {
    let mut view = SimView::default();
    view.map_items = map_view_items(sim, request.map_viewport);
    view.map_lines = map_view_lines(sim, request.map_viewport);
    view
}

#[derive(Default)]
pub struct SimView {
    pub map_lines: Vec<(V2, V2)>,
    pub map_items: Vec<MapItem>,
}

fn map_view_lines(sim: &Simulation, viewport: Extents) -> Vec<(V2, V2)> {
    let mut out = Vec::with_capacity(100);
    for (idx, site) in sim.sites.entries.iter().enumerate() {
        if !viewport.contains(site.pos) {
            continue;
        }
        for &neigh_id in &site.neighbours {
            if neigh_id.0 >= idx {
                continue;
            }
            let destination = sim.sites.get(neigh_id).unwrap().pos;
            if !viewport.contains(destination) {
                continue;
            }
            out.push((site.pos, destination));
        }
    }
    out
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
