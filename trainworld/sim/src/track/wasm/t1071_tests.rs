use super::*;

/// A built line between two stations 6 km apart with two trains at every level, its id.
fn line_api() -> (TrackApi, u32) {
    let mut a = TrackApi::new(-73.985, 40.758, true);
    a.set_cash(1e9);
    let ends = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 6000.0, 0.0, 0.0];
    assert_eq!(a.add_route(&ends, &[], 2, false), 0, "{}", a.issues());
    let node = |t: &TrackApi, x: f64| t.nodes_info().chunks(10).find(|c| (c[1] - x).abs() < 1e-6).unwrap()[0] as u32;
    let (w, e) = (node(&a, 0.0), node(&a, 6000.0));
    for (n, x) in [(w, 0.0), (e, 6000.0)] {
        assert_eq!(a.add_station(1, n, x, 0.0, 200, "S".into()), 0, "{}", a.issues());
    }
    let edges: Vec<u32> = (0..a.w.net.edge_count() as u32).filter(|&k| a.w.net.edge_ok(k)).collect();
    assert_eq!(a.construct(&edges, &[w, e]), 0, "{}", a.issues());
    let id = a.new_line_id();
    assert_eq!(a.set_line(id, &[w, e], 6.0, 4.0, 2.0, 30.0, 180.0, 0, 0, "L".into()), 0, "{}", a.issues());
    assert_eq!(a.set_train_counts(id, 2, 2, 2), 0, "{}", a.issues());
    (a, id)
}

/// T-1071: an edit with a lead takes effect that much later: the trips drawn before then are
/// the old service's to the bit, and a save made before the change is over loads with it.
#[test]
fn a_count_change_with_a_lead_takes_effect_then_and_saves_with_its_eras() {
    let (mut a, id) = line_api();
    let now: f64 = 2.0 * 86400.0 + 11.0 * 3600.0 + 417.0;
    let epoch = (now / 3600.0).floor() * 3600.0;
    a.set_now(now);
    let old = a.trips(epoch);
    a.set_edit_lead(300.0);
    assert_eq!(a.set_train_counts(id, 3, 2, 2), 0, "{}", a.issues());
    assert_eq!(a.edit_time(), now + 300.0);
    // the lead is used once
    a.set_now(now + 1.0);
    assert_eq!(a.set_train_counts(id, 3, 2, 2), 0);
    assert_eq!(a.edit_time(), now + 1.0);
    let new = a.trips(epoch);
    // trips that left before the change are the same ones (draw, as the fourth number may
    // name a later departure), and none of the new pattern leaves before it
    let left = |v: &[f32]| v.chunks(4).filter(|t| (t[1] as f64) + epoch < now + 300.0 - 1.0).map(|t| (t[0], t[1])).collect::<Vec<_>>();
    assert_eq!(left(&old), left(&new));
    assert_ne!(old, new, "the change shows after the lead");
    // a save made now loads with the change still going on
    let bytes = a.save();
    assert_eq!(&bytes[bytes.len() - 4..], ERAS_TAG);
    let mut b = TrackApi::new(-73.985, 40.758, true);
    assert!(b.load(&bytes));
    b.set_now(now + 1.0);
    assert_eq!(b.w.svc.eras, a.w.svc.eras);
    let (x, y) = (a.trips(epoch), b.trips(epoch));
    assert_eq!(x.len(), y.len());
    assert!(x.iter().zip(&y).all(|(p, q)| (p - q).abs() < 0.01 || (p.is_nan() && q.is_nan())));
    // and a save with no change going on ends as before
    b.set_now(now + 6.0 * 3600.0);
    assert!(b.w.svc.eras.iter().all(|e| e.is_empty()));
    assert_eq!(b.save(), save::encode(&b.w.net));
}

