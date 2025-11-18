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

pub(crate) fn tick(sim: &mut Simulation, mut request: TickRequest) -> SimView {
    spawn_locations(sim, request.commands.spawn_locations.drain(..));
    spawn_people(sim, request.commands.spawn_people.drain(..));

    if request.commands.init {
        test_init(sim);
    }

    if request.advance_time {
        sim.date.advance();

        // Pathfinding
        for (id, update) in pathfind(&sim.parties, &sim.sites) {
            let party_data = &mut sim.parties[id];
            match update {
                ChangePath::Keep => {}
                ChangePath::Clear => party_data.path.clear(),
                ChangePath::Set(steps) => party_data.path = Path::new(steps),
            }
        }

        // Update coordinates and positions
        let movements = move_to_next_coord(&sim.parties, &sim.sites);
        for movement in movements {
            let party = &mut sim.parties[movement.party_id];
            party.position = movement.next_position;
            if movement.advance_path {
                party.path.advance();
            }
            party.pos = pos_of_grid_coordinate(&sim.sites, party.position);
        }
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

fn pathfind(parties: &Parties, sites: &Sites) -> Vec<(PartyId, ChangePath)> {
    parties
        .iter()
        .map(|(party_id, party_data)| {
            let update = if party_data.position == party_data.destination {
                ChangePath::Clear
            } else if Some(party_data.destination) == party_data.path.endpoint() {
                ChangePath::Keep
            } else {
                let start_node = party_data.position.closest_endpoint();
                let end_node = party_data.destination.closest_endpoint();
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
                path.push(party_data.destination);
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
                    // We are moving with a certain speed
                    const BASE_SPEED: f32 = 0.01;
                    let speed = party_data.movement_speed * BASE_SPEED;
                    let t_speed = if speed / sites.distance(start, end) == 0.0 {
                        0.0
                    } else {
                        speed / distance
                    };
                    // Let's now adjust the t
                    let next_t = (current_t + t_speed * t_direction).clamp(0., 1.);
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
pub struct TickCommands {
    pub spawn_locations: Vec<SpawnLocation>,
    pub spawn_people: Vec<SpawnPerson>,
    pub init: bool,
}

fn test_init(sim: &mut Simulation) {
    let entity = sim.entities.insert(EntityData {
        name: "Test".to_string(),
        ..Default::default()
    });

    let site_a = sim.sites.lookup("din_drust").unwrap().0;
    let site_b = sim.sites.lookup("llan_heledd").unwrap().0;

    let position = GridCoord::at(site_a);
    let destination = GridCoord::at(site_b);
    let pos = pos_of_grid_coordinate(&sim.sites, position);
    let party = sim.parties.insert(PartyData {
        entity,
        destination,
        position,
        path: Path::default(),
        pos,
        size: 1.,
        movement_speed: 2.5,
        contents: PartyContents::default(),
    });
    sim.entities[entity].party = Some(party);
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettlementKind {
    Town,
    Village,
}

pub struct SpawnLocation {
    pub name: String,
    pub site: String,
    pub kind: SettlementKind,
}

fn spawn_locations(sim: &mut Simulation, spawns: impl Iterator<Item = SpawnLocation>) {
    for spawn in spawns {
        let (site_id, site_data) = match sim.sites.lookup(&spawn.site) {
            Some(site) => site,
            None => {
                println!("Unknown site '{}'", spawn.site);
                continue;
            }
        };

        let entity = sim.entities.insert(EntityData {
            name: spawn.name.to_string(),
            ..Default::default()
        });

        let size = match spawn.kind {
            SettlementKind::Town => 3.,
            SettlementKind::Village => 2.,
        };

        let grid_coord = GridCoord::at(site_id);
        let party = sim.parties.insert(PartyData {
            entity,
            pos: site_data.pos,
            destination: grid_coord,
            position: grid_coord,
            path: Path::default(),
            size,
            movement_speed: 0.,
            contents: PartyContents::default(),
        });

        let location = sim.locations.insert(LocationData {
            entity,
            site: site_id,
            buildings: Default::default(),
        });

        let entity = &mut sim.entities[entity];
        entity.party = Some(party);
        entity.location = Some(location);
        sim.sites.bind_location(site_id, location);
    }
}

pub struct SpawnPerson {}

fn spawn_people(sim: &mut Simulation, spawns: impl Iterator<Item = SpawnPerson>) {}
