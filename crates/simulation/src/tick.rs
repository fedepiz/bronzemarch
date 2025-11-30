use util::arena::Arena;

use crate::object::*;
use crate::simulation::*;
use crate::sites::*;
use crate::tokens::*;
use crate::view;
use crate::view::*;

#[derive(Default)]
pub struct TickRequest<'a> {
    pub commands: TickCommands<'a>,
    pub advance_time: bool,
    pub map_viewport: Extents,
    pub objects_to_extract: Vec<ObjectId>,
}

pub(crate) fn tick(sim: &mut Simulation, mut request: TickRequest, arena: &Arena) -> SimView {
    if request.advance_time {
        sim.date.advance();

        let tick_market = sim.date.is_new_day();

        tick_influences(arena, &mut sim.sites, &sim.locations);

        // Simulate economy at locations
        tick_location_economy(
            arena,
            &sim.entities,
            &mut sim.locations,
            &sim.tokens,
            &sim.good_types,
            &sim.sites,
            tick_market,
        );

        // Apply movement order
        if let Some((subject, target)) = request.commands.move_to {
            apply_move_order_to(sim, subject, target);
        }

        // Tick party AI (deciding where to go)
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

    // Create entities
    {
        let cmds = request.commands.create_entity_cmds.drain(..);
        process_entity_create_commands(sim, cmds);
    }

    // Extract view
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

fn apply_move_order_to(sim: &mut Simulation, subject: ObjectId, target: ObjectId) {
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

fn tick_influences(arena: &Arena, sites: &mut Sites, locations: &Locations) {
    let mut sources = sites.make_secondary_map();

    for (loc_id, location) in locations.iter() {
        let mut influences = arena.new_vec();

        for source in &location.influence_sources {
            let mut power = 0;

            // Add in population weight
            power += (source.population_modifier * location.population as f64).round() as i64;

            if power > 0 {
                influences.push((
                    InfluenceType {
                        kind: source.kind,
                        location: loc_id,
                    },
                    power as i32,
                ));
            }
        }
        let prev = sources.insert(location.site, influences.into_bump_slice());
        assert!(prev.is_none())
    }

    crate::sites::propagate_influences(arena, sites, &sources);
}

fn tick_location_economy(
    arena: &Arena,
    entities: &Entities,
    locations: &mut Locations,
    tokens: &Tokens,
    good_types: &GoodTypes,
    sites: &Sites,
    tick_market: bool,
) {
    // New location economic tick
    for location in locations.values_mut() {
        let tokens = {
            let entity = &entities[location.entity];
            arena.alloc_iter(
                entity
                    .tokens
                    .into_iter()
                    .flat_map(|id| tokens.all_tokens_in(id)),
            )
        };

        location.population = Tokens::count_size(tokens, TokenCategory::Pop);

        if !tick_market {
            continue;
        }

        const GOODS_POPULATION_SCALE: f64 = 0.01;

        let mut new_market = Market::new(good_types);

        // Calculate token contributions
        let mut rgo_work_points = 0.0;
        {
            let mut value_of_token_production = 0.0;
            let mut value_of_token_consumption = 0.0;

            for tok in tokens {
                let (scale, is_commerical) = match tok.typ.category {
                    TokenCategory::Building => (1., true),
                    TokenCategory::Pop => (GOODS_POPULATION_SCALE, false),
                };

                let size = tok.data.size as f64 * scale;

                for (good_id, &amt) in &tok.typ.demand {
                    let amount = amt * size;
                    let price = amount * location.market.goods[good_id].price;
                    let value = amount * price;
                    if is_commerical {
                        value_of_token_consumption += value;
                    }

                    new_market.goods[good_id].demand_base += amount;
                }

                for (good_id, &amt) in &tok.typ.supply {
                    let amount = amt * size;
                    let price = amount * location.market.goods[good_id].price;
                    let value = amount * price;

                    if is_commerical {
                        value_of_token_production += value;
                    }

                    new_market.goods[good_id].supply_base += amount;
                }
                rgo_work_points += tok.typ.rgo_points * size;
            }

            new_market.income += value_of_token_production;
            new_market.income -= value_of_token_consumption;
        }

        // Calculate RGO production
        {
            let rgo = &sites[location.site].rgo;
            let num_workers = rgo_work_points.floor().min(rgo.capacity as f64);

            let mut value_of_rgo_production = 0.0;

            for (good_id, rate) in rgo.rates.iter() {
                let produced = rate * num_workers;
                let price = location.market.goods[good_id].price;
                value_of_rgo_production += price * produced;
                new_market.goods[good_id].supply_base += produced;
            }

            new_market.income += value_of_rgo_production;
        }

        {
            // Calculate effective supply and demand (used for pricing)
            for good_id in good_types.keys() {
                let good_data = &mut new_market.goods[good_id];
                good_data.supply_effective += good_data.supply_base;
                good_data.supply_effective += good_data.supply_from_stock;

                good_data.demand_effective += good_data.demand_base;

                // Add a proportion of the stock to the effective supply
                const STOCK_SUPPLY_BONUS: f64 = 0.05;
                let from_stock = location.market.goods[good_id].stock * STOCK_SUPPLY_BONUS;
                good_data.supply_from_stock += from_stock;
            }
        }

        // Update good prices and stock
        for (good_id, good_type) in good_types {
            let new_good = &mut new_market.goods[good_id];

            // Price calculations
            {
                let sd_modifier = {
                    let numerator = new_good.demand_base - new_good.supply_effective;
                    let denominator = new_good
                        .supply_effective
                        .max(new_good.demand_effective)
                        .max(0.1);
                    (numerator / denominator).clamp(-0.75, 0.75)
                };
                let prosperity_modifier = location.prosperity.max(0.);
                let target_price =
                    good_type.price * (1. + sd_modifier) * (1. + prosperity_modifier);
                let current_price = location.market.goods[good_id].price;
                const PRICE_CONVERGENCE_SPEED: f64 = 0.1;
                let new_price = lerp_f64(current_price, target_price, PRICE_CONVERGENCE_SPEED);

                new_good.target_price = target_price;
                new_good.price = new_price;
            }

            // Handle stock
            {
                let prev_stock = location.market.goods[good_id].stock;
                let available = prev_stock + new_good.supply_base;
                new_good.consumed = available.min(new_good.demand_base);
                new_good.satisfaction = if new_good.demand_base <= 0.0 {
                    1.0
                } else {
                    (new_good.consumed / new_good.demand_base).min(1.)
                };

                let max_stock = location.population as f64 * GOODS_POPULATION_SCALE * 10.0;
                new_good.stock = (available - new_good.consumed).clamp(0.0, max_stock);
                new_good.stock_delta = new_good.stock - prev_stock;
            }

            // Food
            new_market.food_consumed += new_good.consumed * good_type.food_rate;
            new_market.food_stockpile += new_good.stock * good_type.food_rate;
        }

        // Update market proper
        location.market = new_market;
    }
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

                    let steps = sites.astar(start_node, end_node).unwrap_or_default().0;

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
                    // We are guaranteed colinearity by construction of the path
                    let ColinearPair {
                        start,
                        end,
                        t1: current_t,
                        t2: end_t,
                    } = GridCoord::as_colinear(party_data.position, destination).unwrap();

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

fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[derive(Default)]
struct CreateEntity<'a> {
    name: &'a str,
    kind_name: &'static str,
    agent: Option<CreateAgent<'a>>,
    location: Option<CreateLocation<'a>>,
    party: Option<CreateParty<'a>>,
    tokens: Option<&'a [CreateToken<'a>]>,
}

struct CreateAgent<'a> {
    tag: &'a str,
    flags: &'a [AgentFlag],
    political_parent: Option<&'a str>,
}

pub struct CreateToken<'a> {
    pub tag: &'a str,
    pub size: i64,
}

