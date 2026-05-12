use std::path::Path;

use chrono::{NaiveDate, TimeZone};
use chrono_tz::Europe::Paris;
use tppss::{DemReader, KM, LatLon, SunriseSunset, compute_horizon, sunrise_sunset};

const DEFAULT_DEM: &str = "/Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif";

fn fixture_dem() -> Option<String> {
    std::env::var("TPPSS_TEST_DEM").ok().or_else(|| {
        Path::new(DEFAULT_DEM)
            .exists()
            .then(|| DEFAULT_DEM.to_owned())
    })
}

#[tokio::test]
async fn reference_day_is_dark_all_day() -> anyhow::Result<()> {
    let Some(dem_path) = fixture_dem() else {
        eprintln!("skipping DEM fixture test; set TPPSS_TEST_DEM to enable it");
        return Ok(());
    };

    let latlon = LatLon::new(46.010148, 6.112227);
    let dem = DemReader::open(dem_path).await?;
    let horizon = compute_horizon(latlon, &dem, 25.0 * KM, 1, 5.0).await?;
    let day = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();

    assert_eq!(
        sunrise_sunset(latlon, &horizon, day, Paris, 60)?,
        SunriseSunset::NightAllDay
    );
    Ok(())
}

#[tokio::test]
async fn reference_day_matches_python_sunrise_and_sunset() -> anyhow::Result<()> {
    let Some(dem_path) = fixture_dem() else {
        eprintln!("skipping DEM fixture test; set TPPSS_TEST_DEM to enable it");
        return Ok(());
    };

    let latlon = LatLon::new(45.902351, 6.144737);
    let dem = DemReader::open(dem_path).await?;
    let horizon = compute_horizon(latlon, &dem, 25.0 * KM, 1, 30.0).await?;
    let day = NaiveDate::from_ymd_opt(2025, 9, 7).unwrap();

    assert_eq!(
        sunrise_sunset(latlon, &horizon, day, Paris, 60)?,
        SunriseSunset::Times {
            sunrise: Paris.with_ymd_and_hms(2025, 9, 7, 8, 32, 0).unwrap(),
            sunset: Paris.with_ymd_and_hms(2025, 9, 7, 19, 51, 0).unwrap(),
        }
    );
    Ok(())
}
