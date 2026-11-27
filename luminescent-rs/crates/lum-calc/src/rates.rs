//! Currency rates: euros-to-X from the European Central Bank by way of frankfurter, fetched at most once a day
//! into `currency.json`.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use lum_core::pyjson::Style;
use lum_core::store;
use serde_json::{Value, json};

const URLS: [&str; 2] =
    ["https://api.frankfurter.dev/v1/latest?base=EUR", "https://api.frankfurter.app/latest?from=EUR"];
const DAY: Duration = Duration::from_secs(86_400);

/// `{CODE: units per euro}`, empty until the first fetch has happened.
#[derive(Debug, Clone, Default)]
pub struct Rates {
    map: HashMap<String, f64>,
    date: String,
}

impl Rates {
    pub fn load(path: &Path) -> Rates {
        let mut rates = Rates::default();
        if let Some(Value::Object(saved)) = store::load_json(path) {
            rates.date = saved.get("date").and_then(Value::as_str).unwrap_or_default().to_owned();
            if let Some(Value::Object(list)) = saved.get("rates") {
                rates.map = rates_of(list);
            }
        }
        rates
    }

    pub fn map(&self) -> &HashMap<String, f64> {
        &self.map
    }

    /// The day the rates are from, for "Update currency rates (from 2026-11-23)".
    pub fn date(&self) -> &str {
        &self.date
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

fn rates_of(list: &serde_json::Map<String, Value>) -> HashMap<String, f64> {
    let mut map: HashMap<String, f64> =
        list.iter().filter_map(|(k, v)| Some((k.to_uppercase(), v.as_f64()?))).collect();
    map.insert("EUR".to_owned(), 1.0);
    map
}

/// Brings the rates up to date, at most once a day unless `force`. Blocks while it asks the network, so call it
/// from a worker thread. Gives back the rates now on disk, or `None` when nothing could be read or fetched.
pub fn refresh_rates(path: &Path, force: bool) -> Option<Rates> {
    if !force && saved_today(path) {
        return Some(Rates::load(path)).filter(|r| !r.is_empty());
    }
    for url in URLS {
        match fetch(url) {
            Ok(body) => {
                if let Some(rates) = accept(path, &body) {
                    tracing::info!("currency rates for {}", rates.date);
                    return Some(rates);
                }
            }
            Err(e) => tracing::info!("currency rates from {url} failed: {e}"),
        }
    }
    Some(Rates::load(path)).filter(|r| !r.is_empty())
}

fn saved_today(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified());
    modified.ok().and_then(|at| SystemTime::now().duration_since(at).ok()).is_some_and(|age| age < DAY)
}

fn fetch(url: &str) -> Result<String, ureq::Error> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    agent.get(url).header("User-Agent", "Luminescent").call()?.body_mut().read_to_string()
}

/// Reads an answer from the API and, when it holds rates, saves it the way the Python app does.
fn accept(path: &Path, body: &str) -> Option<Rates> {
    let answer: Value = serde_json::from_str(body).ok()?;
    let list = answer.get("rates")?.as_object().filter(|l| !l.is_empty())?;
    let date = answer.get("date").and_then(Value::as_str).unwrap_or_default().to_owned();
    let saved = json!({"date": date, "base": "EUR", "rates": list});
    if let Err(e) = store::save_json(path, &saved, Style::indented(1)) {
        tracing::warn!("could not save the currency rates: {e}");
    }
    Some(Rates { map: rates_of(list), date })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_with_rates_is_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("currency.json");
        let body = r#"{"amount":1.0,"base":"EUR","date":"2026-11-23","rates":{"usd":1.08,"GBP":0.85}}"#;
        let rates = accept(&path, body).unwrap();
        assert_eq!(rates.map()["USD"], 1.08);
        assert_eq!(rates.map()["EUR"], 1.0);
        let again = Rates::load(&path);
        assert_eq!(again.date(), "2026-11-23");
        assert_eq!(again.map()["GBP"], 0.85);
        assert!(accept(&path, r#"{"rates":{}}"#).is_none());
        assert!(accept(&path, "nonsense").is_none());
    }
}
