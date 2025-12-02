use slotmap::SecondaryMap;
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
    pub num_ticks: usize,
    pub map_viewport: Extents,
    pub objects_to_extract: Vec<ObjectId>,
}

pub(super) fn tick(sim: &mut Simulation, mut request: TickRequest, arena: &Arena) -> SimView {
    // Apply movement orders
    if let Some((subject, target)) = request.commands.move_to {
        apply_move_order_to(sim, subject, target);
    }

    // Inner ticks
    if request.num_ticks == 0 {
        let cmds = std::mem::take(&mut request.commands);
        tick_inner(sim, cmds, false, arena);
    }
    for _ in 0..request.num_ticks {
        let cmds = std::mem::take(&mut request.commands);
        tick_inner(sim, cmds, true, arena);
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

fn tick_inner(sim: &mut Simulation, mut commands: TickCommands, advance_time: bool, arena: &Arena) {
    let mut create_entitity_requests = vec![];
    if advance_time {
        sim.date.advance();

        let is_new_day = sim.date.is_new_day();

        tick_influences(arena, &mut sim.sites, &sim.locations);

        // Pressures
        {
            let events = tick_pressures(&mut sim.pressurables, is_new_day);
            let creations = handle_pressure_events(arena, sim, events);
            create_entitity_requests.extend(creations);
        }

        // Simulate economy at locations
        tick_location_economy(
            arena,
            &mut sim.locations,
            &sim.tokens,
            &sim.good_types,
            &sim.sites,
            is_new_day,
        );

        // nnnnnnors
        let effects = tick_behaviors::tick_behaviors(sim);

        transfer::resolve(sim, effects.transfers);
        trade::resolve(sim, effects.trade_events);

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
        let cmds = commands
            .create_entity_cmds
            .drain(..)
            .chain(create_entitity_requests);
        process_entity_create_commands(sim, cmds);
    }

    // Despawns
    let mut despawns = vec![];
    despawns.extend(
        sim.beahviors
            .values()
            .filter(|data| data.request_despawn)
            .map(|x| x.entity),
    );

    for entity in despawns {
        let entity = match sim.entities.remove(entity) {
            Some(x) => x,
            None => continue,
        };
        if let Some(id) = entity.party {
            sim.parties.remove(id);
        }
        if let Some(id) = entity.behavior {
            sim.beahviors.remove(id);
        }
        if let Some(id) = entity.agent {
            sim.agents.despawn(arena, id);
        }
        if let Some(id) = entity.location {
            let location = sim.locations.remove(id).unwrap();
            sim.tokens.despawn(location.tokens);
            sim.sites.unbind_location(location.site);
        }
        if let Some(id) = entity.pressure_agent {
            sim.pressurables.remove(id);
        }
    }
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

    for location in locations.values() {
        let mut influences = arena.new_vec();

        for source_data in &location.influence_sources {
            let mut power = 0;

            // Add in population weight
            power += (source_data.population_modifier * location.population as f64).round() as i64;

            if power > 0 {
                influences.push((
                    InfluenceType {
                        kind: source_data.kind,
                        source: location.party,
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

#[derive(Clone, Copy)]
enum PressureEventType {
    SpawnFarmer,
}

struct PressureEvent {
    typ: PressureEventType,
    target: EntityId,
}

fn tick_pressures(agents: &mut Pressurables, is_new_day: bool) -> Vec<PressureEvent> {
    let mut events = vec![];
    if is_new_day {
        for agent in agents.values_mut() {
            for &(typ, value) in &agent.innate_growth {
                agent.current.update(typ, |x| (x + value).max(0.));
            }
        }

        struct Trigger {
            target: PressureType,
            threshold: f64,
            subtract: f64,
            event: PressureEventType,
        }

        const TRIGGERS: &[Trigger] = &[Trigger {
            target: PressureType::Farmer,
            threshold: 20.,
            subtract: 20.,
            event: PressureEventType::SpawnFarmer,
        }];

        for agent in agents.values_mut() {
            for trigger in TRIGGERS {
                let current = *agent.current.get(trigger.target);
                if current >= trigger.threshold {
                    agent
                        .current
                        .set(trigger.target, (current - trigger.subtract).max(0.));
                    events.push(PressureEvent {
                        typ: trigger.event,
                        target: agent.entity,
                    });
                }
            }
        }
    }
    events
}

fn handle_pressure_events<'a>(
    arena: &'a Arena,
    sim: &Simulation,
    events: Vec<PressureEvent>,
) -> Vec<CreateEntity<'a>> {
    let mut out = vec![];
    // Handle pressure events
    for event in events {
        match event.typ {
            PressureEventType::SpawnFarmer => {
                let target_entity = &sim.entities[event.target];

                let political_parent = target_entity
                    .agent
                    .and_then(|id| sim.agents.political_hierarchy.parent(id))
                    .and_then(|id| sim.agents.tags.reverse_lookup(&id))
                    .map(|str| arena.alloc_str(str));

                let target_location = &sim.locations[target_entity.location.unwrap()];
                let site = arena.alloc_str(&sim.sites[target_location.site].tag);

                out.push(CreateEntity {
                    name: "Farmers",
                    agent: Some(CreateAgent {
                        tag: "",
                        flags: &[],
                        political_parent,
                        cash: 1000.,
                    }),
                    party: Some(CreateParty {
                        site,
                        image: "farmers",
                        size: 1.,
                        movement_speed: 2.,
                        layer: 1,
                    }),
                    behavior: Some(CreateBehavior {
                        base: Some(target_entity.party.unwrap()),
                    }),
                    ..Default::default()
                });
            }
        }
    }
    out
}

fn tick_location_economy(
    arena: &Arena,
    locations: &mut Locations,
    tokens: &Tokens,
    good_types: &GoodTypes,
    sites: &Sites,
    tick_market: bool,
) {
    // New location economic tick
    for location in locations.values_mut() {
        let tokens = arena.alloc_iter(tokens.all_tokens_in(location.tokens));

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
                destination = target.and_then(|tgt| match tgt {
                    MovementTarget::Site(site) => Some(GridCoord::at(site)),
                    MovementTarget::Party(party) => sim.parties.get(party).map(|x| x.position),
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
    pressure_agent: Option<CreatePressureAgent<'a>>,
    behavior: Option<CreateBehavior>,
}

struct CreateAgent<'a> {
    tag: &'a str,
    flags: &'a [AgentFlag],
    political_parent: Option<&'a str>,
    cash: f64,
}

pub struct CreateToken<'a> {
    pub tag: &'a str,
    pub size: i64,
}

pub struct CreatePressureAgent<'a> {
    pressures: &'a [(PressureType, f64)],
}

struct CreateLocation<'a> {
    site: &'a str,
    prosperity: f64,
    is_town: bool,
    tokens: &'a [CreateToken<'a>],
}

struct CreateParty<'a> {
    site: &'a str,
    image: &'static str,
    size: f32,
    movement_speed: f32,
    layer: u8,
}

struct CreateBehavior {
    base: Option<PartyId>,
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
    pub settlement_kind: &'static str,
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
        let pressures: &[(PressureType, f64)] = match params.settlement_kind {
            "village" => &[(PressureType::Farmer, 1.0)],
            _ => &[],
        };

        self.create_entity_cmds.push(CreateEntity {
            name: params.name,
            kind_name: "Location",
            agent: Some(CreateAgent {
                tag: "",
                flags: &[],
                political_parent: Some(params.faction),
                cash: 0.,
            }),
            location: Some(CreateLocation {
                site: params.site,
                prosperity: params.prosperity,
                is_town,
                tokens: params.tokens,
            }),
            party: Some(CreateParty {
                site: params.site,
                image: params.settlement_kind,
                size,
                movement_speed: 0.,
                layer: 0,
            }),
            pressure_agent: Some(CreatePressureAgent { pressures }),
            ..Default::default()
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
                cash: 0.,
            }),
            party: Some(CreateParty {
                site: params.site,
                image: "person",
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
                cash: 0.,
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

        let agent = command.agent.map(|args| {
            let id = sim.agents.insert(AgentData {
                entity,
                flags: AgentFlags::new(args.flags),
                cash: args.cash,
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
                location: None,
                image: args.image,
                position,
                pos,
                size: args.size,
                layer: args.layer,
                movement_speed: args.movement_speed,
                movement: PartyMovement::default(),
                good_stock: GoodStock::new(&sim.good_types),
            });
            Some(id)
        });

        let location = command.location.and_then(|args| {
            let site = match sim.sites.lookup(args.site) {
                Some((id, _)) => id,
                None => {
                    println!("Undefined site '{}'", args.site);
                    return None;
                }
            };

            let party = match party {
                Some(id) => id,
                None => {
                    println!("Location creation requires party");
                    return None;
                }
            };

            let tokens = sim.tokens.add_container();
            for create in args.tokens {
                match sim.tokens.types.lookup(create.tag) {
                    Some(typ) => {
                        sim.tokens.add_token(tokens, typ, create.size);
                    }
                    None => {
                        println!("Unknown token type '{}'", create.tag);
                        continue;
                    }
                }
            }

            let mut influence_sources = vec![];

            if args.is_town {
                influence_sources.push(InfluenceSource {
                    kind: InfluenceKind::Market,
                    population_modifier: 1.,
                });
            }

            let location = sim.locations.insert(LocationData {
                entity,
                party,
                site,
                tokens,
                population: 0,
                prosperity: args.prosperity,
                market: Market::new(&sim.good_types),
                influence_sources,
            });
            sim.sites.bind_location(site, location);

            sim.parties[party].location = Some(location);

            Some(location)
        });

        let pressure_agent = command.pressure_agent.map(|args| {
            sim.pressurables.insert(Pressureble {
                entity,
                current: PressureMap::default(),
                innate_growth: args.pressures.iter().copied().collect(),
            })
        });

        let behavior = command.behavior.map(|args| {
            let goal = match args.base {
                Some(base) => Goal::LocalTrade { base },
                None => Goal::Idle,
            };
            sim.beahviors.insert(Behavior {
                entity,
                goal,
                ..Default::default()
            })
        });

        let entity = &mut sim.entities[entity];
        entity.agent = agent;
        entity.party = party;
        entity.location = location;
        entity.pressure_agent = pressure_agent;
        entity.behavior = behavior;
    }
}

mod tick_behaviors {
    use slotmap::Key;

    #[derive(Default)]
    pub(super) struct Effects {
        pub transfers: Vec<super::transfer::Event>,
        pub trade_events: Vec<super::trade::Event>,
    }

    use super::*;
    pub(super) fn tick_behaviors(sim: &mut Simulation) -> Effects {
        let mut effects = Effects::default();

        let mut behaviors = std::mem::take(&mut sim.beahviors);
        for (_, behavior) in &mut behaviors {
            let my_entity = &sim.entities[behavior.entity];
            let my_party = &sim.parties[my_entity.party.unwrap()];

            behavior.task = behavior
                .task
                .take()
                .filter(|task| {
                    let validation = validate_task(sim, task, my_party);
                    if validation.is_over {
                        on_task_complete(sim, task, &validation, behavior, &mut effects);
                    }
                    !validation.is_over
                })
                .or_else(|| decide_task(sim, &behavior.goal, &behavior.memory));
        }

        for (_, behavior) in &behaviors {
            let party = sim.entities[behavior.entity].party.unwrap();
            let party_data = &mut sim.parties[party];
            party_data.movement.target = behavior
                .task
                .as_ref()
                .filter(|x| !x.target.is_null())
                .map(|x| MovementTarget::Party(x.target));
        }

        sim.beahviors = behaviors;

        effects
    }

    #[derive(Default)]
    struct TaskValidation {
        is_over: bool,
        at_target: Option<PartyId>,
    }

    fn validate_task(sim: &Simulation, task: &Task, my_party: &PartyData) -> TaskValidation {
        let mut result = TaskValidation::default();

        if task.target.is_null() {
            result.is_over = true;
        }

        if let Some(target) = sim.parties.get(task.target) {
            if !task.continue_after_arrival && my_party.position == target.position {
                result.is_over = true;
                result.at_target = Some(task.target)
            }
        }

        result
    }

    fn on_task_complete(
        sim: &Simulation,
        task: &Task,
        validation: &TaskValidation,
        behavior: &mut Behavior,
        effects: &mut Effects,
    ) {
        behavior.memory.state = task.on_complete_state;

        if task.despawn_on_complete {
            behavior.request_despawn = true;
        }

        if task.trade_with_target
            && let Some(target) = validation.at_target
            && let Some(location) = sim.parties[target].location
        {
            let entity = &sim.entities[behavior.entity];
            effects.trade_events.push(trade::Event {
                party: entity.party.unwrap(),
                agent: entity.agent.unwrap(),
                location,
            });
        }

        if task.give_away_to_target
            && let Some(target) = validation.at_target
        {
            let source = sim.entities[behavior.entity].party.unwrap();
            effects
                .transfers
                .push(super::transfer::Event { source, target });
        }
    }

    fn decide_task(sim: &Simulation, goal: &Goal, memory: &BehaviorMemory) -> Option<Task> {
        match goal {
            Goal::Idle => None,
            &Goal::LocalTrade { base } => {
                const STATE_BEGIN: usize = 0;
                const STATE_OUTGOING: usize = 1;
                const STATE_RETURING: usize = 2;
                let base_party = sim.parties.get(base)?;
                // Are we on the outgoing or the return leg?
                // Go home if we are not home
                Some(
                    if memory.state == STATE_BEGIN || memory.state == STATE_RETURING {
                        // If we already had traded once (by completing the outgoing task), mark for death
                        let is_returning = memory.state == STATE_RETURING;
                        let on_complete_state = if !is_returning {
                            STATE_OUTGOING
                        } else {
                            STATE_RETURING
                        };
                        Task {
                            target: base,
                            give_away_to_target: is_returning,
                            trade_with_target: !is_returning,
                            despawn_on_complete: is_returning,
                            on_complete_state,
                            ..Default::default()
                        }
                    } else {
                        // Set out from home
                        let site = base_party.position.as_site()?;
                        let target = sim.sites[site]
                            .influences
                            .top_source(InfluenceKind::Market)?;
                        Task {
                            target,
                            on_complete_state: STATE_RETURING,
                            trade_with_target: true,
                            ..Default::default()
                        }
                    },
                )
            }
        }
    }
}

mod transfer {
    use super::*;
    use crate::PartyId;

    #[derive(Clone, Copy)]
    pub(super) struct Event {
        pub source: PartyId,
        pub target: PartyId,
    }

    pub fn resolve(sim: &mut Simulation, events: impl IntoIterator<Item = Event>) {
        for event in events {
            let source_data = &mut sim.parties[event.source];
            let bundle = source_data.good_stock.amount.clone();
            source_data.good_stock.clear();
            let target_data = &mut sim.parties[event.target];
            match target_data.location {
                Some(location) => {
                    let market = &mut sim.locations[location].market;
                    for (good_id, value) in bundle {
                        market.goods[good_id].stock += value;
                        market.goods[good_id].stock_delta += value;
                    }
                }
                None => {
                    target_data.good_stock.add_goods(bundle);
                }
            }
        }
    }
}

mod trade {
    use super::*;

    #[derive(Clone, Copy)]
    pub(super) struct Event {
        pub party: PartyId,
        pub agent: AgentId,
        pub location: LocationId,
    }

    pub fn resolve(sim: &mut Simulation, events: impl IntoIterator<Item = Event>) {
        let scratch = &mut Scratch::new(&sim.good_types);
        let mut traders = collect_traders(sim, events);

        // Process
        for trader in &mut traders {
            let market = &mut sim.locations[trader.event.location].market;
            resolve_trade(&sim.good_types, trader, market, scratch);
        }

        // Write back
        for trader in traders {
            let agent_data = &mut sim.agents[trader.event.agent];
            let party_data = &mut sim.parties[trader.event.party];

            agent_data.cash = trader.cash;
            for good_id in sim.good_types.keys() {
                party_data.good_stock[good_id] = trader.goods[good_id].quantity;
            }
        }
    }

    fn collect_traders(sim: &Simulation, events: impl IntoIterator<Item = Event>) -> Vec<Trader> {
        events
            .into_iter()
            .map(|event| {
                let cash = sim.agents[event.agent].cash;
                let party_data = &sim.parties[event.party];
                let goods = sim
                    .good_types
                    .keys()
                    .map(|good_id| {
                        let quantity = party_data.good_stock[good_id];
                        let data = TraderGood {
                            quantity,
                            can_sell: true,
                            can_buy: true,
                        };
                        (good_id, data)
                    })
                    .collect();

                Trader { cash, goods, event }
            })
            .collect()
    }

    #[derive(Clone, Copy, Default)]
    struct TraderGood {
        quantity: f64,
        can_sell: bool,
        can_buy: bool,
    }

    struct Trader {
        cash: f64,
        goods: SecondaryMap<GoodId, TraderGood>,
        event: Event,
    }

    struct Scratch {
        weights: SecondaryMap<GoodId, f64>,
    }

    impl Scratch {
        fn new(good_types: &GoodTypes) -> Self {
            Self {
                weights: good_types.keys().map(|x| (x, 0.0)).collect(),
            }
        }
    }

    fn resolve_trade(
        goods: &GoodTypes,
        trader: &mut Trader,
        market: &mut Market,
        scratch: &mut Scratch,
    ) {
        // Decide what to buy and what to sell
        scratch.weights.values_mut().for_each(|x| *x = 0.0);

        // Perform sales
        for good_id in goods.keys() {
            let in_trader = &mut trader.goods[good_id];
            if !in_trader.can_sell {
                continue;
            }

            let in_market = &mut market.goods[good_id];

            let quantity = in_trader.quantity;
            let value = in_market.price * quantity;
            trader.cash += value;

            in_market.stock += quantity;
            in_market.stock_delta += quantity;
            in_trader.quantity -= quantity;
        }

        // Perform buys
        // First calculate how much money the trader wants to spend on each goods
        let mut total_weight = 0.0;
        for good_id in goods.keys() {
            let in_trader = &trader.goods[good_id];
            let in_market = &market.goods[good_id];

            let want_weight = if in_trader.can_buy { 1.0 } else { 0.0 };
            let exists_weight = if in_market.stock <= 0.0 { 0.0 } else { 1.0 };
            let price_weight = 1.0 / in_market.price;
            let weight = price_weight * want_weight * exists_weight;
            scratch.weights[good_id] = weight;
            total_weight += weight;
        }

        // Actually effectuate the transaction
        if total_weight != 0.0 {
            for good_id in goods.keys() {
                let weight = scratch.weights[good_id];
                let prop = weight / total_weight;
                let cash_allocated = (trader.cash * prop).min(trader.cash);

                let in_market = &mut market.goods[good_id];
                let price = in_market.price;
                let can_afford = if price == 0.0 {
                    f64::MAX
                } else {
                    cash_allocated / price
                };
                let bought = can_afford.min(in_market.stock);
                in_market.stock -= bought;
                in_market.stock_delta -= bought;

                let in_trader = &mut trader.goods[good_id];
                in_trader.quantity += bought;
                trader.cash = (trader.cash - bought * in_market.price).max(0.);
            }
        }
    }
}
