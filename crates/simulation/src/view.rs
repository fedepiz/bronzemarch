use crate::object::*;
use crate::simulation::*;

#[derive(Default)]
pub struct SimView {
    pub map_lines: Vec<(V2, V2)>,
    pub map_items: Vec<MapItem>,
    pub objects: Vec<Option<Object>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum MapItemKind {
    Site,
    Party,
}

pub struct MapItem {
    pub id: ObjectId,
    pub kind: MapItemKind,
    pub name: String,
    pub pos: V2,
    pub size: f32,
    pub rank: i64,
}

pub(crate) fn map_view_lines(sim: &Simulation, viewport: Extents) -> Vec<(V2, V2)> {
    let mut out = Vec::with_capacity(100);
    for (id, site) in sim.sites.iter() {
        let parent_out = !viewport.contains(site.pos);
        for neigh_id in sim.sites.greater_neighbours(id) {
            let destination = sim.sites.get(neigh_id).unwrap().pos;
            let child_out = !viewport.contains(destination);
            if !parent_out || !child_out {
                out.push((site.pos, destination));
            }
        }
    }
    out
}

pub(crate) fn map_view_items(sim: &Simulation, viewport: Extents) -> Vec<MapItem> {
    let sites = sim
        .sites
        .iter()
        .filter(|(_, site)| viewport.contains(site.pos))
        .filter_map(|(site_id, site)| {
            // Skip sites that have a location (and thus a party)
            if site.location.is_some() {
                return None;
            }
            Some(MapItem {
                id: ObjectId(ObjectHandle::Site(site_id)),
                kind: MapItemKind::Site,
                name: String::default(),
                pos: site.pos,
                size: 1.,
                rank: 0,
            })
        });

    let parties = sim
        .parties
        .iter()
        .filter(|(_, party)| viewport.contains(party.pos))
        .map(|(party_id, party)| {
            let rank = if party.contents.location.is_some() {
                1
            } else {
                2
            };

            MapItem {
                id: ObjectId(ObjectHandle::Party(party_id)),
                kind: MapItemKind::Party,
                name: party.name.clone(),
                pos: party.pos,
                size: party.size,
                rank,
            }
        });

    let mut items: Vec<_> = sites.chain(parties).collect();
    items.sort_by_key(|item| item.rank);
    items
}

pub(super) fn extract_object(sim: &mut Simulation, id: ObjectId) -> Option<Object> {
    let mut obj = Object::new();
    obj.set("id", id);

    match id.0 {
        ObjectHandle::Null => {
            return None;
        }

        ObjectHandle::Global => {
            let date = sim.date;
            let date = format!(
                "{}/{}/{}",
                date.calendar_day(),
                date.calendar_month(),
                date.calendar_year()
            );
            obj.set("date", date);
        }

        ObjectHandle::Party(party_id) => {
            let party = &sim.parties[party_id];
            obj.set("name", &party.name);

            if let Some(leader) = party.contents.leader {
                obj.set("leader", &sim.people[leader].name);
            }
        }

        ObjectHandle::Site(_) => {
            obj.set("kind", "Site");
        }
    }

    Some(obj)
}
