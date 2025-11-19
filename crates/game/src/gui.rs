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

            let entries = [
                ("Name", "name"),
                ("Kind", "kind"),
                ("Leader", "leader"),
                ("Faction", "faction"),
            ];

            egui::Grid::new("overview-gui").show(ui, |ui| {
                for (label, field) in entries {
                    if let Some(txt) = obj.try_text(field) {
                        ui.label(label);
                        ui.label(txt);
                        ui.end_row();
                    }
                }
            });
        });
}
