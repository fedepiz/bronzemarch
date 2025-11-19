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

    {
        let mut spawns = Spawns::default();

        let cmds = request.commands.create_entity_cmds.drain(..);
        process_entity_create_commands(sim, cmds, &mut spawns);

        let mut party_leader_changes = vec![];
        let mut party_name_refreshes = vec![];

        spawn_factions(sim, spawns.factions.drain(..));

        spawn_locations(sim, spawns.locations.drain(..));

        let result = spawn_people(sim, spawns.people.drain(..));
        spawns.mobile_parties.extend(result.spawn_mobile_parties);

        let result = spawn_mobile_parties(sim, spawns.mobile_parties.drain(..));
        party_leader_changes.extend(result.party_leader_changes);

        let result = change_party_leaders(sim, party_leader_changes.into_iter());
        party_name_refreshes.extend(result.refresh_party_name);

        self::refresh_party_names(sim, party_name_refreshes);
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
    kind: CreateEntityKind,
    tag: String,
    name: String,
    site: Option<String>,
    faction: Option<String>,
    settlement_kind: Option<String>,
}

enum CreateEntityKind {
    Nothing,
    Location,
    Person,
    Faction,
    TestParty,
}

impl Default for CreateEntityKind {
    fn default() -> Self {
        Self::Nothing
    }
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
            kind: CreateEntityKind::Location,
            name: params.name,
            site: Some(params.site),
            faction: Some(params.faction),
            settlement_kind: Some(params.settlement_kind),
            ..Default::default()
        });
    }

    pub fn create_person(&mut self, params: CreatePersonParams) {
        self.create_entity_cmds.push(CreateEntity {
            kind: CreateEntityKind::Person,
            name: params.name,
            site: Some(params.site),
            faction: Some(params.faction),
            ..Default::default()
        });
    }

    pub fn create_faction(&mut self, params: CreateFactionParams) {
        self.create_entity_cmds.push(CreateEntity {
            kind: CreateEntityKind::Faction,
            tag: params.tag,
            name: params.name,
            ..Default::default()
        });
    }

    pub fn create_test_party(&mut self, params: CreateTestPartyParams) {
        self.create_entity_cmds.push(CreateEntity {
            kind: CreateEntityKind::TestParty,
            site: Some(params.site),
            faction: Some(params.faction),
            ..Default::default()
        });
    }
}

#[derive(Default)]
struct Spawns {
    factions: Vec<SpawnFaction>,
    locations: Vec<SpawnLocation>,
    people: Vec<SpawnPerson>,
    mobile_parties: Vec<SpawnMobileParty>,
}

