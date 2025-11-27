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
                let table = [
                    Row {
                        label: "Name",
                        primary: "name",
                        tooltip: &[],
                    },
                    Row {
                        label: "Size",
                        primary: "size",
                        tooltip: &[],
                    },
                ];
                rows_table(ui, "pop_grid", &table, obj.list("pops"));

                ui.separator();
                ui.heading("Market");
                let table = [
                    Row {
                        label: "Name",
                        primary: "name",
                        tooltip: &[],
                    },
                    Row {
                        label: "Stock",
                        primary: "stock",
                        tooltip: &[("Change", "stock_delta")],
                    },
                    Row {
                        label: "Supply",
                        primary: "supply_effective",
                        tooltip: &[
                            ("Base", "supply_base"),
                            ("From stockpile", "supply_from_stock"),
                        ],
                    },
                    Row {
                        label: "Demand",
                        primary: "demand_effective",
                        tooltip: &[("Base", "demand_base"), ("Satisfaction", "satisfaction")],
                    },
                    Row {
                        label: "Price",
                        primary: "price",
                        tooltip: &[("Target", "target_price"), ("Change", "price_delta")],
                    },
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

struct Row<'a> {
    label: &'a str,
    primary: &'a str,
    tooltip: &'a [(&'a str, &'a str)],
}

fn rows_table(ui: &mut egui::Ui, grid_id: &str, table: &[Row], list: &[Object]) {
    egui::Grid::new(grid_id).striped(true).show(ui, |ui| {
        for row in table {
            ui.label(row.label);
        }
        ui.end_row();
        for obj in list {
            for row in table {
                let primary = obj.txt(row.primary);
                let response = ui.label(primary);
                if !row.tooltip.is_empty() {
                    response.on_hover_ui(|ui| {
                        ui.heading(format!("{} {}", row.label, primary));
                        ui.separator();
                        field_table(ui, "hover-grid", row.tooltip, obj);
                    });
                }
            }
            ui.end_row();
        }
    });
}
