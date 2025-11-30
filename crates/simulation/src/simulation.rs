use slotmap::*;
use std::collections::*;
use strum::EnumCount;
use util::arena::*;
use util::hierarchy::Hierarchy;
use util::tally::Tally;

use crate::date::Date;
use crate::sites::*;
use crate::tick::TickRequest;
use crate::tokens::*;

#[derive(Default)]
pub struct Simulation {
    pub(crate) date: Date,
    pub(crate) sites: Sites,
    pub(crate) good_types: GoodTypes,
    pub(crate) tokens: Tokens,
    pub(crate) entities: Entities,
    pub(crate) parties: Parties,
    pub(crate) agents: Agents,
    pub(crate) locations: Locations,
}

new_key_type! { pub (crate) struct EntityId; }
impl ArenaSafe for EntityId {}
new_key_type! { pub(crate) struct AgentId; }
impl ArenaSafe for AgentId {}

new_key_type! { pub(crate) struct LocationId; }
new_key_type! { pub(crate) struct PartyId; }

new_key_type! { pub(crate) struct GoodId; }

pub(crate) type GoodTypes = SlotMap<GoodId, GoodData>;
pub(crate) type Entities = SlotMap<EntityId, EntityData>;
pub(crate) type Locations = SlotMap<LocationId, LocationData>;
pub(crate) type Parties = SlotMap<PartyId, PartyData>;

impl Simulation {
    pub fn new() -> Simulation {
        let mut sim = Simulation::default();
        init(&mut sim);
        sim
    }

    pub fn tick(&mut self, request: TickRequest, arena: &Arena) -> crate::view::SimView {
        crate::tick::tick(self, request, arena)
    }
}

pub(crate) trait Tagged {
    fn tag(&self) -> &str;
}

pub(crate) trait TaggedCollection {
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

fn parse_tally_sm<K: Key, T: Tagged>(
    coll: &SlotMap<K, T>,
    items: &[(&str, f64)],
    kind_name: &str,
) -> SecondaryMap<K, f64> {
    let mut out: SecondaryMap<K, f64> = coll.keys().map(|id| (id, 0.)).collect();
    for (tag, value) in items {
        match coll.lookup(tag) {
            Some(id) => out[id] += *value,
            None => println!("Undefined {kind_name} with tag '{tag}'"),
        }
    }
    out
}

pub(crate) struct GoodData {
    pub tag: &'static str,
    pub name: &'static str,
    pub price: f64,
    pub food_rate: f64,
}

impl Tagged for GoodData {
    fn tag(&self) -> &str {
        self.tag
    }
}

pub(crate) struct Tags<T: Copy> {
    string_to_id: HashMap<String, T>,
}

impl<T: Copy> Default for Tags<T> {
    fn default() -> Self {
        Self {
            string_to_id: HashMap::default(),
        }
    }
}

impl<T: Copy> Tags<T> {
    pub fn insert(&mut self, tag: impl Into<String>, id: T) {
        self.string_to_id.insert(tag.into(), id);
    }

    pub fn remove(&mut self, tag: &str) {
        self.string_to_id.remove(tag);
    }

    pub fn lookup(&self, tag: &str) -> Option<T> {
        self.string_to_id.get(tag).copied()
    }
}

#[derive(Default)]
pub(crate) struct Agents {
    pub entries: SlotMap<AgentId, AgentData>,
    pub tags: Tags<AgentId>,
    pub political_hierarchy: Hierarchy<AgentId, AgentId>,
}

impl Agents {
    pub fn insert(&mut self, data: AgentData) -> AgentId {
        self.entries.insert(data)
    }
}

impl std::ops::Index<AgentId> for Agents {
    type Output = AgentData;

    fn index(&self, index: AgentId) -> &Self::Output {
        &self.entries[index]
    }
}

impl std::ops::IndexMut<AgentId> for Agents {
    fn index_mut(&mut self, index: AgentId) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Default)]
pub(crate) struct AgentData {
    pub entity: EntityId,
    pub flags: AgentFlags,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumCount)]
pub(crate) enum AgentFlag {
    IsFaction,
}

#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct AgentFlags([bool; AgentFlag::COUNT]);

