use crate::object::*;
use crate::simulation::*;

#[derive(Default)]
pub struct SimView {
    pub map_lines: Vec<(V2, V2)>,
    pub map_items: Vec<MapItem>,
    pub objects: Vec<Option<Object>>,
}

pub struct MapItem {
    pub id: ObjectId,
    pub name: String,
    pub pos: V2,
    pub size: f32,
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
                name: String::default(),
                pos: site.pos,
                size: 1.,
            })
        });
    let parties = sim
        .parties
        .iter()
        .filter(|(_, party)| viewport.contains(party.pos))
        .map(|(id, party)| MapItem {
            id: ObjectId(ObjectHandle::Party(id)),
            name: party.name.clone(),
            pos: party.pos,
            size: party.size,
        });
    sites.chain(parties).collect()
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

            let kind = if party.contents.location.is_some() {
                "Location"
            } else {
                "Party"
            };
            obj.set("kind", kind);
        }

        ObjectHandle::Site(_) => {
            obj.set("kind", "Site");
        }
    }

    Some(obj)
}
