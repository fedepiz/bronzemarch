use macroquad::prelude as mq;
use simulation::{Extents, ObjectId, SimView, Simulation, V2};

use crate::{gui::WindowKind, *};

pub fn start() {
    let config = mq::Conf {
        window_width: 1600,
        window_height: 900,
        high_dpi: true,
        ..Default::default()
    };
    macroquad::Window::from_config(config, amain());
}

async fn amain() {
    let mut sim = Simulation::new();

    let mut gui = gui::Gui::new();
    egui_macroquad::cfg(|ctx| gui.setup(ctx));

    let mut board = board::Board::new(20.);
    let mut selected_entity: Option<ObjectId> = None;

    let mut view = simulation::SimView::default();
    // Pre-records the kind of windows the matching requested objects are
    let mut window_kinds = vec![];

    let mut is_paused = true;

    loop {
        if mq::is_key_pressed(mq::KeyCode::Escape) {
            break;
        }

        let mut is_mouse_over_ui = false;
        let mut is_keyboard_taken_by_ui = false;
        egui_macroquad::ui(|ctx| {
            for (kind, obj) in window_kinds.drain(..).zip(view.objects.drain(..)) {
                if let Some(obj) = obj {
                    gui.add_object(kind, obj);
                }
            }
            gui.tick(ctx);
            is_mouse_over_ui = ctx.wants_pointer_input();
            is_keyboard_taken_by_ui = ctx.wants_keyboard_input();
        });

        let map_item_ids: Vec<_> = view.map_items.iter().map(|x| x.id).collect();
        populate_board(&mut board, &view, selected_entity);

        if !is_mouse_over_ui && mq::is_mouse_button_pressed(mq::MouseButton::Left) {
            selected_entity = board
                .hovered()
                .and_then(|handle| map_item_ids.get(handle.0))
                .copied();
        }

        if !is_keyboard_taken_by_ui {
            update_camera_from_keyboard(&mut board);

            if mq::is_key_pressed(mq::KeyCode::Space) {
                is_paused = !is_paused;
            }
        }

        mq::clear_background(mq::LIGHTGRAY);
        board.draw();
        if is_paused {
            board.billboard("Paused");
        }
        egui_macroquad::draw();

        let mut request = simulation::TickRequest {
            advance_time: !is_paused,
            ..Default::default()
        };

        request.map_viewport = {
            let convert = |v: mq::Vec2| V2::new(v.x, v.y);
            let top_left = convert(board.screen_to_world(mq::Vec2::ZERO));
            let bottom_right = convert(
                board.screen_to_world(mq::Vec2::new(mq::screen_width(), mq::screen_height())),
            );
            simulation::Extents {
                top_left,
                bottom_right,
            }
        };

        {
            // Prepare next tick object requests
            window_kinds.clear();

            request.objects.push(ObjectId::global());
            window_kinds.push(WindowKind::TopStrip);

            request.objects.extend(selected_entity);
            window_kinds.extend(selected_entity.map(|_| WindowKind::Entity));
        }

        view = sim.tick(request);
        mq::next_frame().await;
    }
}

fn populate_board(board: &mut board::Board, view: &SimView, selected_entity: Option<ObjectId>) {
    board.clear();
    let mut ids = Vec::with_capacity(view.map_items.len());
    // Lines
    for (source, dest) in &view.map_lines {
        board.push_line(
            mq::Vec2::new(source.x, source.y),
            mq::Vec2::new(dest.x, dest.y),
        );
    }
    // Pawns
    for item in &view.map_items {
        let handle = board::Handle(ids.len());
        ids.push(item.id);

        let is_selected = Some(item.id) == selected_entity;
        let (border_color, text_color) = if is_selected {
            (mq::YELLOW, mq::YELLOW)
        } else {
            (mq::BLACK, mq::WHITE)
        };

        board.push_pawn(
            handle,
            &item.name,
            mq::Vec2::new(item.pos.x, item.pos.y),
            item.size,
            mq::GREEN,
            border_color,
            text_color,
        );
    }
}

fn update_camera_from_keyboard(board: &mut board::Board) {
    let mut dtranslate = mq::Vec2::ZERO;
    let mut dzoom = 0.0;

    const TRANSLATIONS: &'static [(mq::KeyCode, (f32, f32))] = &[
        (mq::KeyCode::W, (0., -1.)),
        (mq::KeyCode::S, (0., 1.)),
        (mq::KeyCode::A, (-1., 0.)),
        (mq::KeyCode::D, (1., 0.)),
    ];
    for &(key, dv) in TRANSLATIONS {
        if !mq::is_key_down(key) {
            continue;
        }
        dtranslate += mq::Vec2::from(dv);
    }

    const ZOOM: &'static [(mq::KeyCode, f32)] = &[(mq::KeyCode::Q, 1.), (mq::KeyCode::E, -1.)];
    for &(key, dz) in ZOOM {
        if !mq::is_key_down(key) {
            continue;
        }
        dzoom += dz;
    }

    board.update_camera(dtranslate, dzoom);
}
