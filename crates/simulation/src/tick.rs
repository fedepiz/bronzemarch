use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::IteratorRandom;

use crate::date::Date;
use crate::object::*;
use crate::simulation::*;
use crate::view;
use crate::view::*;

#[derive(Default)]
pub struct TickRequest {
    pub commands: TickCommands,
    pub advance_time: bool,
    pub map_viewport: Extents,
    pub objects_to_extract: Vec<ObjectId>,
}

fn key_rng<K: slotmap::Key>(date: Date, key: K) -> SmallRng {
    let num = key.data().as_ffi();
    let seed = date
        .epoch()
        .wrapping_mul(13)
        .wrapping_add(num.wrapping_mul(17));
    SmallRng::seed_from_u64(seed)
}

pub(crate) fn tick(sim: &mut Simulation, mut request: TickRequest) -> SimView {
    if request.advance_time {
        sim.date.advance();

        let result = tick_party_ai(sim);
        for update in result {
            let ai = &mut sim.parties[update.id].ai;
            ai.target = update.target;
            ai.destination = update.destination;
        }

        // Pathfinding
        for (id, update) in pathfind(&sim.parties, &sim.sites) {
            let ai_data = &mut sim.parties[id].ai;
            match update {
                ChangePath::Keep => {}
                ChangePath::Clear => ai_data.path.clear(),
                ChangePath::Set(steps) => ai_data.path = Path::new(steps),
            }
        }

        // Update coordinates and positions
        let movements = move_to_next_coord(&sim.parties, &sim.sites);
        for movement in movements {
            let party = &mut sim.parties[movement.party_id];
            party.position = movement.next_position;
            if movement.advance_path {
                party.ai.path.advance();
            }
            party.pos = pos_of_grid_coordinate(&sim.sites, party.position);
        }
    }

    {
        let mut spawns = Spawns::default();
        let cmds = request.commands.create_entity_cmds.drain(..);
        process_entity_create_commands(sim, cmds, &mut spawns);
        spawn_parties(sim, spawns.parties.drain(..));
    }

    let mut view = SimView::default();
    view.map_items = view::map_view_items(sim, request.map_viewport);
    view.map_lines = view::map_view_lines(sim, request.map_viewport);
    view.objects = request
        .objects_to_extract
        .iter()
        .map(|&id| view::extract_object(sim, id))
        .collect();
    view
}

enum ChangePath {
    Clear,
    Keep,
    Set(Vec<GridCoord>),
}

#[derive(Default)]
struct Navigate {
    id: PartyId,
    target: Option<SiteId>,
    destination: Option<GridCoord>,
}

fn tick_party_ai(sim: &Simulation) -> Vec<Navigate> {
    sim.parties
        .iter()
        .map(|(party_id, party_data)| {
            let mut target;
            let destination;

            if party_data.movement_speed == 0.0 {
                target = None;
                destination = None;
            } else {
                target = party_data.ai.target;

                if let Some(tgt) = target
                    && party_data.position == GridCoord::at(tgt)
                {
                    target = None;
                }

                if target.is_none() {
                    let rng = &mut key_rng(sim.date, party_id);
                    target = sim.sites.iter().choose(rng).map(|x| x.0);
                }

                destination = target.map(|tgt| GridCoord::at(tgt));
            };

            Navigate {
                id: party_id,
                target,
                destination,
            }
        })
        .collect()
}

fn pathfind(parties: &Parties, sites: &Sites) -> Vec<(PartyId, ChangePath)> {
    parties
        .iter()
        .map(|(party_id, party_data)| {
            let destination = party_data.ai.destination.unwrap_or(party_data.position);
            let update = if party_data.position == destination {
                ChangePath::Clear
            } else if Some(destination) == party_data.ai.path.endpoint() {
                ChangePath::Keep
            } else {
                let start_node = party_data.position.closest_endpoint();
                let end_node = destination.closest_endpoint();
                let end_v2 = sites.get(end_node).unwrap().pos;

                fn metric(x: f32) -> i64 {
                    (x * 1000.).round() as i64
                }

                let steps = pathfinding::directed::astar::astar(
                    &start_node,
                    |&site| sites.neighbours(site).iter().map(|&(s, d)| (s, metric(d))),
                    |&site| {
                        let site_v2 = sites.get(site).unwrap().pos;
                        metric(end_v2.distance(site_v2))
                    },
                    |&site| site == end_node,
                )
                .map(|x| x.0)
                .unwrap_or_default();

                // Adjust path
                let mut path = Vec::with_capacity(steps.len());
                path.extend(steps.into_iter().skip(1).map(|site| GridCoord::At(site)));
                path.push(destination);
                ChangePath::Set(path)
            };
            (party_id, update)
        })
        .collect()
}

