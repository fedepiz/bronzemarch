use std::collections::BTreeSet;

use rand::SeedableRng;
use rand::rngs::SmallRng;

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

        if let Some((subject, target)) = request.commands.move_to {
            apply_move_to(sim, subject, target);
        }

        let result = tick_party_ai(sim);
        for update in result {
            let movement = &mut sim.parties[update.id].movement;
            movement.target = update.target;
            movement.destination = update.destination;
        }

        // Pathfinding
        for (id, update) in pathfind(&sim.parties, &sim.sites) {
            let party = &mut sim.parties[id];
            match update {
                ChangePath::Keep => {}
                ChangePath::Clear => party.movement.path.clear(),
                ChangePath::Set(steps) => {
                    party.movement.path = Path::new(steps);
                }
            }
        }

        // Advance pathing
        for party in sim.parties.values_mut() {
            let path = &mut party.movement.path;
            while path.beginning() == Some(party.position) {
                path.advance();
            }
        }

        // Update coordinates and positions
        let movements = move_to_next_coord(&sim.parties, &sim.sites);
        for movement in movements {
            let party = &mut sim.parties[movement.party_id];
            party.position = movement.next_position;
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

fn apply_move_to(sim: &mut Simulation, subject: ObjectId, target: ObjectId) {
    let subject = match subject.0 {
        ObjectHandle::Entity(id) => match sim.entities[id].party {
            Some(x) => x,
            None => return,
        },
        _ => return,
    };
    let target = match target.0 {
        ObjectHandle::Site(site) => Some(MovementTarget::Site(site)),
        ObjectHandle::Entity(entity) => sim
            .entities
            .get(entity)
            .and_then(|e| e.party)
            .map(MovementTarget::Party),
        _ => None,
    };
    sim.parties[subject].movement.target = target;
}

enum ChangePath {
    Clear,
    Keep,
    Set(Vec<GridCoord>),
}

#[derive(Default)]
struct Navigate {
    id: PartyId,
    target: Option<MovementTarget>,
    destination: Option<GridCoord>,
}

fn tick_party_ai(sim: &Simulation) -> Vec<Navigate> {
    sim.parties
        .iter()
        .map(|(party_id, party_data)| {
            let target;
            let destination;

            if party_data.movement_speed == 0.0 {
                target = None;
                destination = None;
            } else {
                target = party_data.movement.target;
                destination = target.map(|tgt| match tgt {
                    MovementTarget::Site(site) => GridCoord::at(site),
                    MovementTarget::Party(party) => sim.parties[party].position,
                });
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
            let destination = party_data
                .movement
                .destination
                .unwrap_or(party_data.position);
            let update = if party_data.position == destination {
                ChangePath::Clear
            } else if Some(destination) == party_data.movement.path.endpoint() {
                ChangePath::Keep
            } else {
                let current_pos = party_data.position;
                let path = if current_pos.is_colinear(destination) {
                    vec![destination]
                } else {
                    let start_node = current_pos.closest_endpoint();
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

                    // Construct path
                    let mut path = Vec::with_capacity(steps.len() + 1);

                    let touches = |idx: usize| {
                        steps
                            .get(idx)
                            .map(|&s| current_pos.touches(s))
                            .unwrap_or(false)
                    };

                    let skip = if touches(0) && touches(1) { 1 } else { 0 };
                    path.extend(steps.into_iter().skip(skip).map(|site| GridCoord::at(site)));

                    path.push(destination);
                    path
                };
                ChangePath::Set(path)
            };
            (party_id, update)
        })
        .collect()
}

struct Movement {
    party_id: PartyId,
    next_position: GridCoord,
}

fn move_to_next_coord(parties: &Parties, sites: &Sites) -> Vec<Movement> {
    parties
        .iter()
        .map(|(party_id, party_data)| {
            let next_position = party_data
                .movement
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
                    next_pos
                })
                .unwrap_or(party_data.position);
            Movement {
                party_id,
                next_position,
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
    name: &'a str,
    kind_name: &'static str,
    agent: Option<CreateAgent<'a>>,
    location: Option<CreateLocation<'a>>,
    party: Option<CreateParty<'a>>,
}

struct CreateAgent<'a> {
    tag: &'a str,
    flags: &'a [AgentFlag],
    political_parent: Option<&'a str>,
}

struct CreateLocation<'a> {
    site: &'a str,
}

struct CreateParty<'a> {
    site: &'a str,
    size: f32,
    movement_speed: f32,
    layer: u8,
}

#[derive(Default)]
pub struct TickCommands<'a> {
    create_entity_cmds: Vec<CreateEntity<'a>>,
    move_to: Option<(ObjectId, ObjectId)>,
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

impl<'a> TickCommands<'a> {
    pub fn issue_move_to_object(&mut self, subject: ObjectId, target: ObjectId) {
        self.move_to = Some((subject, target));
    }

    pub fn create_location(&mut self, params: CreateLocationParams<'a>) {
        let size = match params.settlement_kind {
            "town" => 2.5,
            "village" => 2.,
            _ => 1.,
        };
        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            kind_name: "Location",
            agent: Some(CreateAgent {
                tag: "",
                flags: &[],
                political_parent: Some(params.faction),
            }),
            location: Some(CreateLocation { site: params.site }),
            party: Some(CreateParty {
                site: params.site,
                size,
                movement_speed: 0.,
                layer: 0,
            }),
        });
    }

    pub fn create_person(&mut self, params: CreatePersonParams<'a>) {
        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            kind_name: "Person",
            agent: Some(CreateAgent {
                tag: "",
                flags: &[],
                political_parent: Some(params.faction),
            }),
            party: Some(CreateParty {
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
            name: params.name,
            kind_name: "Faction",
            agent: Some(CreateAgent {
                tag: params.tag,
                flags: &[AgentFlag::IsFaction],
                political_parent: None,
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
        let entity = sim.entities.insert(EntityData {
            name: command.name.to_string(),
            kind_name: command.kind_name,
            ..Default::default()
        });

        let agent = command.agent.map(|args| {
            let id = sim.agents.insert(AgentData {
                entity,
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

        let location = command.location.and_then(|args| {
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
            Some(location)
        });

        let party = command.party.and_then(|args| {
            let (position, pos) = match sim.sites.lookup(args.site) {
                Some((id, data)) => (GridCoord::at(id), data.pos),
                None => {
                    println!("Undefined site '{}'", args.site);
                    return None;
                }
            };
            let id = sim.parties.insert(PartyData {
                entity,
                position,
                pos,
                size: args.size,
                layer: args.layer,
                movement_speed: args.movement_speed,
                movement: PartyMovement::default(),
            });
            Some(id)
        });

        let entity = &mut sim.entities[entity];
        entity.agent = agent;
        entity.party = party;
        entity.location = location;
    }
}
