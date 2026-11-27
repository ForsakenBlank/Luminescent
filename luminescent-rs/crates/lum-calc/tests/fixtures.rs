#![allow(clippy::unwrap_used)]
//! Every calculator, conversion and time case the Python app recorded (phase 0) must come out the same.

use std::collections::HashMap;

use jiff::civil::{Date, DateTime};
use lum_calc::timeparse::{fmt_secs, parse_clock, parse_duration};
use lum_calc::{
    Base, Book, Error, Num, Scope, convert, date_math, evaluate, fmt, parse_date, pin_name, prepare,
};
use lum_fixtures::{Outcome, parse_python_float, read_json};
use num_bigint::BigInt;
use serde_json::Value;

fn num(v: &Value) -> Num {
    let obj = v.as_object().unwrap_or_else(|| panic!("not an exact number: {v}"));
    if let Some(text) = obj.get("int").and_then(Value::as_str) {
        return Num::Int(text.parse::<BigInt>().unwrap());
    }
    Num::Float(parse_python_float(obj["float"].as_str().unwrap()).unwrap())
}

/// The same answer, to the last digit on Linux (where the fixtures were made). Other systems' maths libraries may
/// differ in the final place for sin, cos and friends.
fn same(a: &Num, b: &Num, what: &str) {
    match (a, b) {
        (Num::Float(x), Num::Float(y)) if cfg!(not(target_os = "linux")) => {
            assert!((x - y).abs() <= 1e-13 * y.abs().max(1.0), "{what}: {x} is not {y}");
        }
        _ => assert_eq!(a, b, "{what}"),
    }
}

fn check(outcome: Result<Num, Error>, case: &Value, what: &str) {
    match (outcome, Outcome::of(case)) {
        (Ok(got), Outcome::Ok(want)) => same(&got, &num(&want), what),
        (Err(e), Outcome::Error { kind, message }) => {
            assert_eq!((e.kind(), e.to_string()), (kind.as_str(), message), "{what}");
        }
        (got, want) => panic!("{what}: got {got:?}, the Python app gave {want:?}"),
    }
}

fn today() -> Date {
    Date::new(2026, 11, 23).unwrap()
}

#[test]
fn the_function_and_constant_lists_match() {
    let cases = read_json("calc_cases.json");
    let mut functions: Vec<&str> = lum_calc::FUNCTIONS.to_vec();
    functions.sort_unstable();
    let want: Vec<&str> =
        cases["functions"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    assert_eq!(functions, want);
    for (name, value) in lum_calc::CONSTANTS {
        same(&Num::Float(*value), &num(&cases["constants"][name]), name);
    }
    let mut reserved: Vec<&str> = want.iter().copied().chain(["pi", "e", "tau", "ans", "mod"]).collect();
    reserved.sort_unstable();
    let listed: Vec<&str> =
        cases["reserved"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    assert_eq!(reserved, listed);
    assert!(listed.iter().all(|name| lum_calc::is_reserved(name)));
    assert_eq!(cases["limits"]["max_pow"], lum_calc::MAX_POW);
    assert_eq!(cases["limits"]["max_fact"], lum_calc::MAX_FACT);
}

#[test]
fn every_sum_gives_what_python_gave() {
    let cases = read_json("calc_cases.json");
    let variables: HashMap<String, Num> =
        cases["variables_for_sums"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), num(v))).collect();
    let sums = cases["sums"].as_array().unwrap();
    assert!(sums.len() > 200);
    for case in sums {
        let text = case["sum"].as_str().unwrap();
        let scope = Scope { variables: variables.clone(), degrees: case["degrees"].as_bool().unwrap() };
        assert_eq!(prepare(text), case["prepared"].as_str().unwrap(), "preparing {text:?}");
        check(evaluate(text, &scope), case, text);
    }
}

