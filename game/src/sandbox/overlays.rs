use crate::common::{
    Histogram, ObjectColorer, ObjectColorerBuilder, RoadColorer, RoadColorerBuilder,
};
use crate::game::{Transition, WizardState};
use crate::helpers::ID;
use crate::render::DrawOptions;
use crate::sandbox::bus_explorer::ShowBusRoute;
use crate::sandbox::SandboxMode;
use crate::ui::{ShowEverything, UI};
use abstutil::{prettyprint_usize, Counter};
use ezgui::{Choice, Color, Composite, Drawable, EventCtx, GeomBatch, GfxCtx, Key, Line, Text};
use geom::{Distance, Duration, PolyLine, Time};
use map_model::IntersectionID;
use sim::ParkingSpot;
use std::collections::HashSet;

pub enum Overlays {
    Inactive,
    ParkingAvailability(Time, RoadColorer),
    IntersectionDelay(Time, ObjectColorer),
    CumulativeThroughput(Time, ObjectColorer),
    FinishedTripsHistogram(Time, Histogram),
    BikeNetwork(RoadColorer),
    BusNetwork(RoadColorer),
    // Only set by certain gameplay modes
    BusRoute(ShowBusRoute),
    BusDelaysOverTime(Composite),
    IntersectionDemand(Time, IntersectionID, Drawable),
}

impl Overlays {
    pub fn event(&mut self, ctx: &mut EventCtx, ui: &UI) -> Option<Transition> {
        let now = ui.primary.sim.time();
        match self {
            // Don't bother with Inactive, BusRoute, BusDelaysOverTime, BikeNetwork, BusNetwork --
            // nothing needed or the gameplay mode will update it.
            Overlays::ParkingAvailability(t, _) if now != *t => {
                *self = Overlays::parking_availability(ctx, ui);
            }
            Overlays::IntersectionDelay(t, _) if now != *t => {
                *self = Overlays::intersection_delay(ctx, ui);
            }
            Overlays::CumulativeThroughput(t, _) if now != *t => {
                *self = Overlays::cumulative_throughput(ctx, ui);
            }
            Overlays::IntersectionDemand(t, i, _) if now != *t => {
                *self = Overlays::intersection_demand(*i, ctx, ui);
            }
            Overlays::FinishedTripsHistogram(t, _) if now != *t => {
                *self = Overlays::finished_trips_histogram(ctx, ui);
            }
            _ => {}
        };
        None
    }

    // True if active and should block normal drawing
    pub fn draw(&self, g: &mut GfxCtx, ui: &UI) -> bool {
        match self {
            Overlays::Inactive => false,
            Overlays::ParkingAvailability(_, ref heatmap)
            | Overlays::BikeNetwork(ref heatmap)
            | Overlays::BusNetwork(ref heatmap) => {
                heatmap.draw(g, ui);
                true
            }
            Overlays::IntersectionDelay(_, ref heatmap)
            | Overlays::CumulativeThroughput(_, ref heatmap) => {
                heatmap.draw(g, ui);
                true
            }
            Overlays::FinishedTripsHistogram(_, ref hgram) => {
                ui.draw(
                    g,
                    DrawOptions::new(),
                    &ui.primary.sim,
                    &ShowEverything::new(),
                );
                hgram.draw(g);
                true
            }
            Overlays::BusDelaysOverTime(ref composite) => {
                ui.draw(
                    g,
                    DrawOptions::new(),
                    &ui.primary.sim,
                    &ShowEverything::new(),
                );
                composite.draw(g);
                true
            }
            Overlays::IntersectionDemand(_, _, ref draw) => {
                ui.draw(
                    g,
                    DrawOptions::new(),
                    &ui.primary.sim,
                    &ShowEverything::new(),
                );
                g.redraw(draw);
                true
            }
            Overlays::BusRoute(ref s) => {
                s.draw(g, ui);
                true
            }
        }
    }

