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
    pub layer: u8,
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
                layer: 0,
            })
        });

    let parties = sim
        .parties
        .values()
        .filter(|party| viewport.contains(party.pos))
        .map(|party| {
            let entity = &sim.entities[party.entity];
            MapItem {
                id: ObjectId(ObjectHandle::Entity(party.entity)),
                kind: MapItemKind::Party,
                name: entity.name.clone(),
                pos: party.pos,
                size: party.size,
                layer: party.layer,
            }
        });

    let mut items: Vec<_> = sites.chain(parties).collect();
    items.sort_by_key(|item| item.layer);
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

        ObjectHandle::Entity(entity_id) => {
            let entity = &sim.entities[entity_id];

            obj.set("name", &entity.name);

            if let Some(agent_id) = entity.agent {
                struct Field {
                    tag: &'static str,
                    query: RelatedAgent,
                }

                let fields = [
                    Field {
                        tag: "faction",
                        query: RelatedAgent::Faction,
                    },
                    Field {
                        tag: "country",
                        query: RelatedAgent::Country,
                    },
                ];

                for field in fields {
                    if let Some((_, found)) =
                        query_related_agent(&sim.agents, agent_id, field.query)
                    {
                        let name = sim.entities[found.entity].name.as_str();
                        obj.set(field.tag, name);
                    }
                }
            }
        }

        ObjectHandle::Site(_) => {
            obj.set("kind", "Site");
        }
    }

    Some(obj)
}
