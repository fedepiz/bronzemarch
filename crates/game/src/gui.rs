use simulation::Object;

#[derive(Default)]
pub(crate) struct Gui {
    objects: Vec<(WindowKind, Object)>,
}

impl Gui {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn setup(&mut self, ctx: &egui::Context) {
        ctx.set_pixels_per_point(1.6);
    }

    pub fn add_object(&mut self, kind: WindowKind, obj: Object) {
        self.objects.push((kind, obj))
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        for (window_idx, (kind, obj)) in self.objects.drain(..).enumerate() {
            match kind {
                WindowKind::TopStrip => top_strip(ctx, &obj),
                WindowKind::Entity => object_ui(ctx, window_idx, &obj),
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WindowKind {
    TopStrip,
    Entity,
}

fn top_strip(ctx: &egui::Context, obj: &Object) {
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(obj.txt("date"));
        });
    });
}

fn object_ui(ctx: &egui::Context, obj_idx: usize, obj: &Object) {
    let window_id = format!("object_window_{obj_idx}");
    egui::Window::new(obj.txt("name"))
        .id(window_id.into())
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(250.);

            let table = [
                ("Name", "name"),
                ("Kind", "kind"),
                ("Faction", "faction"),
                ("Country", "country"),
            ];
            field_table(ui, "overview-table", &table, obj);

            if let Some(obj) = obj.try_child("location") {
                ui.separator();
                ui.heading("Location");
                let table = [
                    ("Population", "population"),
                    ("Prosperity", "prosperity"),
                    ("Food", "food"),
                    ("Income", "income"),
                ];
                field_table(ui, "location-table", &table, obj);

                ui.separator();
                ui.heading("Pops");
                let table = [("Name", "name"), ("Size", "size")];
                rows_table(ui, "pop_grid", &table, obj.list("pops"));

                ui.separator();
                ui.heading("Market");
                let table = [
                    ("Name", "name"),
                    ("Stock", "stock"),
                    ("Supply", "supply"),
                    ("Demand", "demand"),
                    ("Price", "price"),
                ];
                rows_table(ui, "market-grid", &table, obj.list("market_goods"));
            }
        });
}

fn field_table(ui: &mut egui::Ui, grid_id: &str, table: &[(&str, &str)], obj: &Object) {
    egui::Grid::new(grid_id).show(ui, |ui| {
        for &(label, field) in table {
            if let Some(txt) = obj.try_text(field) {
                ui.label(label);
                ui.label(txt);
                ui.end_row();
            }
        }
    });
}

fn rows_table(ui: &mut egui::Ui, grid_id: &str, table: &[(&str, &str)], list: &[Object]) {
    egui::Grid::new(grid_id).striped(true).show(ui, |ui| {
        for &(txt, _) in table {
            ui.label(txt);
        }
        ui.end_row();
        for obj in list {
            for (_, field) in table {
                ui.label(obj.txt(field));
            }
            ui.end_row();
        }
    });
}
