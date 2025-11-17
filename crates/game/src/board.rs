use std::io::Read;

use macroquad::prelude as mq;

pub(super) struct Board {
    camera: mq::Camera2D,
    font: mq::Font,
    world_unit: f32,
    strings: Vec<String>,
    lines: Vec<Line>,
    pawns: Vec<Pawn>,
    click_boxes: Vec<ClickBox>,
}

impl Board {
    pub fn new(world_unit: f32) -> Self {
        let display_rect = mq::Rect::new(0.0, 0.0, mq::screen_width(), mq::screen_height());
        let mut camera = mq::Camera2D::from_display_rect(display_rect);
        camera.target = mq::Vec2::ZERO;
        camera.zoom.y *= -1.;
        let font = load_font("assets/fonts/default.ttf");
        Self {
            camera,
            font,
            world_unit,
            strings: vec![],
            lines: vec![],
            pawns: vec![],
            click_boxes: vec![],
        }
    }

    pub fn screen_to_world(&self, pos: mq::Vec2) -> mq::Vec2 {
        self.camera.screen_to_world(pos) * self.world_unit
    }

    pub fn clear(&mut self) {
        self.strings.clear();
        self.lines.clear();
        self.pawns.clear();
        self.click_boxes.clear();

        // Ensure there is a valid "zero index" string
        self.push_string("");
    }

    pub fn hovered(&self) -> Option<Handle> {
        let screen_pos = mq::Vec2::from(mq::mouse_position());
        let world_pos = self.camera.screen_to_world(screen_pos);
        self.click_boxes
            .iter()
            .rev()
            .find(|cb| cb.bounds.contains(world_pos))
            .map(|cb| cb.handle)
    }

    pub fn push_pawn(
        &mut self,
        handle: Handle,
        name: &str,
        pos: mq::Vec2,
        size: f32,
        fill_color: mq::Color,
        stroke: mq::Color,
        text_color: mq::Color,
    ) {
        let pos = pos * self.world_unit;
        let size = size * self.world_unit;
        let bounds = mq::Rect::new(pos.x - size / 2., pos.y - size / 2., size, size);

        let name = if name.is_empty() {
            StringIdx::default()
        } else {
            self.push_string(name)
        };

        let name = Label {
            text: name,
            color: text_color,
            font_size: 20,
        };

        let stroke = Stroke {
            color: stroke,
            thickness: 4.,
        };

        self.pawns.push(Pawn {
            label: name,
            bounds,
            fill_color,
            stroke,
        });

        self.click_boxes.push(ClickBox { handle, bounds });
    }

    pub fn push_line(&mut self, source: mq::Vec2, destination: mq::Vec2) {
        let source = source * self.world_unit;
        let destination = destination * self.world_unit;
        self.lines.push(Line {
            source,
            destination,
            thicknkess: 6.,
            color: mq::GRAY.with_alpha(0.5),
        });
    }

    fn push_string(&mut self, text: &str) -> StringIdx {
        let id = StringIdx(self.strings.len());
        self.strings.push(String::default());
        self.strings.last_mut().unwrap().push_str(text);
        id
    }

    fn get_string<'a>(&'a self, id: StringIdx) -> &'a str {
        self.strings.get(id.0).map(|x| x.as_str()).unwrap_or("N/A")
    }

    pub fn draw(&self) {
        mq::push_camera_state();
        mq::set_camera(&self.camera);

        for line in &self.lines {
            mq::draw_line(
                line.source.x,
                line.source.y,
                line.destination.x,
                line.destination.y,
                line.thicknkess,
                line.color,
            );
        }

        for pawn in &self.pawns {
            fill_rect(&pawn.bounds, pawn.fill_color);
            stroke_rect(&pawn.bounds, &pawn.stroke);
            draw_label(self, &pawn.label, &pawn.bounds, Some(&self.font));
        }

        mq::pop_camera_state();
    }

    pub fn update_camera(&mut self, delta_translation: mq::Vec2, delta_zoom: f32) {
        let dt = mq::get_frame_time();
        self.camera.target += delta_translation * 200. * dt;
        self.camera.zoom *= 1.0 + delta_zoom * 2. * dt;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Handle(pub usize);

fn fill_rect(rect: &mq::Rect, color: mq::Color) {
    mq::draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
}

fn stroke_rect(rect: &mq::Rect, stroke: &Stroke) {
    mq::draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        stroke.thickness,
        stroke.color,
    );
}

fn draw_label(board: &Board, label: &Label, bounds: &mq::Rect, font: Option<&mq::Font>) {
    let text = board.get_string(label.text);
    let pad = 4.0;
    let measure = mq::measure_text(text, font, label.font_size, 1.0);

    let x = bounds.x + (bounds.w - measure.width) / 2.;

    // y is shifted down by pad a bit
    let y = bounds.y + bounds.h + pad * 1.5;
    let border = mq::Rect::new(
        x - pad,
        y - pad,
        measure.width + 2. * pad,
        measure.height + 2. * pad,
    );
    fill_rect(&border, mq::BLACK.with_alpha(0.5));
    mq::draw_text_ex(
        text,
        x,
        y + measure.offset_y,
        mq::TextParams {
            color: label.color,
            font,
            font_size: label.font_size,
            ..Default::default()
        },
    );
}

struct Line {
    source: mq::Vec2,
    destination: mq::Vec2,
    thicknkess: f32,
    color: mq::Color,
}

#[derive(Default)]
struct Pawn {
    label: Label,
    bounds: mq::Rect,
    fill_color: mq::Color,
    stroke: Stroke,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StringIdx(usize);

#[derive(Default)]
struct Label {
    text: StringIdx,
    color: mq::Color,
    font_size: u16,
}

#[derive(Default)]
struct Stroke {
    color: mq::Color,
    thickness: f32,
}

struct ClickBox {
    handle: Handle,
    bounds: mq::Rect,
}

fn load_font(path: &str) -> mq::Font {
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    let mut buf = vec![];
    reader.read_to_end(&mut buf).unwrap();
    mq::load_ttf_font_from_bytes(&buf).unwrap()
}