    pub fn change_overlays(_: &mut EventCtx, _: &mut UI) -> Option<Transition> {
        Some(Transition::Push(WizardState::new(Box::new(
            |wiz, ctx, _| {
                let (choice, _) = wiz.wrap(ctx).choose("Show which analytics overlay?", || {
                    // TODO Filter out the current
                    vec![
                        Choice::new("none", ()).key(Key::N),
                        Choice::new("parking availability", ()).key(Key::P),
                        Choice::new("intersection delay", ()).key(Key::I),
                        Choice::new("cumulative throughput", ()).key(Key::T),
                        Choice::new("finished trips histogram", ()).key(Key::H),
                        Choice::new("bike network", ()).key(Key::B),
                        Choice::new("bus network", ()).key(Key::U),
                    ]
                })?;
                Some(Transition::PopWithData(Box::new(move |state, ui, ctx| {
                    let mut sandbox = state.downcast_mut::<SandboxMode>().unwrap();
                    sandbox.overlay = match choice.as_ref() {
                        "none" => Overlays::Inactive,
                        "parking availability" => Overlays::parking_availability(ctx, ui),
                        "intersection delay" => Overlays::intersection_delay(ctx, ui),
                        "cumulative throughput" => Overlays::cumulative_throughput(ctx, ui),
                        "finished trips histogram" => Overlays::finished_trips_histogram(ctx, ui),
                        "bike network" => Overlays::bike_network(ctx, ui),
                        "bus network" => Overlays::bus_network(ctx, ui),
                        _ => unreachable!(),
                    };
                })))
            },
        ))))
    }
}

impl Overlays {
    fn parking_availability(ctx: &EventCtx, ui: &UI) -> Overlays {
        let (filled_spots, avail_spots) = ui.primary.sim.get_all_parking_spots();
        let mut txt = Text::from(Line("parking availability"));
        txt.add(Line(format!(
            "{} spots filled",
            prettyprint_usize(filled_spots.len())
        )));
        txt.add(Line(format!(
            "{} spots available ",
            prettyprint_usize(avail_spots.len())
        )));

        let awful = Color::BLACK;
        let bad = Color::RED;
        let meh = Color::YELLOW;
        let good = Color::GREEN;
        let mut colorer = RoadColorerBuilder::new(
            txt,
            vec![
                ("< 10%", awful),
                ("< 30%", bad),
                ("< 60%", meh),
                (">= 60%", good),
            ],
        );

        let lane = |spot| match spot {
            ParkingSpot::Onstreet(l, _) => l,
            ParkingSpot::Offstreet(b, _) => ui
                .primary
                .map
                .get_b(b)
                .parking
                .as_ref()
                .unwrap()
                .driving_pos
                .lane(),
        };

        let mut filled = Counter::new();
        let mut avail = Counter::new();
        let mut keys = HashSet::new();
        for spot in filled_spots {
            let l = lane(spot);
            keys.insert(l);
            filled.inc(l);
        }
        for spot in avail_spots {
            let l = lane(spot);
            keys.insert(l);
            avail.inc(l);
        }

        for l in keys {
            let open = avail.get(l);
            let closed = filled.get(l);
            let percent = (open as f64) / ((open + closed) as f64);
            let color = if percent >= 0.6 {
                good
            } else if percent > 0.3 {
                meh
            } else if percent > 0.1 {
                bad
            } else {
                awful
            };
            colorer.add(l, color, &ui.primary.map);
        }

        Overlays::ParkingAvailability(ui.primary.sim.time(), colorer.build(ctx, &ui.primary.map))
    }

    pub fn intersection_delay(ctx: &EventCtx, ui: &UI) -> Overlays {
        let fast = Color::GREEN;
        let meh = Color::YELLOW;
        let slow = Color::RED;
        let mut colorer = ObjectColorerBuilder::new(
            Text::from(Line(
                "intersection delay for traffic signals in the last 2 hours (90%ile)",
            )),
            vec![("< 10s", fast), ("<= 60s", meh), ("> 60s", slow)],
        );

        for i in ui.primary.map.all_intersections() {
            let delays = ui.primary.sim.get_analytics().intersection_delays(
                i.id,
                ui.primary.sim.time().clamped_sub(Duration::hours(2)),
                ui.primary.sim.time(),
            );
            if let Some(d) = delays.percentile(90.0) {
                let color = if d < Duration::seconds(10.0) {
                    fast
                } else if d <= Duration::seconds(60.0) {
                    meh
                } else {
                    slow
                };
                colorer.add(ID::Intersection(i.id), color);
            }
        }

        Overlays::IntersectionDelay(ui.primary.sim.time(), colorer.build(ctx, &ui.primary.map))
    }

