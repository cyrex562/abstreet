use crate::common::CommonState;
use crate::helpers::{rotating_color, ID};
use crate::ui::UI;
use abstutil::{prettyprint_usize, Timer};
use ezgui::{Color, EventCtx, GfxCtx, Key, ModalMenu, Text};
use geom::Polygon;
use popdat::PopDat;
use std::collections::BTreeMap;

pub struct DataVisualizer {
    menu: ModalMenu,
    popdat: PopDat,
    tracts: BTreeMap<String, Tract>,

    // TODO Urgh. 0, 1, or 2.
    current_dataset: usize,
    current_tract: Option<String>,
}

struct Tract {
    polygon: Polygon,
    color: Color,

    num_bldgs: usize,
    num_parking_spots: usize,
}

impl DataVisualizer {
    pub fn new(ctx: &mut EventCtx, ui: &UI) -> DataVisualizer {
        let mut timer = Timer::new("initialize popdat");
        let popdat: PopDat = abstutil::read_binary("../data/shapes/popdat", &mut timer)
            .expect("Couldn't load popdat");

        DataVisualizer {
            menu: ModalMenu::new(
                "Data Visualizer",
                vec![
                    (Some(Key::Escape), "quit"),
                    (Some(Key::Num1), "household vehicles"),
                    (Some(Key::Num2), "commute times"),
                    (Some(Key::Num3), "commute modes"),
                ],
                ctx,
            ),
            tracts: clip(&popdat, ui, &mut timer),
            popdat,
            current_dataset: 0,
            current_tract: None,
        }
    }

    // Returns true if the we're done
    pub fn event(&mut self, ctx: &mut EventCtx, ui: &UI) -> bool {
        let mut txt = Text::prompt("Data Visualizer");
        if let Some(ref name) = self.current_tract {
            txt.add_line("Census ".to_string());
            txt.append(name.clone(), Some(ui.cs.get("OSD name color")));
            let tract = &self.tracts[name];
            txt.add_line(format!("{} buildings", prettyprint_usize(tract.num_bldgs)));
            txt.add_line(format!(
                "{} parking spots ",
                prettyprint_usize(tract.num_parking_spots)
            ));
        }
        self.menu.handle_event(ctx, Some(txt));
        ctx.canvas.handle_event(ctx.input);

        // TODO Remember which dataset we're showing and don't allow reseting to the same.
        if self.menu.action("quit") {
            return true;
        } else if self.current_dataset != 0 && self.menu.action("household vehicles") {
            self.current_dataset = 0;
        } else if self.current_dataset != 1 && self.menu.action("commute times") {
            self.current_dataset = 1;
        } else if self.current_dataset != 2 && self.menu.action("commute modes") {
            self.current_dataset = 2;
        }

        if !ctx.canvas.is_dragging() && ctx.input.get_moved_mouse().is_some() {
            if let Some(pt) = ctx.canvas.get_cursor_in_map_space() {
                self.current_tract = None;
                for (name, tract) in &self.tracts {
                    if tract.polygon.contains_pt(pt) {
                        self.current_tract = Some(name.clone());
                        break;
                    }
                }
            }
        }

        false
    }

    pub fn draw(&self, g: &mut GfxCtx, ui: &UI) {
        for (name, tract) in &self.tracts {
            let color = if Some(name.clone()) == self.current_tract {
                ui.cs.get("selected")
            } else {
                tract.color
            };
            g.draw_polygon(color, &tract.polygon);
        }

        self.menu.draw(g);
        if let Some(ref name) = self.current_tract {
            let mut osd = Text::new();
            osd.add_line("Census ".to_string());
            osd.append(name.clone(), Some(ui.cs.get("OSD name color")));
            CommonState::draw_custom_osd(g, osd);
        } else {
            CommonState::draw_osd(g, ui, None);
        }
    }
}

fn clip(popdat: &PopDat, ui: &UI, timer: &mut Timer) -> BTreeMap<String, Tract> {
    // TODO Partial clipping could be neat, except it'd be confusing to interpret totals.
    let mut results = BTreeMap::new();
    timer.start_iter("clip tracts", popdat.tracts.len());
    for (name, tract) in &popdat.tracts {
        timer.next();
        if let Some(pts) = ui.primary.map.get_gps_bounds().try_convert(&tract.pts) {
            // TODO We should actually make sure the polygon is completely contained within the
            // map's boundary.
            let polygon = Polygon::new(&pts);

            // TODO Don't just use the center...
            let mut num_bldgs = 0;
            let mut num_parking_spots = 0;
            for id in ui
                .primary
                .draw_map
                .get_matching_objects(polygon.get_bounds())
            {
                match id {
                    ID::Building(b) => {
                        if polygon.contains_pt(ui.primary.map.get_b(b).polygon.center()) {
                            num_bldgs += 1;
                        }
                    }
                    ID::Lane(l) => {
                        let lane = ui.primary.map.get_l(l);
                        if lane.is_parking() && polygon.contains_pt(lane.lane_center_pts.middle()) {
                            num_parking_spots += lane.number_parking_spots();
                        }
                    }
                    _ => {}
                }
            }

            results.insert(
                name.clone(),
                Tract {
                    polygon,
                    color: rotating_color(results.len()),
                    num_bldgs,
                    num_parking_spots,
                },
            );
        }
    }
    println!(
        "Clipped {} tracts from {}",
        results.len(),
        popdat.tracts.len()
    );
    results
}