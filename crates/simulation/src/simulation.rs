use std::collections::{BTreeMap, BTreeSet};

use slotmap::*;
use util::tally::Tally;

use crate::date::Date;
use crate::tick::TickRequest;

#[derive(Default)]
pub struct Simulation {
    pub(crate) date: Date,
    pub(crate) sites: Sites,
    pub(crate) good_types: GoodTypes,
    pub(crate) building_types: BuildingTypes,
    pub(crate) entities: Entities,
    pub(crate) locations: Locations,
    pub(crate) parties: Parties,
    pub(crate) people: People,
    pub(crate) buildings: Buildings,
}

pub(crate) type GoodTypes = SlotMap<GoodId, GoodData>;
pub(crate) type BuildingTypes = SlotMap<BuildingTypeId, BuildingType>;
pub(crate) type Entities = SlotMap<EntityId, EntityData>;
pub(crate) type Locations = SlotMap<LocationId, LocationData>;
pub(crate) type Parties = SlotMap<PartyId, PartyData>;
pub(crate) type People = SlotMap<PersonId, PersonData>;
pub(crate) type Buildings = SlotMap<BuildingId, BuildingData>;

impl Simulation {
    pub fn new() -> Simulation {
        let mut sim = Simulation::default();
        init(&mut sim);
        sim
    }

    pub fn tick(&mut self, request: TickRequest) -> crate::view::SimView {
        crate::tick::tick(self, request)
    }
}

trait Tagged {
    fn tag(&self) -> &str;
}

trait TaggedCollection {
    type Output;

    fn lookup(&self, tag: &str) -> Option<Self::Output>;
}

impl<K: slotmap::Key, V: Tagged> TaggedCollection for SlotMap<K, V> {
    type Output = K;

    fn lookup(&self, tag: &str) -> Option<Self::Output> {
        self.iter()
            .find(|(_, data)| data.tag() == tag)
            .map(|(id, _)| id)
    }
}

fn parse_tally<C: TaggedCollection>(
    coll: &C,
    items: &[(&str, f64)],
    kind_name: &str,
) -> Tally<C::Output>
where
    C::Output: Copy + Ord,
{
    let mut out = Tally::new();
    for (tag, value) in items {
        match coll.lookup(tag) {
            Some(id) => out.add_one(id, *value),
            None => println!("Undefined {kind_name} with tag '{tag}'"),
        }
    }
    out
}

new_key_type! { pub(crate) struct GoodId; }

pub(crate) struct GoodData {
    pub tag: &'static str,
    pub name: &'static str,
    pub price: f64,
}

impl Tagged for GoodData {
    fn tag(&self) -> &str {
        self.tag
    }
}

new_key_type! { pub(crate) struct BuildingTypeId; }

pub(crate) struct BuildingType {
    pub tag: &'static str,
    pub name: &'static str,
    pub inputs: Tally<GoodId>,
    pub outputs: Tally<GoodId>,
}

impl Tagged for BuildingType {
    fn tag(&self) -> &str {
        self.tag
    }
}

new_key_type! { pub struct EntityId; }
new_key_type! { pub(crate) struct LocationId; }
new_key_type! { pub(crate) struct PartyId; }
new_key_type! { pub(crate) struct PersonId; }

#[derive(Default)]
pub(crate) struct EntityData {
    pub name: String,
    pub person: Option<PersonId>,
    pub party: Option<PartyId>,
    pub location: Option<LocationId>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug, Hash)]
pub(crate) struct SiteId(usize);

#[derive(Default)]
pub(crate) struct SiteData {
    pub tag: String,
    pub pos: V2,
    pub neighbours: Vec<(SiteId, f32)>,
    pub location: Option<LocationId>,
}

#[derive(Default)]
pub(crate) struct Sites {
    entries: Vec<SiteData>,
    distances: BTreeMap<(SiteId, SiteId), f32>,
}

impl Sites {
    pub fn define(&mut self, tag: impl Into<String>, pos: V2) -> SiteId {
        let id = SiteId(self.entries.len());
        self.entries.push(SiteData {
            tag: tag.into(),
            pos,
            neighbours: vec![],
            location: None,
        });
        id
    }

    pub fn connect(&mut self, id1: SiteId, id2: SiteId) {
        let distance = self.entries[id1.0].pos.distance(self.entries[id2.0].pos);
        Self::insert_no_repeat(&mut self.entries[id1.0].neighbours, id2, distance);
        Self::insert_no_repeat(&mut self.entries[id2.0].neighbours, id1, distance);
    }

