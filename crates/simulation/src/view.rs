use crate::object::*;
use crate::simulation::*;
use crate::tokens::*;

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
    pub image: &'static str,
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
                image: "",
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
                image: party.image,
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
            let entity = sim.entities.get(entity_id)?;

            obj.set("name", &entity.name);
            obj.set("kind", entity.kind_name);

            if let Some(agent_id) = entity.agent {
                let agent_data = &sim.agents[agent_id];
                obj.set("cash", format!("{:1.0}$", agent_data.cash));

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

            if let Some(party) = entity.party {
                let party = &sim.parties[party];
                obj.set(
                    "good_stock",
                    sim.good_types
                        .iter()
                        .filter_map(|(good_id, good_data)| {
                            let amount = party.good_stock[good_id];
                            if amount == 0.0 {
                                return None;
                            }
                            let mut obj = Object::new();
                            obj.set("name", good_data.name);
                            obj.set("amount", format!("{amount:1.0}"));
                            Some(obj)
                        })
                        .collect::<Vec<_>>(),
                );
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
                    .all_tokens_of_category(location.tokens, TokenCategory::Pop)
                    .map(|tok| {
                        let mut obj = Object::new();
                        obj.set("name", tok.typ.name);
                        obj.set("size", format!("{}", tok.data.size));
                        obj
                    })
                    .collect();
                entry.set("pops", pops);

                let buildings: Vec<_> = sim
                    .tokens
                    .all_tokens_of_category(location.tokens, TokenCategory::Building)
                    .map(|tok| {
                        let mut obj = Object::new();
                        obj.set("name", tok.typ.name);
                        obj.set("size", format!("{}", tok.data.size));
                        obj
                    })
                    .collect();
                entry.set("buildings", buildings);

                let market_goods: Vec<_> = location
                    .market
                    .goods
                    .iter()
                    .map(|(id, good)| {
                        let mut entry = Object::new();
                        let typ = &sim.good_types[id];
                        entry.set("name", typ.name);
                        entry.set("stock", format!("{:1.1}", good.stock));
                        {
                            let mark = if good.stock_delta >= 0. { "+" } else { "" };
                            entry.set("stock_delta", format!("{mark}{:1.1}", good.stock_delta));
                        }

                        entry.set("supply_effective", format!("{:1.1}", good.supply_effective));
                        entry.set("supply_base", format!("{:1.1}", good.supply_base));
                        entry.set(
                            "supply_from_stock",
                            format!("{:1.1}", good.supply_from_stock),
                        );

                        entry.set("satisfaction", format!("{:1.1}%", good.satisfaction * 100.));

                        entry.set("demand_effective", format!("{:1.1}", good.demand_effective));
                        entry.set("demand_base", format!("{:1.1}", good.demand_base));

                        entry.set("price", format!("{:1.2}$", good.price));
                        entry.set("target_price", format!("{:1.2}$", good.target_price));
                        entry
                    })
                    .collect();

                entry.set("market_goods", market_goods);

                entry.set("influences", {
                    let influences = &sim.sites[location.site].influences;
                    influences
                        .iter()
                        .map(|(typ, amount)| {
                            let mut obj = Object::new();
                            obj.set(
                                "kind",
                                match typ.kind {
                                    crate::sites::InfluenceKind::Market => "Market",
                                },
                            );
                            {
                                let entity = sim.parties[typ.source].entity;
                                let name = &sim.entities[entity].name;
                                obj.set("source", name);
                            }
                            obj.set("amount", format!("{amount}"));
                            obj
                        })
                        .collect::<Vec<_>>()
                });

                obj.set("location", entry);
            }

            if let Some(agent) = entity.pressure_agent {
                let agent = &sim.pressurables[agent];
                let mut entry = Object::new();

                entry.set(
                    "current",
                    agent
                        .current
                        .iter()
                        .map(|(kind, amount)| {
                            let mut item = Object::new();

                            let name = match kind {
                                PressureType::Farmer => "Farmer",
                            };
                            item.set("name", name);
                            item.set("amount", format!("{amount:1.0}"));
                            item
                        })
                        .collect::<Vec<_>>(),
                );

                obj.set("pressure_agent", entry);
            }
        }

        ObjectHandle::Site(_) => {
            obj.set("kind", "Site");
        }
    }

    Some(obj)
}
