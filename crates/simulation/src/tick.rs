use std::collections::BTreeSet;

use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::IteratorRandom;

use crate::date::Date;
use crate::object::*;
use crate::simulation::*;
use crate::view;
use crate::view::*;

#[derive(Default)]
pub struct TickRequest<'a> {
    pub commands: TickCommands<'a>,
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
        let cmds = request.commands.create_entity_cmds.drain(..);
        process_entity_create_commands(sim, cmds);
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
struct CreateEntity<'a> {
    agent: Option<CreateAgent<'a>>,
    location: Option<CreateLocation<'a>>,
    party: Option<CreateParty<'a>>,
}

struct CreateAgent<'a> {
    tag: &'a str,
    name: &'a str,
    flags: &'a [AgentFlag],
    political_parent: Option<&'a str>,
}

struct CreateLocation<'a> {
    site: &'a str,
}

enum CreatePartyName<'a> {
    FromAgent,
    Fixed(&'a str),
}

struct CreateParty<'a> {
    name: CreatePartyName<'a>,
    site: &'a str,
    size: f32,
    movement_speed: f32,
    layer: u8,
}

#[derive(Default)]
pub struct TickCommands<'a> {
    create_entity_cmds: Vec<CreateEntity<'a>>,
}

pub struct CreateLocationParams<'a> {
    pub name: &'a str,
    pub site: &'a str,
    pub faction: &'a str,
    pub settlement_kind: &'a str,
}

pub struct CreatePersonParams<'a> {
    pub name: &'a str,
    pub site: &'a str,
    pub faction: &'a str,
}

pub struct CreateFactionParams<'a> {
    pub tag: &'a str,
    pub name: &'a str,
}

pub struct CreateTestPartyParams<'a> {
    pub site: &'a str,
    pub faction: &'a str,
}

impl<'a> TickCommands<'a> {
    pub fn create_location(&mut self, params: CreateLocationParams<'a>) {
        let size = match params.settlement_kind {
            "town" => 2.5,
            "village" => 2.,
            _ => 1.,
        };
        self.create_entity_cmds.push(CreateEntity {
            agent: Some(CreateAgent {
                tag: "",
                name: params.name,
                flags: &[],
                political_parent: Some(params.faction),
            }),
            location: Some(CreateLocation { site: params.site }),
            party: Some(CreateParty {
                name: CreatePartyName::FromAgent,
                site: params.site,
                size,
                movement_speed: 0.,
                layer: 0,
            }),
        });
    }

    pub fn create_person(&mut self, params: CreatePersonParams<'a>) {
        self.create_entity_cmds.push(CreateEntity {
            agent: Some(CreateAgent {
                tag: "",
                name: params.name,
                flags: &[],
                political_parent: Some(params.faction),
            }),
            party: Some(CreateParty {
                name: CreatePartyName::FromAgent,
                site: params.site,
                size: 1.,
                movement_speed: 2.5,
                layer: 1,
            }),
            ..Default::default()
        });
    }

    pub fn create_faction(&mut self, params: CreateFactionParams<'a>) {
        self.create_entity_cmds.push(CreateEntity {
            agent: Some(CreateAgent {
                tag: params.tag,
                name: params.name,
                flags: &[AgentFlag::IsFaction],
                political_parent: None,
            }),
            ..Default::default()
        });
    }

    pub fn create_test_party(&mut self, params: CreateTestPartyParams<'a>) {
        self.create_entity_cmds.push(CreateEntity {
            party: Some(CreateParty {
                name: CreatePartyName::Fixed("Test"),
                site: params.site,
                size: 1.,
                movement_speed: 2.5,
                layer: 1,
            }),
            ..Default::default()
        });
    }
}

fn process_entity_create_commands<'a>(
    sim: &mut Simulation,
    commands: impl Iterator<Item = CreateEntity<'a>>,
) {
    for command in commands {
        let agent = command.agent.map(|args| {
            let name = AgentName::fixed(args.name);
            let id = sim.agents.insert(AgentData {
                name,
                flags: AgentFlags::new(args.flags),
            });

            if !args.tag.is_empty() {
                sim.agents.tags.insert(args.tag, id);
            }

            if let Some(parent) = args.political_parent {
                match sim.agents.tags.lookup(parent) {
                    Some(parent) => sim.agents.political_hierarchy.insert(parent, id),
                    None => println!("Unknown agent with tag '{parent}'"),
                }
            }
            id
        });

        command.location.and_then(|args| {
            let site = match sim.sites.lookup(args.site) {
                Some((id, _)) => id,
                None => {
                    println!("Undefined site '{}'", args.site);
                    return None;
                }
            };
            let location = sim.locations.insert(LocationData {
                site,
                buildings: BTreeSet::default(),
            });
            sim.sites.bind_location(site, location);
            Some(())
        });

        command.party.and_then(|args| {
            let name = match args.name {
                CreatePartyName::FromAgent => sim.agents[agent.unwrap()].name.as_str(),
                CreatePartyName::Fixed(str) => str,
            }
            .to_string();
            let (position, pos) = match sim.sites.lookup(args.site) {
                Some((id, data)) => (GridCoord::At(id), data.pos),
                None => {
                    println!("Undefined site '{}'", args.site);
                    return None;
                }
            };
            sim.parties.insert(PartyData {
                name,
                position,
                pos,
                size: args.size,
                layer: args.layer,
                movement_speed: args.movement_speed,
                ai: PartyAi::default(),
            });
            Some(())
        });
    }
}
