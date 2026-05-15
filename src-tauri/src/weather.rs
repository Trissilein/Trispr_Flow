//! Weather query helpers and live forecast lookups.
//!
//! Pure heuristics over the user's command text (English/German bilingual),
//! plus a thin HTTP client that calls Open-Meteo's free geocoding and
//! forecast APIs. No app state, no Tauri handles — extracted from `lib.rs`
//! as part of the Phase 1 refactoring (QW4) so the assistant rule-engine
//! glue stays small.

use std::time::Duration;

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenMeteoGeocodingResponse {
    results: Option<Vec<OpenMeteoGeocodingEntry>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenMeteoGeocodingEntry {
    name: String,
    country: Option<String>,
    admin1: Option<String>,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenMeteoForecastResponse {
    daily: Option<OpenMeteoDailyData>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenMeteoDailyData {
    time: Vec<String>,
    weather_code: Option<Vec<i64>>,
    temperature_2m_max: Option<Vec<f64>>,
    temperature_2m_min: Option<Vec<f64>>,
    precipitation_probability_max: Option<Vec<f64>>,
}

pub(crate) fn weather_query_english_hint(normalized: &str) -> bool {
    normalized.contains("please")
        || normalized.contains("what")
        || normalized.contains("session")
        || normalized.contains("status")
        || normalized.contains("weather")
        || normalized.contains("tomorrow")
}

pub(crate) fn weather_query_like(normalized: &str) -> bool {
    normalized.contains("weather")
        || normalized.contains("wetter")
        || normalized.contains("forecast")
        || normalized.contains("temperatur")
}

fn weather_query_day_offset(normalized: &str) -> usize {
    if normalized.contains("übermorgen") || normalized.contains("day after tomorrow") {
        2
    } else if normalized.contains("morgen") || normalized.contains("tomorrow") {
        1
    } else {
        0
    }
}

fn is_weather_location_stopword(token: &str) -> bool {
    matches!(
        token,
        "heute"
            | "today"
            | "morgen"
            | "tomorrow"
            | "übermorgen"
            | "after"
            | "day"
            | "wetter"
            | "weather"
            | "forecast"
            | "temperatur"
            | "temperature"
            | "bitte"
            | "please"
            | "wie"
            | "wird"
            | "ist"
            | "das"
            | "the"
            | "for"
            | "in"
    )
}

fn extract_weather_location_hint(command_text: &str) -> Option<String> {
    let tokens: Vec<String> = command_text
        .split_whitespace()
        .map(|raw| {
            raw.trim_matches(|ch: char| {
                matches!(
                    ch,
                    ',' | '.' | '!' | '?' | ';' | ':' | '"' | '\'' | '(' | ')' | '[' | ']'
                )
            })
            .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.len() < 2 {
        return None;
    }

    for idx in 0..tokens.len().saturating_sub(1) {
        let marker = tokens[idx].to_lowercase();
        if marker != "in" && marker != "für" && marker != "for" {
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        for token in tokens.iter().skip(idx + 1).take(3) {
            let lower = token.to_lowercase();
            if is_weather_location_stopword(lower.as_str()) {
                break;
            }
            parts.push(token.to_string());
        }
        if !parts.is_empty() {
            return Some(parts.join(" "));
        }
    }
    None
}

fn weather_code_label(code: i64, english: bool) -> &'static str {
    match code {
        0 => {
            if english {
                "clear sky"
            } else {
                "klar"
            }
        }
        1 => {
            if english {
                "mainly clear"
            } else {
                "überwiegend klar"
            }
        }
        2 => {
            if english {
                "partly cloudy"
            } else {
                "leicht bewölkt"
            }
        }
        3 => {
            if english {
                "overcast"
            } else {
                "bedeckt"
            }
        }
        45 | 48 => {
            if english {
                "fog"
            } else {
                "Nebel"
            }
        }
        51 | 53 | 55 | 56 | 57 => {
            if english {
                "drizzle"
            } else {
                "Nieselregen"
            }
        }
        61 | 63 | 65 | 66 | 67 => {
            if english {
                "rain"
            } else {
                "Regen"
            }
        }
        71 | 73 | 75 | 77 => {
            if english {
                "snow"
            } else {
                "Schnee"
            }
        }
        80 | 81 | 82 => {
            if english {
                "rain showers"
            } else {
                "Regenschauer"
            }
        }
        85 | 86 => {
            if english {
                "snow showers"
            } else {
                "Schneeschauer"
            }
        }
        95 | 96 | 99 => {
            if english {
                "thunderstorm"
            } else {
                "Gewitter"
            }
        }
        _ => {
            if english {
                "mixed conditions"
            } else {
                "gemischte Bedingungen"
            }
        }
    }
}

fn encode_url_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
}

pub(crate) fn online_weather_unavailable_reply(command_text: &str) -> String {
    let normalized = command_text.to_lowercase();
    let english_hint = weather_query_english_hint(&normalized);
    if english_hint {
        return "Online weather lookup is currently unavailable. Please try again or include a city (for example: weather in Berlin tomorrow).".to_string();
    }
    "Live-Wetterabfrage ist aktuell nicht verfügbar. Bitte erneut versuchen oder eine Stadt mit angeben (z. B. Wetter in Berlin morgen).".to_string()
}

pub(crate) fn fetch_live_weather_reply(command_text: &str) -> Result<String, String> {
    let normalized = command_text.to_lowercase();
    let english_hint = weather_query_english_hint(&normalized);
    let day_offset = weather_query_day_offset(&normalized);

    let explicit_location = extract_weather_location_hint(command_text);
    let using_default_location = explicit_location.is_none();
    let location_query = explicit_location.unwrap_or_else(|| "Berlin".to_string());
    let geocode_language = if english_hint { "en" } else { "de" };

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(6))
        .timeout_write(Duration::from_secs(6))
        .build();

    let geocode_url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={}&format=json",
        encode_url_component(location_query.trim()),
        geocode_language
    );

    let geocode_response: OpenMeteoGeocodingResponse = agent
        .get(&geocode_url)
        .call()
        .map_err(|err| format!("weather geocoding request failed: {err}"))?
        .into_json()
        .map_err(|err| format!("weather geocoding parse failed: {err}"))?;

    let location = geocode_response
        .results
        .and_then(|mut entries| entries.drain(..).next())
        .ok_or_else(|| {
            format!(
                "weather geocoding returned no result for '{}'",
                location_query
            )
        })?;

    let forecast_days = std::cmp::max(3, day_offset + 1);
    let forecast_url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.5}&longitude={:.5}&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days={}",
        location.latitude,
        location.longitude,
        forecast_days
    );

    let forecast_response: OpenMeteoForecastResponse = agent
        .get(&forecast_url)
        .call()
        .map_err(|err| format!("weather forecast request failed: {err}"))?
        .into_json()
        .map_err(|err| format!("weather forecast parse failed: {err}"))?;

    let daily = forecast_response
        .daily
        .ok_or_else(|| "weather forecast daily block missing".to_string())?;
    let weather_codes = daily
        .weather_code
        .ok_or_else(|| "weather codes missing in forecast response".to_string())?;
    let max_temps = daily
        .temperature_2m_max
        .ok_or_else(|| "max temperatures missing in forecast response".to_string())?;
    let min_temps = daily
        .temperature_2m_min
        .ok_or_else(|| "min temperatures missing in forecast response".to_string())?;
    if daily.time.is_empty()
        || weather_codes.is_empty()
        || max_temps.is_empty()
        || min_temps.is_empty()
    {
        return Err("weather forecast response is empty".to_string());
    }

    let idx = day_offset.min(daily.time.len().saturating_sub(1));
    let weather_code = *weather_codes
        .get(idx)
        .ok_or_else(|| "weather code missing for requested day".to_string())?;
    let temp_max = *max_temps
        .get(idx)
        .ok_or_else(|| "max temperature missing for requested day".to_string())?;
    let temp_min = *min_temps
        .get(idx)
        .ok_or_else(|| "min temperature missing for requested day".to_string())?;
    let precip_prob = daily
        .precipitation_probability_max
        .as_ref()
        .and_then(|values| values.get(idx))
        .copied()
        .unwrap_or(0.0);

    let location_display = match (&location.admin1, &location.country) {
        (Some(region), Some(country))
            if !region.trim().is_empty() && !country.trim().is_empty() =>
        {
            format!("{}, {}, {}", location.name, region, country)
        }
        (_, Some(country)) if !country.trim().is_empty() => {
            format!("{}, {}", location.name, country)
        }
        _ => location.name.clone(),
    };

    let day_label = match day_offset {
        0 => {
            if english_hint {
                "today".to_string()
            } else {
                "heute".to_string()
            }
        }
        1 => {
            if english_hint {
                "tomorrow".to_string()
            } else {
                "morgen".to_string()
            }
        }
        2 => {
            if english_hint {
                "the day after tomorrow".to_string()
            } else {
                "übermorgen".to_string()
            }
        }
        n => {
            if english_hint {
                format!("in {n} days")
            } else {
                format!("in {n} Tagen")
            }
        }
    };

    let summary = weather_code_label(weather_code, english_hint);
    let temp_max_round = temp_max.round() as i32;
    let temp_min_round = temp_min.round() as i32;
    let precip_round = precip_prob.round() as i32;

    if english_hint {
        let prefix = if using_default_location {
            "Live weather (default location Berlin)"
        } else {
            "Live weather"
        };
        return Ok(format!(
            "{prefix} for {location_display} {day_label}: {summary}, {temp_min_round}°C to {temp_max_round}°C, precipitation up to {precip_round}%. Source: Open-Meteo."
        ));
    }

    let prefix = if using_default_location {
        "Live-Wetter (Standardort Berlin)"
    } else {
        "Live-Wetter"
    };
    Ok(format!(
        "{prefix} für {location_display} {day_label}: {summary}, {temp_min_round}°C bis {temp_max_round}°C, Niederschlag bis {precip_round}%. Quelle: Open-Meteo."
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        extract_weather_location_hint, online_weather_unavailable_reply, weather_query_day_offset,
        weather_query_like,
    };

    #[test]
    fn weather_query_keywords_cover_de_and_en() {
        assert!(weather_query_like("wie wird das wetter morgen?"));
        assert!(weather_query_like("weather forecast tomorrow"));
        assert!(!weather_query_like("build me a gdd recap"));
    }

    #[test]
    fn weather_day_offset_detects_relative_terms() {
        assert_eq!(weather_query_day_offset("weather tomorrow"), 1);
        assert_eq!(weather_query_day_offset("wetter übermorgen"), 2);
        assert_eq!(weather_query_day_offset("weather today"), 0);
    }

    #[test]
    fn weather_location_hint_extracts_city_after_markers() {
        assert_eq!(
            extract_weather_location_hint("Wie wird das Wetter morgen in Berlin?"),
            Some("Berlin".to_string())
        );
        assert_eq!(
            extract_weather_location_hint("weather forecast for New York tomorrow"),
            Some("New York".to_string())
        );
    }

    #[test]
    fn weather_unavailable_reply_is_localized() {
        let de = online_weather_unavailable_reply("wie wird das wetter?");
        assert!(de.contains("Live-Wetterabfrage"));

        let en = online_weather_unavailable_reply("weather tomorrow please");
        assert!(en.contains("Online weather lookup"));
    }
}
