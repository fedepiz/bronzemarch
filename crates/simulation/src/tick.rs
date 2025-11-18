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

    if request.commands.init {
        let entity = sim.entities.insert(EntityData {
            name: "Test".to_string(),
            ..Default::default()
        });

        let site_a = sim.sites.lookup("caer_ligualid").unwrap().0;
        let site_b = sim.sites.lookup("anava").unwrap().0;

        let grid_coord = GridCoord::Between(site_a, site_b, 0.5);
        let pos = pos_of_grid_coordinate(&sim.sites, grid_coord);
        let party = sim.parties.insert(PartyData {
            entity,
            grid_coord,
            pos,
            size: 1.,
        });
        sim.entities[entity].party = Some(party);
    }

    if request.advance_time {
        sim.date.advance();

        // Update positions
        let positions = calculate_new_positions(&sim.parties, &sim.sites);
        for (id, pos) in positions {
            sim.parties[id].pos = pos;
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

fn calculate_new_positions(parties: &Parties, sites: &Sites) -> Vec<(PartyId, V2)> {
    parties
        .iter()
        .map(|(party_id, party_data)| {
            let pos = pos_of_grid_coordinate(sites, party_data.grid_coord);
            (party_id, pos)
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
    pub init: bool,
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

        let party = sim.parties.insert(PartyData {
            entity,
            pos: site_data.pos,
            grid_coord: GridCoord::At(site_id),
            size,
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
