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
            obj.set("kind", entity.kind_name);

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

            if let Some(location) = entity.location {
                let location = &sim.locations[location];
                let mut entry = Object::new();
                entry.set("population", location.population.to_string());
                entry.set(
                    "prosperity",
                    format!("{:1.2}%", (location.prosperity * 100.0)),
                );

                entry.set(
                    "food",
                    format!(
                        "{:1.1}/{:1.1}",
                        location.market.food_consumed, location.market.food_stockpile
                    ),
                );
                entry.set("income", format!("{:1.0}$", location.market.income));

                let pops: Vec<_> = sim
                    .tokens
                    .all_tokens_of_category(entity.tokens.unwrap(), TokenCategory::Pop)
                    .map(|tok| {
                        let mut obj = Object::new();
                        obj.set("name", tok.typ.name);
                        obj.set("size", format!("{}", tok.data.size));
                        obj
                    })
                    .collect();
                entry.set("pops", pops);

                let market_goods: Vec<_> = location
                    .market
                    .goods
                    .iter()
                    .map(|(id, good)| {
                        let mut entry = Object::new();
                        let typ = &sim.good_types[id];
                        entry.set("name", typ.name);
                        entry.set("stock", format!("{:1.1}", good.stock));
                        entry.set("supply", format!("{:1.1}", good.supply));
                        entry.set("demand", format!("{:1.1}", good.demand));
                        entry.set("price", format!("{:1.2}$", good.price));
                        entry
                    })
                    .collect();

                entry.set("market_goods", market_goods);

                obj.set("location", entry);
            }
        }

        ObjectHandle::Site(_) => {
            obj.set("kind", "Site");
        }
    }

    Some(obj)
}
