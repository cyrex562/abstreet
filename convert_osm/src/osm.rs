use abstutil::{FileWithProgress, Timer};
use geom::{Distance, LonLat};
use map_model::{raw_data, AreaType};
use osm_xml;
use std::collections::{BTreeMap, HashMap};

pub fn osm_to_raw_roads(
    osm_path: &str,
    boundary_polygon: &Vec<LonLat>,
    timer: &mut Timer,
) -> (
    Vec<raw_data::Road>,
    Vec<raw_data::Building>,
    Vec<raw_data::Area>,
) {
    let (reader, done) = FileWithProgress::new(osm_path).unwrap();
    let doc = osm_xml::OSM::parse(reader).expect("OSM parsing failed");
    println!(
        "OSM doc has {} nodes, {} ways, {} relations",
        doc.nodes.len(),
        doc.ways.len(),
        doc.relations.len()
    );
    done(timer);

    let mut id_to_way: HashMap<i64, Vec<LonLat>> = HashMap::new();
    let mut roads: Vec<raw_data::Road> = Vec::new();
    let mut buildings: Vec<raw_data::Building> = Vec::new();
    let mut areas: Vec<raw_data::Area> = Vec::new();
    timer.start_iter("processing OSM ways", doc.ways.len());
    for way in doc.ways.values() {
        timer.next();

        let mut valid = true;
        let mut pts = Vec::new();
        for node_ref in &way.nodes {
            match doc.resolve_reference(node_ref) {
                osm_xml::Reference::Node(node) => {
                    pts.push(LonLat::new(node.lon, node.lat));
                }
                // Don't handle nested ways/relations yet
                _ => {
                    valid = false;
                }
            }
        }
        if !valid {
            continue;
        }
        let tags = tags_to_map(&way.tags);
        if is_road(&tags) {
            roads.push(raw_data::Road {
                osm_way_id: way.id,
                points: pts,
                osm_tags: tags,
                // We'll fill this out later
                i1: raw_data::StableIntersectionID(0),
                i2: raw_data::StableIntersectionID(0),
                parking_lane_fwd: false,
                parking_lane_back: false,
            });
        } else if is_bldg(&tags) {
            buildings.push(raw_data::Building {
                osm_way_id: way.id,
                points: pts,
                osm_tags: tags,
                num_residential_units: None,
            });
        } else if let Some(at) = get_area_type(&tags) {
            areas.push(raw_data::Area {
                area_type: at,
                osm_id: way.id,
                points: pts,
                osm_tags: tags,
            });
        } else {
            // The way might be part of a relation later.
            id_to_way.insert(way.id, pts);
        }
    }

    timer.start_iter("processing OSM relations", doc.relations.len());
    for rel in doc.relations.values() {
        timer.next();
        let tags = tags_to_map(&rel.tags);
        if let Some(at) = get_area_type(&tags) {
            if tags.get("type") == Some(&"multipolygon".to_string()) {
                let mut ok = true;
                let mut pts_per_way: Vec<Vec<LonLat>> = Vec::new();
                for member in &rel.members {
                    match *member {
                        osm_xml::Member::Way(osm_xml::UnresolvedReference::Way(id), ref role) => {
                            // If the way is clipped out, that's fine
                            if let Some(pts) = id_to_way.get(&id) {
                                if role == "outer" {
                                    pts_per_way.push(pts.to_vec());
                                } else {
                                    println!(
                                        "Relation {} has unhandled member role {}, ignoring it",
                                        rel.id, role
                                    );
                                }
                            }
                        }
                        _ => {
                            println!("Relation {} refers to {:?}", rel.id, member);
                            ok = false;
                        }
                    }
                }
                if ok {
                    let polygons = glue_multipolygon(pts_per_way, boundary_polygon);
                    if polygons.is_empty() {
                        println!("Relation {} failed to glue multipolygon", rel.id);
                    } else {
                        for points in polygons {
                            areas.push(raw_data::Area {
                                area_type: at,
                                osm_id: rel.id,
                                points,
                                osm_tags: tags.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    (roads, buildings, areas)
}

fn tags_to_map(raw_tags: &[osm_xml::Tag]) -> BTreeMap<String, String> {
    raw_tags
        .iter()
        .map(|tag| (tag.key.clone(), tag.val.clone()))
        .collect()
}

fn is_road(tags: &BTreeMap<String, String>) -> bool {
    if !tags.contains_key("highway") {
        return false;
    }

    // https://github.com/Project-OSRM/osrm-backend/blob/master/profiles/car.lua is another
    // potential reference
    for &value in &[
        // List of non-car types from https://wiki.openstreetmap.org/wiki/Key:highway
        // TODO Footways are very useful, but they need more work to associate with main roads
        "footway",
        "living_street",
        "pedestrian",
        "track",
        "bus_guideway",
        "escape",
        "raceway",
        "bridleway",
        "steps",
        "path",
        "cycleway",
        "proposed",
        "construction",
        // This one's debatable. Includes alleys.
        "service",
        // more discovered manually
        "abandoned",
        "elevator",
        "planned",
        "razed",
    ] {
        if tags.get("highway") == Some(&String::from(value)) {
            return false;
        }
    }

    true
}

fn is_bldg(tags: &BTreeMap<String, String>) -> bool {
    tags.contains_key("building")
}

fn get_area_type(tags: &BTreeMap<String, String>) -> Option<AreaType> {
    if tags.get("leisure") == Some(&"park".to_string()) {
        return Some(AreaType::Park);
    }
    if tags.get("leisure") == Some(&"golf_course".to_string()) {
        return Some(AreaType::Park);
    }
    if tags.get("natural") == Some(&"wood".to_string()) {
        return Some(AreaType::Park);
    }
    if tags.get("landuse") == Some(&"cemetery".to_string()) {
        return Some(AreaType::Park);
    }
    if tags.get("natural") == Some(&"water".to_string()) {
        return Some(AreaType::Water);
    }
    None
}

// The result could be more than one disjoint polygon.
fn glue_multipolygon(
    mut pts_per_way: Vec<Vec<LonLat>>,
    boundary_polygon: &Vec<LonLat>,
) -> Vec<Vec<LonLat>> {
    // First deal with all of the closed loops.
    let mut polygons: Vec<Vec<LonLat>> = Vec::new();
    pts_per_way.retain(|pts| {
        if pts[0] == *pts.last().unwrap() {
            polygons.push(pts.to_vec());
            false
        } else {
            true
        }
    });
    if pts_per_way.is_empty() {
        return polygons;
    }

    // The main polygon
    let mut result = pts_per_way.pop().unwrap();
    let mut reversed = false;
    while !pts_per_way.is_empty() {
        let glue_pt = *result.last().unwrap();
        if let Some(idx) = pts_per_way
            .iter()
            .position(|pts| pts[0] == glue_pt || *pts.last().unwrap() == glue_pt)
        {
            let mut append = pts_per_way.remove(idx);
            if append[0] != glue_pt {
                append.reverse();
            }
            result.pop();
            result.extend(append);
        } else {
            if reversed {
                // Totally filter the thing out, since something clearly broke.
                return Vec::new();
            } else {
                reversed = true;
                result.reverse();
                // Try again!
            }
        }
    }
    extrude_to_boundary(boundary_polygon, &mut result);
    polygons.push(result);
    polygons
}

fn extrude_to_boundary(boundary_polygon: &Vec<LonLat>, result: &mut Vec<LonLat>) {
    // Some ways of the multipolygon are clipped out. Connect the ends by traveling along the
    // boundary polygon in the closest direction (clockwise or counter-clockwise).
    let first_pt = result[0];
    let last_pt = *result.last().unwrap();
    if first_pt == last_pt {
        return;
    }

    if true {
        // Simple resolution:
        result.push(first_pt);
    } else {
        // Proper resolution:
        let closest_to_last = *boundary_polygon
            .iter()
            .min_by_key(|pt| pt.gps_dist_meters(last_pt))
            .unwrap();
        let closest_to_first = *boundary_polygon
            .iter()
            .min_by_key(|pt| pt.gps_dist_meters(first_pt))
            .unwrap();
        println!("first pt is {}, last pt is {}", first_pt, last_pt);
        println!(
            "boundary closest... first pt is {}, last pt is {}",
            closest_to_first, closest_to_last
        );

        let slice1 = find_slice(boundary_polygon, closest_to_last, closest_to_first);
        let mut backwards_boundary: Vec<LonLat> = boundary_polygon.to_vec();
        backwards_boundary.reverse();
        let slice2 = find_slice(&backwards_boundary, closest_to_last, closest_to_first);
        if slice_len(&slice1) <= slice_len(&slice2) {
            println!("  fwd won. adding {:?}", slice1);
            result.extend(slice1);
        } else {
            println!("  back won. adding {:?}", slice2);
            result.extend(slice2);
        }
        result.push(first_pt);
    }
}

fn slice_len(pts: &Vec<LonLat>) -> Distance {
    let mut dist = Distance::ZERO;
    for pair in pts.windows(2) {
        dist += pair[0].gps_dist_meters(pair[1]);
    }
    dist
}

// TODO DrawIntersection has find_pts_between, basically a copy
fn find_slice(pts: &Vec<LonLat>, start: LonLat, end: LonLat) -> Vec<LonLat> {
    let mut result = Vec::new();
    for pt in pts {
        if result.is_empty() && *pt == start {
            result.push(*pt);
        } else if !result.is_empty() {
            result.push(*pt);
        }
        // start and end might be the same.
        if !result.is_empty() && *pt == end {
            return result;
        }
    }

    if result.is_empty() {
        panic!("Couldn't find start");
    }

    // Go through again, looking for end
    for pt in pts {
        result.push(*pt);
        if *pt == end {
            return result;
        }
    }
    panic!("Couldn't find end");
}