#[test]
fn every_result_is_written_the_way_python_wrote_it() {
    let cases = read_json("calc_cases.json");
    for case in cases["fmt"].as_array().unwrap() {
        let base = Base::from_key(case["base"].as_str().unwrap());
        let text = fmt(
            &num(&case["value"]),
            case["decimals"].as_u64().unwrap() as usize,
            case["thousands"].as_bool().unwrap(),
            base,
        );
        assert_eq!(text, case["text"].as_str().unwrap(), "{case}");
    }
    for case in cases["pin_names"].as_array().unwrap() {
        assert_eq!(pin_name(case["name"].as_str().unwrap()), case["var"].as_str().unwrap(), "{case}");
    }
}

fn sample_rates() -> HashMap<String, f64> {
    let saved = read_json("sample_home/data/currency.json");
    let mut rates: HashMap<String, f64> =
        saved["rates"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap())).collect();
    rates.insert("EUR".into(), 1.0);
    rates
}

/// A book whose history holds `history` and whose pins are `pins`, with the results shown in `base`.
fn book_with(folder: &std::path::Path, history: &[(&str, i64)], pins: &[(&str, i64)], base: Base) -> Book {
    let mut book = Book::load(&folder.join("calculator.json"));
    let now = DateTime::constant(2026, 11, 23, 9, 0, 0, 0);
    for (expr, value) in pins {
        book.add_history(expr, Some(&Num::from(*value)), "", now);
        book.pin(book.history().len() - 1, expr).unwrap();
        book.delete_history(book.history().len() - 1);
    }
    for (expr, value) in history {
        book.add_history(expr, Some(&Num::from(*value)), "", now);
    }
    book.set_base(base);
    book
}

#[test]
fn the_panels_compute_matches_with_ans_pins_conversions_and_dates() {
    let cases = read_json("calc_cases.json");
    let rates = sample_rates();
    let dir = tempfile::tempdir().unwrap();
    for case in cases["compute"].as_array().unwrap() {
        let base = Base::from_key(case["base"].as_str().unwrap());
        let book = book_with(dir.path(), &[("6*7", 42)], &[("rent", 11400)], base);
        let text = case["text"].as_str().unwrap();
        let got = book.compute(text, &rates, today());
        match (got, Outcome::of(case)) {
            (Ok(got), Outcome::Ok(want)) => {
                assert_eq!(got.result, want["result"].as_str().unwrap(), "{text}");
                assert_eq!(got.expr, want["expr"].as_str().unwrap(), "{text}");
                match (&got.value, &want["value"]) {
                    (None, Value::Null) => {}
                    (Some(v), w) => same(v, &num(w), text),
                    (v, w) => panic!("{text}: {v:?} against {w}"),
                }
            }
            (Err(e), Outcome::Error { kind, message }) => {
                assert_eq!((e.kind(), e.to_string()), (kind.as_str(), message), "{text}");
            }
            (got, want) => panic!("{text}: got {got:?}, Python gave {want:?}"),
        }
    }
    for case in cases["plain"].as_array().unwrap() {
        let book = book_with(dir.path(), &[], &[], Base::from_key(case["base"].as_str().unwrap()));
        let mut entry = serde_json::Map::new();
        entry.insert("result".into(), case["entry"]["result"].clone());
        entry.insert(
            "value".into(),
            case["entry"]["value"]
                .as_object()
                .map_or(Value::Null, |_| num(&case["entry"]["value"]).to_json()),
        );
        let plain = book.plain(&lum_calc::Entry(entry));
        assert_eq!(plain, case["plain"].as_str().unwrap(), "{case}");
    }
}