impl AgentFlags {
    pub fn new(flags: &[AgentFlag]) -> Self {
        let mut this = Self::default();
        for &flag in flags {
            this.set(flag, true);
        }
        this
    }
    pub fn set(&mut self, flag: AgentFlag, value: bool) {
        let idx = flag as usize;
        self.0[idx] = value;
    }
    pub fn get(&self, flag: AgentFlag) -> bool {
        let idx = flag as usize;
        self.0[idx]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum RelatedAgent {
    Faction,
    Country,
}

pub(crate) fn query_related_agent(
    agents: &Agents,
    subject: AgentId,
    query: RelatedAgent,
) -> Option<(AgentId, &AgentData)> {
    enum HierarchyTraversal {
        Parent,
        Root,
    }

    struct Plan<'a> {
        hierarchy: &'a Hierarchy<AgentId, AgentId>,
        traversal: HierarchyTraversal,
        flags: &'a [AgentFlag],
    }

    let plan = match query {
        RelatedAgent::Faction => Plan {
            hierarchy: &agents.political_hierarchy,
            traversal: HierarchyTraversal::Parent,
            flags: &[AgentFlag::IsFaction],
        },
        RelatedAgent::Country => Plan {
            hierarchy: &agents.political_hierarchy,
            traversal: HierarchyTraversal::Root,
            flags: &[AgentFlag::IsFaction],
        },
    };

    let target = match plan.traversal {
        HierarchyTraversal::Parent => plan.hierarchy.parent(subject),
        HierarchyTraversal::Root => plan.hierarchy.root_parent(subject),
    }?;

    let target_data = &agents.entries[target];
    let all_flags_check = plan.flags.iter().all(|&flag| target_data.flags.get(flag));
    if !all_flags_check {
        return None;
    }

    Some((target, target_data))
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
pub(crate) struct EntityData {
    pub name: String,
    pub kind_name: &'static str,
    pub agent: Option<AgentId>,
    pub party: Option<PartyId>,
    pub location: Option<LocationId>,
    pub tokens: Option<TokenContainerId>,
}
pub(crate) struct LocationData {
    pub entity: EntityId,
    pub site: SiteId,
    pub population: i64,
    pub prosperity: f64,
    pub market: Market,
    pub influence_sources: Vec<InfluenceSource>,
}

pub(crate) struct InfluenceSource {
    pub kind: InfluenceKind,
    pub population_modifier: f64,
}

#[derive(Default)]
pub(crate) struct MarketGood {
    pub stock: f64,
    pub stock_delta: f64,
    pub price: f64,
    pub target_price: f64,
    pub supply_base: f64,
    pub supply_from_stock: f64,
    pub supply_effective: f64,
    pub demand_base: f64,
    pub demand_effective: f64,
    pub consumed: f64,
    pub satisfaction: f64,
}

pub(crate) struct Market {
    pub goods: SecondaryMap<GoodId, MarketGood>,
    pub food_consumed: f64,
    pub food_stockpile: f64,
    pub income: f64,
}

impl Market {
    pub fn new(good_types: &GoodTypes) -> Self {
        Self {
            goods: good_types
                .iter()
                .map(|(id, typ)| {
                    (
                        id,
                        MarketGood {
                            price: typ.price,
                            target_price: typ.price,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            food_consumed: 0.,
            food_stockpile: 0.,
            income: 0.,
        }
    }
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
            GridCoord::between(start, end, t)
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

    pub fn touches(self, site: SiteId) -> bool {
        match self {
            Self::At(x) => x == site,
            Self::Between(a, b, _) => site == a || site == b,
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

    pub fn is_colinear(self, other: Self) -> bool {
        match (self, other) {
            (Self::At(x), Self::At(y)) => x == y,
            (Self::At(x), Self::Between(a, b, _)) => x == a || x == b,
            (Self::Between(a, b, _), Self::At(x)) => x == a || x == b,
            (Self::Between(a1, b1, _), Self::Between(a2, b2, _)) => a1 == a2 && b1 == b2,
        }
    }

    pub fn as_colinear(p1: Self, p2: Self) -> Option<ColinearPair> {
        let (a1, b1, t1) = p1.as_triple();
        let (a2, b2, t2) = p2.as_triple();
        let start = a1.min(a2);
        let end = b1.max(b2);

        let mut num_differences = 0;
        if a1 != a2 {
            num_differences += 1
        }
        if b1 != b2 {
            num_differences += 1
        }

        if a1 == b1 && a2 == b2 {
            // Both are endpoitns, we don't care about differences
            num_differences = 0;
        }

        if num_differences > 1 {
            return None;
        }

        let t1 = if a1 == end { 1.0 } else { t1 };
        let t2 = if a2 == end { 1.0 } else { t2 };
        Some(ColinearPair { start, end, t1, t2 })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ColinearPair {
    pub start: SiteId,
    pub end: SiteId,
    pub t1: f32,
    pub t2: f32,
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

    pub fn iter(&self) -> impl Iterator<Item = GridCoord> {
        self.0.iter().rev().copied()
    }
}

pub(crate) struct PartyData {
    pub entity: EntityId,
    pub position: GridCoord,
    pub pos: V2,
    pub size: f32,
    pub layer: u8,
    pub movement_speed: f32,
    pub movement: PartyMovement,
}

#[derive(Clone, Copy)]
pub(crate) enum MovementTarget {
    Site(SiteId),
    Party(PartyId),
}

#[derive(Default)]
pub(crate) struct PartyMovement {
    pub target: Option<MovementTarget>,
    pub path: Path,
    pub destination: Option<GridCoord>,
}

fn init(sim: &mut Simulation) {
    sim.date = Date::with_calendar(1, 1, 363);
    // Init goods
    {
        struct Desc<'a> {
            tag: &'a str,
            name: &'a str,
            price: f64,
            food_rate: f64,
        }

        const DESCS: &[Desc] = &[
            Desc {
                tag: "wheat",
                name: "Wheat",
                price: 10.,
                food_rate: 1.0,
            },
            Desc {
                tag: "meat",
                name: "Meat",
                price: 10.,
                food_rate: 1.,
            },
            Desc {
                tag: "lumber",
                name: "Lumber",
                price: 10.,
                food_rate: 0.0,
            },
            Desc {
                tag: "tools",
                name: "Tools",
                price: 20.,
                food_rate: 0.0,
            },
        ];

        for desc in DESCS {
            sim.good_types.insert(GoodData {
                tag: desc.tag,
                name: desc.name,
                price: desc.price,
                food_rate: desc.food_rate,
            });
        }
    }

    // Init pops
    {
        struct Desc {
            tag: &'static str,
            name: &'static str,
            demand: &'static [(&'static str, f64)],
            rgo_points: f64,
        }

        const DESCS: &[Desc] = &[
            Desc {
                tag: "paesants",
                name: "Paesants",
                demand: &[("wheat", 1.0), ("lumber", 0.1)],
                rgo_points: 1.0,
            },
            Desc {
                tag: "artisans",
                name: "Artisans",
                demand: &[
                    ("wheat", 1.0),
                    ("meat", 0.2),
                    ("lumber", 0.1),
                    ("tools", 1.0),
                ],
                rgo_points: 0.,
            },
            Desc {
                tag: "nobles",
                name: "Nobles",
                demand: &[("wheat", 1.0), ("meat", 1.0), ("lumber", 0.1)],
                rgo_points: 0.,
            },
        ];

        for desc in DESCS {
            sim.tokens.define_type(TokenType {
                tag: desc.tag,
                name: desc.name,
                category: TokenCategory::Pop,
                supply: Default::default(),
                demand: parse_tally_sm(&sim.good_types, desc.demand, "goods"),
                rgo_points: desc.rgo_points,
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
                outputs: &[("tools", 100.)],
            },
        ];

        for desc in DESCS {
            sim.tokens.define_type(TokenType {
                tag: desc.tag,
                name: desc.name,
                category: TokenCategory::Building,
                demand: parse_tally_sm(&sim.good_types, desc.inputs, "goods"),
                supply: parse_tally_sm(&sim.good_types, desc.outputs, "goods"),
                rgo_points: 0.,
            });
        }
    }
    // Init sites
    {
        struct Desc {
            tag: &'static str,
            pos: (f32, f32),
            rgo: &'static [(&'static str, f64)],
        }

        const NORMAL_COUNTRYSIDE_RGO: &[(&str, f64)] = &[("wheat", 1.2), ("lumber", 0.5)];

        const DESCS: &[Desc] = &[
            Desc {
                tag: "caer_ligualid",
                pos: (0., 0.),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
            Desc {
                tag: "din_drust",
                pos: (-7., -9.),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
            Desc {
                tag: "anava",
                pos: (7., -5.),
                rgo: &[("wheat", 1.6)],
            },
            Desc {
                tag: "llan_heledd",
                pos: (3., 12.),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
            Desc {
                tag: "caer_ligualid-din_drust",
                pos: (-4., -4.),
                rgo: &[],
            },
            Desc {
                tag: "caer_ligualid_south",
                pos: (0., 8.),
                rgo: &[],
            },
            Desc {
                tag: "isura",
                pos: (-13., -8.),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
            Desc {
                tag: "isura_west",
                pos: (-19.5, -10.),
                rgo: &[],
            },
            Desc {
                tag: "din_rheged",
                pos: (-25., -8.4),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
            Desc {
                tag: "ad_candidam_casam",
                pos: (-19., -6.2),
                rgo: NORMAL_COUNTRYSIDE_RGO,
            },
        ];

        for desc in DESCS {
            let rgo = SiteRGO {
                rates: parse_tally(&sim.good_types, desc.rgo, "goods"),
                capacity: 5_000,
            };
            sim.sites.define(desc.tag, desc.pos.into(), rgo);
        }

        const CONNECTIONS: &[(&str, &str)] = &[
            ("caer_ligualid", "anava"),
            ("din_drust", "anava"),
            ("caer_ligualid", "caer_ligualid_south"),
            ("caer_ligualid_south", "llan_heledd"),
            ("caer_ligualid", "caer_ligualid-din_drust"),
            ("din_drust", "caer_ligualid-din_drust"),
            ("din_drust", "isura"),
            ("isura", "isura_west"),
            ("isura_west", "din_rheged"),
            ("isura_west", "ad_candidam_casam"),
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
        }
    }
}