/// A weekday count change leaves the weekend and holiday counts as they ran: a line still running
/// its weekday trains every day keeps the old ones on those days (`pin_rest`).
#[test]
fn a_weekday_change_leaves_the_weekend_and_holidays() {
    let (mut a, id) = line_api();
    assert!(a.line_rest(id).is_empty(), "a new line runs its weekday trains every day");
    assert_eq!(a.set_train_counts(id, 4, 2, 2), 0, "{}", a.issues());
    assert_eq!(a.line_rest(id), vec![2, 2, 2, 2, 2, 2], "the weekend and holidays keep 2");
    assert_eq!(a.set_line_rest(id, &[1, 1, 0, 2, 1, 1]), 0, "{}", a.issues());
    assert_eq!(a.set_train_counts(id, 6, 3, 2), 0, "{}", a.issues());
    assert_eq!(a.line_rest(id), vec![1, 1, 0, 2, 1, 1], "counts of its own are left as set");
}

/// The fleet is the most trains a line runs on any kind of day at any level, so more weekend trains
/// are bought.
#[test]
fn more_weekend_trains_are_bought() {
    let (mut a, id) = line_api();
    let cars = a.cars_needed();
    assert_eq!(a.set_line_rest(id, &[6, 2, 2, 2, 2, 2]), 0, "{}", a.issues());
    assert_eq!(a.cars_needed(), cars * 3, "six trains on a weekend, two on weekdays");
    assert_eq!(a.set_line_rest(id, &[1, 1, 0, 0, 0, 0]), 0, "{}", a.issues());
    assert_eq!(a.cars_needed(), cars, "fewer at the weekend: the weekdays' fleet");
}

/// Trips over six of local day `day`'s busiest hours, as `trips` gives them, as bits (a trip that
/// leaves service has NaN for its next departure; the line's local time is New York's, a few hours
/// behind game time).
fn day_trips(a: &TrackApi, day: f64) -> Vec<u32> {
    (0..6).flat_map(|h| a.trips(day * 86400.0 + (14 + h) as f64 * 3600.0)).map(f32::to_bits).collect()
}

fn saturday_trips(a: &TrackApi) -> Vec<u32> {
    day_trips(a, 6.0)
}

/// Trips in a list from `trips` (four numbers each).
fn count(trips: &[u32]) -> usize {
    trips.len() / 4
}

/// Undo and redo of weekend counts move the weekend's rates and trips back and forth to the bit:
/// the counts ride on a whole-line edit, whose inverse is the line as it was.
#[test]
fn weekend_counts_undo_and_redo_to_the_bit() {
    let (mut a, id) = line_api();
    let (rest0, sat0) = (a.line_rest(id), saturday_trips(&a));
    assert_eq!(a.set_line_rest(id, &[6, 2, 2, 2, 2, 2]), 0, "{}", a.issues());
    let (rates, sat) = (a.line_tph_rest(id), saturday_trips(&a));
    assert!(count(&sat) > count(&sat0), "{} trips with six weekend trains, {} with two", count(&sat), count(&sat0));
    assert_eq!(a.undo(), 0);
    assert_eq!(a.line_rest(id), rest0);
    assert_eq!(a.w.net.rest_solved, [false, false], "every day back on the weekdays'");
    assert_eq!(saturday_trips(&a), sat0);
    assert_eq!(a.redo(), 0);
    assert_eq!(a.line_rest(id), vec![6, 2, 2, 2, 2, 2]);
    assert_eq!(a.line_tph_rest(id), rates);
    assert_eq!(saturday_trips(&a), sat);
}