fn process_entity_create_commands(
    sim: &Simulation,
    commands: impl Iterator<Item = CreateEntity>,
    spawns: &mut Spawns,
) {
    for spawn in commands {
        let mut site = None;
        let mut faction = None;
        let mut settlement_kind = None;

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
            faction = match sim.factions.lookup(tag) {
                Some(id) => Some(id),
                None => {
                    println!("Undefined faction '{tag}'");
                    continue;
                }
            }
        }

        if let Some(tag) = spawn.settlement_kind.as_ref() {
            settlement_kind = match tag.as_str() {
                "village" => Some(SettlementKind::Village),
                "town" => Some(SettlementKind::Town),
                _ => {
                    println!("Undefined settlement kind '{tag}");
                    continue;
                }
            }
        };

        match spawn.kind {
            CreateEntityKind::Nothing => {
                println!("WARNING: Spawning nothing");
            }
            CreateEntityKind::Location => {
                spawns.locations.push(SpawnLocation {
                    name: spawn.name,
                    site: site.unwrap(),
                    faction: faction.unwrap(),
                    kind: settlement_kind.unwrap(),
                });
            }
            CreateEntityKind::Person => {
                spawns.people.push(SpawnPerson {
                    name: spawn.name,
                    site: site.unwrap(),
                    faction: faction.unwrap(),
                });
            }
            CreateEntityKind::Faction => {
                spawns.factions.push(SpawnFaction {
                    tag: spawn.tag,
                    name: spawn.name,
                });
            }

            CreateEntityKind::TestParty => spawns.mobile_parties.push(SpawnMobileParty {
                name: "Test".to_string(),
                coords: GridCoord::At(site.unwrap()),
                leader: None,
                faction: faction.unwrap(),
                test: true,
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SettlementKind {
    Town,
    Village,
}

struct SpawnFaction {
    pub tag: String,
    pub name: String,
}

fn spawn_factions(sim: &mut Simulation, spawns: impl Iterator<Item = SpawnFaction>) {
    for spawn in spawns {
        sim.factions.insert(FactionData {
            tag: spawn.tag,
            name: spawn.name,
        });
    }
}

struct SpawnLocation {
    pub name: String,
    pub site: SiteId,
    pub faction: FactionId,
    pub kind: SettlementKind,
}

fn spawn_locations(sim: &mut Simulation, spawns: impl Iterator<Item = SpawnLocation>) {
    for spawn in spawns {
        let site_data = &sim.sites[spawn.site];

        if site_data.location.is_some() {
            println!(
                "Site '{}' already bound to a party, cannot initialise a new location",
                site_data.tag
            );
            continue;
        }

        let size = match spawn.kind {
            SettlementKind::Town => 3.,
            SettlementKind::Village => 2.,
        };

        let position = GridCoord::at(spawn.site);
        let party = sim.parties.insert(PartyData {
            name: spawn.name.clone(),
            pos: site_data.pos,
            position,
            destination: position,
            path: Path::default(),
            size,
            movement_speed: 0.,
            faction: spawn.faction,
            contents: PartyContents::default(),
        });

        let location = sim.locations.insert(LocationData {
            name: spawn.name,
            site: spawn.site,
            party,
            faction: spawn.faction,
            buildings: Default::default(),
        });

        sim.parties[party].contents.location = Some(location);

        sim.sites.bind_location(spawn.site, location);
    }
}

struct SpawnPerson {
    pub name: String,
    pub site: SiteId,
    pub faction: FactionId,
}

#[derive(Default)]
struct SpawnPeopleResult {
    spawn_mobile_parties: Vec<SpawnMobileParty>,
}

fn spawn_people(
    sim: &mut Simulation,
    spawns: impl Iterator<Item = SpawnPerson>,
) -> SpawnPeopleResult {
    let mut out = SpawnPeopleResult::default();
    for spawn in spawns {
        let person = sim.people.insert(PersonData {
            name: spawn.name,
            party: None,
            faction: spawn.faction,
        });

        out.spawn_mobile_parties.push(SpawnMobileParty {
            name: String::default(),
            coords: GridCoord::At(spawn.site),
            leader: Some(person),
            faction: spawn.faction,
            test: false,
        });
    }
    out
}

struct SpawnMobileParty {
    name: String,
    coords: GridCoord,
    leader: Option<PersonId>,
    faction: FactionId,
    test: bool,
}

#[derive(Default)]
struct SpawnMobilePartiesResult {
    party_leader_changes: Vec<ChangePartyLeader>,
}

fn spawn_mobile_parties(
    sim: &mut Simulation,
    spawns: impl Iterator<Item = SpawnMobileParty>,
) -> SpawnMobilePartiesResult {
    let mut out = SpawnMobilePartiesResult::default();
    for spawn in spawns {
        let position = spawn.coords;
        let destination = if !spawn.test {
            position
        } else {
            GridCoord::at(sim.sites.lookup("llan_heledd").unwrap().0)
        };
        let party = sim.parties.insert(PartyData {
            name: spawn.name,
            position,
            destination,
            path: Path::default(),
            pos: pos_of_grid_coordinate(&sim.sites, spawn.coords),
            size: 1.,
            movement_speed: 2.5,
            faction: spawn.faction,
            contents: PartyContents::default(),
        });

        if let Some(person) = spawn.leader {
            out.party_leader_changes
                .push(ChangePartyLeader { party, person });
        }
    }
    out
}

#[derive(Clone, Copy)]
struct ChangePartyLeader {
    party: PartyId,
    person: PersonId,
}

#[derive(Default)]
struct ChangePartyLeadersResult {
    refresh_party_name: Vec<PartyId>,
}

fn change_party_leaders(
    sim: &mut Simulation,
    changes: impl Iterator<Item = ChangePartyLeader>,
) -> ChangePartyLeadersResult {
    let mut out = ChangePartyLeadersResult::default();
    for evt in changes {
        let person = &mut sim.people[evt.person];
        let party = &mut sim.parties[evt.party];
        assert!(party.contents.leader.is_none());
        assert!(!party.contents.people.contains(&evt.person));
        assert!(person.party.is_none() || person.party == Some(evt.party));
        party.contents.leader = Some(evt.person);
        party.contents.people.insert(evt.person);
        person.party = Some(evt.party);

        out.refresh_party_name.push(evt.party);
    }
    out
}

fn refresh_party_names(sim: &mut Simulation, to_refresh: impl IntoIterator<Item = PartyId>) {
    for party_id in to_refresh {
        let party = &mut sim.parties[party_id];
        let mut name = String::default();
        if let Some(leader) = party.contents.leader {
            name = sim.people[leader].name.clone();
        }
        party.name = name;
    }
}