    fn insert_no_repeat(vs: &mut Vec<(SiteId, f32)>, id: SiteId, distance: f32) {
        if vs.iter().all(|x| x.0 != id) {
            vs.push((id, distance));
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

    pub fn bind_location(&mut self, id: SiteId, location: LocationId) {
        if let Some(site) = self.entries.get_mut(id.0) {
            assert!(site.location.is_none());
            site.location = Some(location);
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = (SiteId, &'a SiteData)> + use<'a> {
        self.entries
            .iter()
            .enumerate()
            .map(|(idx, data)| (SiteId(idx), data))
    }

    pub fn neighbours(&self, id: SiteId) -> &[(SiteId, f32)] {
        &self.entries[id.0].neighbours
    }

    pub fn greater_neighbours(&self, id: SiteId) -> impl Iterator<Item = SiteId> + use<'_> {
        self.entries
            .get(id.0)
            .into_iter()
            .flat_map(|data| data.neighbours.iter().copied())
            .filter(move |&x| x.0 > id)
            .map(|x| x.0)
    }

    pub fn distance(&self, id1: SiteId, id2: SiteId) -> f32 {
        if id1 == id2 {
            return 0.;
        }
        let a = id1.min(id2);
        let b = id1.max(id2);
        self.distances
            .get(&(a, b))
            .copied()
            .unwrap_or(f32::INFINITY)
    }
}

new_key_type! { pub(crate) struct BuildingId; }

pub(crate) struct BuildingData {
    pub typ: BuildingTypeId,
    pub location: LocationId,
    pub size: i64,
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

    pub fn distance(&self, other: V2) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
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
    pub(crate) fn contains(&self, point: V2) -> bool {
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
    pub buildings: BTreeSet<BuildingId>,
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub(crate) enum GridCoord {
    At(SiteId),
    Between(SiteId, SiteId, f32),
}

impl GridCoord {
    pub fn with_triple(a: SiteId, b: SiteId, t: f32) -> GridCoord {
        if t == 0.0 {
            Self::At(a)
        } else if t == 1. {
            Self::At(b)
        } else {
            let start = a.min(b);
            let end = a.max(b);
            let t = if start == a { t } else { 1. - t };
            GridCoord::Between(start, end, t)
        }
    }

    pub fn at(site: SiteId) -> Self {
        Self::At(site)
    }

    pub fn between(a: SiteId, b: SiteId, t: f32) -> Self {
        assert!(t >= 0. && t <= 1.);
        if a == b {
            return Self::At(a);
        }
        let (a, b, t) = if a < b { (a, b, t) } else { (b, a, 1. - t) };
        Self::Between(a, b, t)
    }

    pub fn as_triple(self) -> (SiteId, SiteId, f32) {
        match self {
            Self::At(x) => (x, x, 0.),
            Self::Between(a, b, t) => (a, b, t),
        }
    }

    pub fn closest_endpoint(self) -> SiteId {
        match self {
            Self::At(x) => x,
            Self::Between(a, b, t) => {
                if t <= 0.5 {
                    a
                } else {
                    b
                }
            }
        }
    }
}

#[derive(Clone, Default, Debug)]
pub(crate) struct Path(Vec<GridCoord>);

impl Path {
    pub fn new(mut steps: Vec<GridCoord>) -> Self {
        steps.reverse();
        Self(steps)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn beginning(&self) -> Option<GridCoord> {
        self.0.last().copied()
    }

    pub fn endpoint(&self) -> Option<GridCoord> {
        self.0.first().copied()
    }

    pub fn advance(&mut self) {
        self.0.pop();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(crate) struct PartyData {
    pub entity: EntityId,
    pub position: GridCoord,
    pub destination: GridCoord,
    pub path: Path,
    pub pos: V2,
    pub size: f32,
    pub movement_speed: f32,
    pub contents: PartyContents,
}

#[derive(Default)]
pub(crate) struct PartyContents {
    pub leader: Option<PersonId>,
    pub people: BTreeSet<PersonId>,
}

pub(crate) struct PersonData {
    pub entity: EntityId,
    pub party: Option<PartyId>,
}

fn init(sim: &mut Simulation) {
    sim.date = Date::with_calendar(1, 1, 363);
    // Init goods
    {
        struct Desc<'a> {
            tag: &'a str,
            name: &'a str,
            price: f64,
        }

        const DESCS: &[Desc] = &[
            Desc {
                tag: "wheat",
                name: "Wheat",
                price: 10.,
            },
            Desc {
                tag: "lumber",
                name: "Lumber",
                price: 10.,
            },
            Desc {
                tag: "tools",
                name: "Tools",
                price: 20.,
            },
        ];

        for desc in DESCS {
            sim.good_types.insert(GoodData {
                tag: desc.tag,
                name: desc.name,
                price: desc.price,
            });
        }
    }

    // Init buildings
    {
        struct Desc<'a> {
            tag: &'a str,
            name: &'a str,
            inputs: &'a [(&'a str, f64)],
            outputs: &'a [(&'a str, f64)],
        }

        const DESCS: &[Desc] = &[
            Desc {
                tag: "wheat_farm",
                name: "Wheat Farm",
                inputs: &[],
                outputs: &[("wheat", 100.)],
            },
            Desc {
                tag: "lumber_field",
                name: "Lumber Field",
                inputs: &[],
                outputs: &[("lumber", 100.)],
            },
            Desc {
                tag: "toolmaker",
                name: "Toolmaker",
                inputs: &[("lumber", 10.)],
                outputs: &[("tools", 10.)],
            },
        ];

        for desc in DESCS {
            let inputs = parse_tally(&sim.good_types, desc.inputs, "good");
            let outputs = parse_tally(&sim.good_types, desc.inputs, "good");
            sim.building_types.insert(BuildingType {
                tag: desc.tag,
                name: desc.name,
                inputs,
                outputs,
            });
        }
    }
    // Init sites
    {
        const DESCS: &[(&str, (f32, f32))] = &[
            ("caer_ligualid", (0., 0.)),
            ("din_drust", (-6., -9.)),
            ("anava", (7., -3.)),
            ("llan_heledd", (1., 12.)),
        ];
        for &(tag, pos) in DESCS {
            sim.sites.define(tag, pos.into());
        }

        const CONNECTIONS: &[(&str, &str)] = &[
            ("caer_ligualid", "anava"),
            ("caer_ligualid", "din_drust"),
            ("din_drust", "anava"),
            ("caer_ligualid", "llan_heledd"),
        ];

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
            if id1 < id2 {
                let p1 = sim.sites.entries[id1.0].pos;
                let p2 = sim.sites.entries[id2.0].pos;
                let distance = p1.distance(p2);
                sim.sites.distances.insert((id1, id2), distance);
            }
        }
    }
}