/// A save keeps the weekend and holiday counts and their settled rates, so a reload runs the same
/// trips on a Saturday and on a holiday from the line's calendar (which is not saved: the clock
/// worker sends it again).
#[test]
fn weekend_and_holiday_trips_load_from_a_save_as_they_ran() {
    let (mut a, id) = line_api();
    assert_eq!(a.set_line_rest(id, &[6, 2, 2, 4, 2, 0]), 0, "{}", a.issues());
    // local Tuesday (day 2) a holiday
    let rows = [id as f64, 3.0, 1.0, 0.0, 2.0, 2.0, 3.0, 0.0];
    a.set_line_rows(rows.to_vec());
    let tue = |a: &TrackApi| day_trips(a, 2.0);
    let (sat, hol) = (saturday_trips(&a), tue(&a));
    let mut b = TrackApi::new(-73.985, 40.758, true);
    assert!(b.load(&a.save()));
    b.set_line_rows(rows.to_vec());
    assert_eq!(b.w.net.rest_solved, a.w.net.rest_solved);
    assert_eq!(b.line_tph_rest(id), a.line_tph_rest(id));
    assert_eq!(saturday_trips(&b), sat, "Saturday");
    assert_eq!(tue(&b), hol, "the holiday");
}

/// A line's calendar (`set_line_rows`) makes a weekday a holiday: that day runs the holiday trains,
/// and taking the holiday off puts the weekday's back.
#[test]
fn a_holiday_on_the_lines_calendar_runs_the_holiday_trains() {
    let (mut a, id) = line_api();
    assert_eq!(a.set_line_rest(id, &[2, 2, 2, 8, 2, 2]), 0, "{}", a.issues());
    let tue = |a: &TrackApi| day_trips(a, 2.0);
    let week = tue(&a);
    a.set_line_rows(vec![id as f64, 3.0, 1.0, 0.0, 2.0, 2.0, 3.0, 0.0]);
    let hol = tue(&a);
    assert!(count(&hol) > count(&week), "{} trips on the holiday, {} on a Tuesday", count(&hol), count(&week));
    a.set_line_rows(vec![]);
    assert_eq!(tue(&a), week);
}

/// A weekday count lowered below the weekend's frees no trains: the weekend still runs them.
#[test]
fn a_weekday_cut_below_the_weekend_keeps_the_fleet() {
    let (mut a, id) = line_api();
    let two = a.cars_needed();
    assert_eq!(a.set_line_rest(id, &[6, 2, 2, 2, 2, 2]), 0, "{}", a.issues());
    let six = a.cars_needed();
    assert_eq!(six, two * 3);
    assert_eq!(a.set_train_counts(id, 1, 1, 1), 0, "{}", a.issues());
    assert_eq!(a.cars_needed(), six, "the weekend's six trains");
    assert_eq!(a.set_line_rest(id, &[1, 1, 1, 1, 1, 1]), 0, "{}", a.issues());
    assert_eq!(a.cars_needed(), two / 2, "one train every day");
}

/// Weekend and holiday counts equal to the weekdays' cost no settle of their own, and give every
/// day the weekday pattern (`rest_solved` off).
#[test]
fn weekend_counts_equal_to_the_weekdays_settle_nothing_more() {
    let (mut a, id) = line_api();
    let sat = saturday_trips(&a);
    assert_eq!(a.set_line_rest(id, &[2, 2, 2, 2, 2, 2]), 0, "{}", a.issues());
    assert_eq!(a.w.net.line_rest[id as usize], Some([[2, 2, 2]; 2]));
    assert_eq!(a.w.net.rest_solved, [false, false]);
    assert!(!a.w.svc.lattice(&a.w.net, id as usize).unwrap().rest_on);
    assert_eq!(saturday_trips(&a), sat);
}

/// Removing a line takes its weekend and holiday counts with it, and undo brings them back.
#[test]
fn a_removed_line_takes_its_weekend_counts() {
    let (mut a, id) = line_api();
    assert_eq!(a.set_line_rest(id, &[6, 2, 2, 4, 2, 2]), 0, "{}", a.issues());
    assert_eq!(a.remove_line(id), 0, "{}", a.issues());
    assert_eq!(a.w.net.line_rest[id as usize], None);
    assert!(a.line_rest(id).is_empty());
    assert_eq!(a.w.net.rest_solved, [false, false], "no line left with counts of its own");
    assert_eq!(a.undo(), 0);
    assert_eq!(a.line_rest(id), vec![6, 2, 2, 4, 2, 2]);
    assert_eq!(a.w.net.rest_solved, [true, true]);
}