#[test]
fn conversions_give_python_answers_with_and_without_rates() {
    let cases = read_json("convert_cases.json");
    let rates = sample_rates();
    let none = HashMap::new();
    let scope = Scope { variables: HashMap::from([("rent".to_owned(), Num::from(1000))]), degrees: true };
    let amount = |e: &str| evaluate(e, &scope);
    for case in cases["conversions"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let with = if case["rates"].as_bool().unwrap() { &rates } else { &none };
        match (convert(text, &amount, with), Outcome::of(case)) {
            (Ok(Some((value, unit))), Outcome::Ok(want)) => {
                let want = want.as_array().unwrap();
                same(&value, &num(&want[0]), text);
                assert_eq!(unit, want[1].as_str().unwrap(), "{text}");
            }
            (Ok(None), Outcome::Ok(Value::Null)) => {}
            (Err(e), Outcome::Error { kind, message }) => {
                assert_eq!((e.kind(), e.to_string()), (kind.as_str(), message), "{text}");
            }
            (got, want) => panic!("{text}: got {got:?}, Python gave {want:?}"),
        }
    }
}

#[test]
fn the_unit_table_is_the_same() {
    let cases = read_json("convert_cases.json");
    let units = cases["units"].as_object().unwrap();
    assert_eq!(units.len(), lum_calc::unit_count());
    for (alias, entry) in units {
        let (dimension, factor) = lum_calc::unit(alias).unwrap_or_else(|| panic!("no unit {alias}"));
        assert_eq!(dimension, entry[0].as_str().unwrap(), "{alias}");
        let want = match num(&entry[1]) {
            Num::Int(i) => i.to_string().parse::<f64>().unwrap(),
            Num::Float(f) => f,
        };
        assert_eq!(factor, want, "{alias}");
    }
}

#[test]
fn date_sums_and_dates_match() {
    let cases = read_json("convert_cases.json");
    for case in cases["date_math"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        match (date_math(text, today()), Outcome::of(case)) {
            (Ok(Some((days, result))), Outcome::Ok(want)) => {
                let want = want.as_array().unwrap();
                match (days, &want[0]) {
                    (None, Value::Null) => {}
                    (Some(n), w) => assert_eq!(Num::from(n), num(w), "{text}"),
                    (n, w) => panic!("{text}: {n:?} against {w}"),
                }
                assert_eq!(result, want[1].as_str().unwrap(), "{text}");
            }
            (Ok(None), Outcome::Ok(Value::Null)) => {}
            (Err(e), Outcome::Error { kind, message }) => {
                assert_eq!((e.kind(), e.to_string()), (kind.as_str(), message), "{text}");
            }
            (got, want) => panic!("{text}: got {got:?}, Python gave {want:?}"),
        }
    }
    for case in cases["parse_date"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        match (parse_date(text, today()), Outcome::of(case)) {
            (Ok(d), Outcome::Ok(want)) => assert_eq!(d.to_string(), want.as_str().unwrap(), "{text}"),
            (Err(e), Outcome::Error { message, .. }) => assert_eq!(e.to_string(), message, "{text}"),
            (got, want) => panic!("{text}: got {got:?}, Python gave {want:?}"),
        }
    }
}

#[test]
fn typed_times_are_read_the_same() {
    let cases = read_json("timeparse_cases.json");
    for case in cases["durations"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let got = parse_duration(text, case["bare_unit"].as_u64().unwrap());
        assert_eq!(got, case["seconds"].as_f64(), "{text:?} counting in {}", case["bare_unit"]);
    }
    for case in cases["clocks"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let now: DateTime = case["now"].as_str().unwrap().parse().unwrap();
        let want = case["due"].as_str().map(|d| d.parse::<DateTime>().unwrap());
        assert_eq!(parse_clock(text, now), want, "{text:?} at {now}");
    }
    for case in cases["fmt_secs"].as_array().unwrap() {
        assert_eq!(fmt_secs(case["seconds"].as_f64().unwrap()), case["text"].as_str().unwrap(), "{case}");
    }
}

#[test]
fn the_sample_calculator_file_is_saved_back_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calculator.json");
    let original = lum_fixtures::read_text("sample_home/data/calculator.json");
    std::fs::write(&path, &original).unwrap();
    let mut book = Book::load(&path);
    assert_eq!(book.history().len(), 3);
    assert_eq!(book.pins()[0].colour(), "#30A46C");
    book.save();
    book.flush();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}