struct Movement {
    party_id: PartyId,
    next_position: GridCoord,
    advance_path: bool,
}

fn move_to_next_coord(parties: &Parties, sites: &Sites) -> Vec<Movement> {
    parties
        .iter()
        .map(|(party_id, party_data)| {
            let (next_position, advance_path) = party_data
                .ai
                .path
                .beginning()
                .map(|destination| {
                    // While we have many possible combinations, there can be at most
                    // 2 relevant nodes.
                    let (a1, b1, t1) = party_data.position.as_triple();
                    let (a2, b2, t2) = destination.as_triple();
                    // Get the actual start and end point
                    let start = a1.min(a2);
                    let end = b1.max(b2);
                    // Adjust the current and end t
                    let current_t = if a1 == end { 1.0 } else { t1 };
                    let end_t = if a2 == end { 1.0 } else { t2 };
                    // Get the actual distance between the two
                    let t_direction = (end_t - current_t).signum();
                    let distance = sites.distance(start, end);
                    if distance == f32::INFINITY {
                        println!("WARNING: Movement to infinitely far location!");
                    }
                    // We are moving with a certain speed
                    const BASE_SPEED: f32 = 0.01;
                    let speed = party_data.movement_speed * BASE_SPEED;
                    let t_speed = if speed / sites.distance(start, end) == 0.0 {
                        0.0
                    } else {
                        speed / distance
                    };
                    // Let's now adjust the t
                    let delta_t = t_speed * t_direction;
                    let next_t = (current_t + delta_t).clamp(0., 1.);
                    let next_pos = GridCoord::with_triple(start, end, next_t);
                    let advance_path = next_pos == destination;
                    (next_pos, advance_path)
                })
                .unwrap_or((party_data.position, false));
            Movement {
                party_id,
                next_position,
                advance_path,
            }
        })
        .collect()
}

fn pos_of_grid_coordinate(sites: &Sites, coord: GridCoord) -> V2 {
    match coord {
        GridCoord::At(site) => sites.get(site).map(|x| x.pos).unwrap_or_default(),
        GridCoord::Between(site1, site2, t) => {
            let p1 = sites.get(site1).map(|x| x.pos).unwrap_or_default();
            let p2 = sites.get(site2).map(|x| x.pos).unwrap_or_default();
            V2 {
                x: lerp(p1.x, p2.x, t),
                y: lerp(p1.y, p2.y, t),
            }
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[derive(Default)]
struct CreateEntity {
    tag: String,
    name: String,
    flags: AgentFlags,
    site: Option<String>,
    faction: Option<String>,
    settlement_kind: Option<String>,
    mobile_party: bool,
    is_test: bool,
}

#[derive(Default)]
pub struct TickCommands {
    create_entity_cmds: Vec<CreateEntity>,
}

pub struct CreateLocationParams {
    pub name: String,
    pub site: String,
    pub faction: String,
    pub settlement_kind: String,
}

pub struct CreatePersonParams {
    pub name: String,
    pub site: String,
    pub faction: String,
}

pub struct CreateFactionParams {
    pub tag: String,
    pub name: String,
}

pub struct CreateTestPartyParams {
    pub site: String,
    pub faction: String,
}

impl TickCommands {
    pub fn create_location(&mut self, params: CreateLocationParams) {
        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            site: Some(params.site),
            faction: Some(params.faction),
            settlement_kind: Some(params.settlement_kind),
            ..Default::default()
        });
    }

    pub fn create_person(&mut self, params: CreatePersonParams) {
        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            site: Some(params.site),
            faction: Some(params.faction),
            mobile_party: true,
            ..Default::default()
        });
    }

    pub fn create_faction(&mut self, params: CreateFactionParams) {
        self.create_entity_cmds.push(CreateEntity {
            tag: params.tag,
            name: params.name,
            flags: AgentFlags::new(&[AgentFlag::IsFaction]),
            ..Default::default()
        });
    }

    pub fn create_test_party(&mut self, params: CreateTestPartyParams) {
        self.create_entity_cmds.push(CreateEntity {
            name: "Test".to_string(),
            site: Some(params.site),
            faction: Some(params.faction),
            mobile_party: true,
            is_test: false,
            ..Default::default()
        });
    }
}