struct CreateLocation<'a> {
    site: &'a str,
    prosperity: f64,
    is_town: bool,
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
    pub prosperity: f64,
    pub tokens: &'a [CreateToken<'a>],
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
            "hillfort" => 2.,
            "village" => 1.5,
            _ => 1.,
        };
        let is_town = match params.settlement_kind {
            "town" => true,
            _ => false,
        };
        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            kind_name: "Location",
            agent: Some(CreateAgent {
                tag: "",
                flags: &[],
                political_parent: Some(params.faction),
            }),
            tokens: Some(params.tokens),
            location: Some(CreateLocation {
                site: params.site,
                prosperity: params.prosperity,
                is_town,
            }),
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

        let tokens = command.tokens.map(|cmds| {
            let container = sim.tokens.add_container();
            for create in cmds {
                match sim.tokens.types.lookup(create.tag) {
                    Some(typ) => {
                        sim.tokens.add_token(container, typ, create.size);
                    }
                    None => {
                        println!("Unknown token type '{}'", create.tag);
                        continue;
                    }
                }
            }
            container
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
            let mut influence_sources = vec![];

            if args.is_town {
                influence_sources.push(InfluenceSource {
                    kind: InfluenceKind::Market,
                    population_modifier: 1.,
                });
            }

            let location = sim.locations.insert(LocationData {
                entity,
                site,
                population: 0,
                prosperity: args.prosperity,
                market: Market::new(&sim.good_types),
                influence_sources,
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
        entity.tokens = tokens;
    }
}
