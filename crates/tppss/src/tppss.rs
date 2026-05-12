use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone};
use chrono_tz::Tz;

use crate::error::{Result, TppssError};
use crate::sunpos::sunpos;
use crate::types::{DaySunriseSunset, Horizon, LatLon, SunriseSunset, SunriseSunsetDetails};

pub fn times_in_day(day: NaiveDate, tz: Tz, precision: usize) -> Result<Vec<DateTime<Tz>>> {
    if precision == 0 {
        return Err(TppssError::InvalidInput(
            "time precision must be greater than zero".into(),
        ));
    }
    let base = tz
        .with_ymd_and_hms(day.year(), day.month(), day.day(), 0, 0, 0)
        .earliest()
        .ok_or_else(|| TppssError::InvalidLocalTime {
            timezone: tz.name().to_owned(),
            time: day.to_string(),
        })?;
    let mut times = Vec::with_capacity(24 * precision + 1);
    let step_seconds = 3600.0 / precision as f64;
    for i in 0..=(24 * precision) {
        let seconds = (i as f64 * step_seconds).round() as i64;
        times.push(base + Duration::seconds(seconds));
    }
    Ok(times)
}

pub fn above_horizon(
    latlon: LatLon,
    times: &[DateTime<Tz>],
    horizon: &Horizon,
) -> (Vec<usize>, Vec<f64>) {
    let mut azimuths = Vec::with_capacity(times.len());
    let mut elevations = Vec::with_capacity(times.len());
    for time in times {
        let position = sunpos(*time, latlon, false);
        azimuths.push(position.azimuth);
        elevations.push(position.elevation);
    }

    let mut centers = Vec::with_capacity(horizon.azimuths.len().saturating_sub(1));
    for i in 0..horizon.azimuths.len().saturating_sub(1) {
        centers.push((horizon.azimuths[i + 1] + horizon.azimuths[i]) / 2.0);
    }

    let mut helevations_by_time = Vec::with_capacity(times.len());
    let mut above = Vec::new();
    for (idx, azimuth) in azimuths.iter().enumerate() {
        let horizon_idx = centers
            .iter()
            .take_while(|center| **center <= *azimuth)
            .count();
        let helevation = horizon.elevations[horizon_idx];
        helevations_by_time.push(helevation);
        if elevations[idx] - helevation > 0.0 {
            above.push(idx);
        }
    }
    (above, helevations_by_time)
}

pub fn sunrise_sunset(
    latlon: LatLon,
    horizon: &Horizon,
    day: NaiveDate,
    tz: Tz,
    precision: usize,
) -> Result<SunriseSunset> {
    let times = times_in_day(day, tz, precision)?;
    let (above_horizon_indices, _) = above_horizon(latlon, &times, horizon);
    Ok(sunrise_sunset_simple(&times, &above_horizon_indices))
}

pub fn sunrise_sunset_details(
    latlon: LatLon,
    horizon: &Horizon,
    day: NaiveDate,
    tz: Tz,
    precision: usize,
) -> Result<SunriseSunsetDetails> {
    let times = times_in_day(day, tz, precision)?;
    let (above_horizon_indices, _) = above_horizon(latlon, &times, horizon);
    Ok(sunrise_sunset_details_from_indices(
        &times,
        &above_horizon_indices,
    ))
}

pub fn sunrise_sunset_year(
    latlon: LatLon,
    horizon: &Horizon,
    year: i32,
    tz: Tz,
    precision: usize,
) -> Result<Vec<DaySunriseSunset>> {
    let mut results = Vec::new();
    let mut day = NaiveDate::from_ymd_opt(year, 1, 1)
        .ok_or_else(|| TppssError::InvalidInput(format!("invalid year: {year}")))?;
    while day.year() == year {
        let result = sunrise_sunset(latlon, horizon, day, tz, precision)?;
        results.push(DaySunriseSunset { day, result });
        day = day
            .succ_opt()
            .ok_or_else(|| TppssError::InvalidInput("date overflow".into()))?;
    }
    Ok(results)
}

fn sunrise_sunset_simple(times: &[DateTime<Tz>], above_horizon_indices: &[usize]) -> SunriseSunset {
    if above_horizon_indices.is_empty() {
        return SunriseSunset::NightAllDay;
    }
    if above_horizon_indices.len() == times.len() {
        return SunriseSunset::LightAllDay;
    }
    SunriseSunset::Times {
        sunrise: times[above_horizon_indices[0]],
        sunset: times[*above_horizon_indices.last().expect("non-empty checked")],
    }
}

fn sunrise_sunset_details_from_indices(
    times: &[DateTime<Tz>],
    above_horizon_indices: &[usize],
) -> SunriseSunsetDetails {
    if above_horizon_indices.is_empty() {
        return SunriseSunsetDetails::NightAllDay;
    }
    if above_horizon_indices.len() == times.len() {
        return SunriseSunsetDetails::LightAllDay;
    }

    let mut transitions = Vec::new();
    for i in 0..above_horizon_indices.len() - 1 {
        if above_horizon_indices[i + 1] - above_horizon_indices[i] > 1 {
            transitions.push(i);
        }
    }

    let mut sunrises = vec![times[above_horizon_indices[0]]];
    for transition in &transitions {
        sunrises.push(times[above_horizon_indices[*transition + 1]]);
    }

    let mut sunsets = Vec::new();
    for transition in &transitions {
        sunsets.push(times[above_horizon_indices[*transition]]);
    }
    sunsets.push(times[*above_horizon_indices.last().expect("non-empty checked")]);

    SunriseSunsetDetails::Times { sunrises, sunsets }
}

#[cfg(test)]
mod tests {
    use chrono_tz::Europe::Paris;

    use super::*;

    #[test]
    fn creates_expected_number_of_times() {
        let day = NaiveDate::from_ymd_opt(2025, 9, 7).unwrap();
        assert_eq!(times_in_day(day, Paris, 60).unwrap().len(), 1441);
    }
}