#[derive(Default)]
struct Spawns {
    parties: Vec<SpawnParty>,
}

fn process_entity_create_commands(
    sim: &mut Simulation,
    commands: impl Iterator<Item = CreateEntity>,
    spawns: &mut Spawns,
) {
    for spawn in commands {
        let mut site = None;
        let mut faction = None;

        if let Some(tag) = spawn.site.as_ref() {
            site = match sim.sites.lookup(tag) {
                Some((id, _)) => Some(id),
                None => {
                    println!("Undefined site '{tag}'");
                    continue;
                }
            }
        }

        if let Some(tag) = spawn.faction.as_ref() {
            faction = match sim.agents.tags.lookup(tag) {
                Some(id) => Some(id),
                None => {
                    println!("Undefined faction '{tag}'");
                    continue;
                }
            }
        }

        let agent = {
            let name = AgentName::fixed(spawn.name);
            sim.agents.insert(AgentData {
                name,
                flags: spawn.flags,
                ..Default::default()
            })
        };

        if !spawn.tag.is_empty() {
            sim.agents.tags.insert(spawn.tag, agent);
        }

        if let Some(faction) = faction {
            sim.agents.political_hierarchy.insert(faction, agent);
        }

        let mut spawn_party = None;

        if let Some(tag) = spawn.settlement_kind.as_ref() {
            let kind = match tag.as_str() {
                "village" => SettlementKind::Village,
                "town" => SettlementKind::Town,
                _ => {
                    println!("Undefined settlement kind '{tag}");
                    continue;
                }
            };
            let site = site.unwrap();
            let site_data = &sim.sites[site];
            if site_data.location.is_some() {
                println!(
                    "Site '{}' already bound to a party, cannot initialise a new location",
                    site_data.tag
                );
                continue;
            }

            let size = match kind {
                SettlementKind::Town => 3.,
                SettlementKind::Village => 2.,
            };

            let location = sim.locations.insert(LocationData {
                agent,
                site,
                buildings: Default::default(),
            });

            sim.sites.bind_location(site, location);

            spawn_party = Some(SpawnParty {
                agent,
                coords: GridCoord::At(site),
                movement_speed: 0.,
                size,
                layer: 0,
                test: false,
            })
        };

        if spawn.mobile_party {
            spawns.parties.push(SpawnParty {
                agent,
                coords: GridCoord::At(site.unwrap()),
                movement_speed: 2.5,
                size: 1.,
                layer: 1,
                test: spawn.is_test,
            });
        }

        spawns.parties.extend(spawn_party);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SettlementKind {
    Town,
    Village,
}

struct SpawnParty {
    agent: AgentId,
    coords: GridCoord,
    movement_speed: f32,
    size: f32,
    layer: u8,
    test: bool,
}

fn spawn_parties(sim: &mut Simulation, spawns: impl Iterator<Item = SpawnParty>) {
    for spawn in spawns {
        let mut ai = PartyAi::default();
        if spawn.test {
            let target = sim.sites.lookup("llan_heledd").unwrap().0;
            ai.target = Some(target);
        }

        sim.parties.insert(PartyData {
            agent: spawn.agent,
            position: spawn.coords,
            pos: pos_of_grid_coordinate(&sim.sites, spawn.coords),
            size: spawn.size,
            movement_speed: spawn.movement_speed,
            layer: spawn.layer,
            ai,
        });
    }
}
