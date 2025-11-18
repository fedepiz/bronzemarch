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
        if !viewport.contains(site.pos) {
            continue;
        }
        for neigh_id in sim.sites.greater_neighbours(id) {
            let destination = sim.sites.get(neigh_id).unwrap().pos;
            if !viewport.contains(destination) {
                continue;
            }
            out.push((site.pos, destination));
        }
    }
    out
}

pub(crate) fn map_view_items(sim: &Simulation, viewport: Extents) -> Vec<MapItem> {
    sim.parties
        .values()
        .filter(|party| viewport.contains(party.pos))
        .map(|party| {
            let entity = &sim.entities[party.entity];
            let id = ObjectId(ObjectHandle::Entity(party.entity));
            MapItem {
                id,
                name: entity.name.clone(),
                pos: party.pos,
                size: party.size,
            }
        })
        .collect()
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
        ObjectHandle::Entity(entity) => {
            let entity = &sim.entities[entity];
            obj.set("name", &entity.name);
        }
    }

    Some(obj)
}