    fn cumulative_throughput(ctx: &EventCtx, ui: &UI) -> Overlays {
        let light = Color::GREEN;
        let medium = Color::YELLOW;
        let heavy = Color::RED;
        let mut colorer = ObjectColorerBuilder::new(
            Text::from(Line("Throughput")),
            vec![
                ("< 50%ile", light),
                ("< 90%ile", medium),
                (">= 90%ile", heavy),
            ],
        );

        let stats = &ui.primary.sim.get_analytics().thruput_stats;

        // TODO If there are many duplicate counts, arbitrarily some will look heavier! Find the
        // disribution of counts instead.
        // TODO Actually display the counts at these percentiles
        // TODO Dump the data in debug mode
        {
            let roads = stats.count_per_road.sorted_asc();
            let p50_idx = ((roads.len() as f64) * 0.5) as usize;
            let p90_idx = ((roads.len() as f64) * 0.9) as usize;
            for (idx, r) in roads.into_iter().enumerate() {
                let color = if idx < p50_idx {
                    light
                } else if idx < p90_idx {
                    medium
                } else {
                    heavy
                };
                colorer.add(ID::Road(*r), color);
            }
        }
        // TODO dedupe
        {
            let intersections = stats.count_per_intersection.sorted_asc();
            let p50_idx = ((intersections.len() as f64) * 0.5) as usize;
            let p90_idx = ((intersections.len() as f64) * 0.9) as usize;
            for (idx, i) in intersections.into_iter().enumerate() {
                let color = if idx < p50_idx {
                    light
                } else if idx < p90_idx {
                    medium
                } else {
                    heavy
                };
                colorer.add(ID::Intersection(*i), color);
            }
        }

        Overlays::CumulativeThroughput(ui.primary.sim.time(), colorer.build(ctx, &ui.primary.map))
    }

    fn bike_network(ctx: &EventCtx, ui: &UI) -> Overlays {
        let mut colorer = RoadColorerBuilder::new(
            Text::from(Line("bike networks")),
            vec![("bike lanes", Color::GREEN)],
        );
        for l in ui.primary.map.all_lanes() {
            if l.is_biking() {
                colorer.add(l.id, Color::GREEN, &ui.primary.map);
            }
        }
        Overlays::BikeNetwork(colorer.build(ctx, &ui.primary.map))
    }

    fn bus_network(ctx: &EventCtx, ui: &UI) -> Overlays {
        let mut colorer = RoadColorerBuilder::new(
            Text::from(Line("bus networks")),
            vec![("bus lanes", Color::GREEN)],
        );
        for l in ui.primary.map.all_lanes() {
            if l.is_bus() {
                colorer.add(l.id, Color::GREEN, &ui.primary.map);
            }
        }
        Overlays::BusNetwork(colorer.build(ctx, &ui.primary.map))
    }

    pub fn finished_trips_histogram(ctx: &EventCtx, ui: &UI) -> Overlays {
        let now = ui.primary.sim.time();
        Overlays::FinishedTripsHistogram(
            now,
            Histogram::new(
                ui.primary
                    .sim
                    .get_analytics()
                    .finished_trip_deltas(now, ui.prebaked()),
                ctx,
            ),
        )
    }

    pub fn intersection_demand(i: IntersectionID, ctx: &EventCtx, ui: &UI) -> Overlays {
        let mut batch = GeomBatch::new();

        let mut total_demand = 0;
        let mut demand_per_group: Vec<(&PolyLine, usize)> = Vec::new();
        for g in ui.primary.map.get_traffic_signal(i).turn_groups.values() {
            let demand = ui
                .primary
                .sim
                .get_analytics()
                .thruput_stats
                .demand
                .get(&g.id)
                .cloned()
                .unwrap_or(0);
            if demand > 0 {
                total_demand += demand;
                demand_per_group.push((&g.geom, demand));
            }
        }

        for (pl, demand) in demand_per_group {
            let percent = (demand as f64) / (total_demand as f64);
            batch.push(
                Color::RED,
                pl.make_arrow(percent * Distance::meters(5.0)).unwrap(),
            );
        }

        Overlays::IntersectionDemand(ui.primary.sim.time(), i, batch.upload(ctx))
    }
}
