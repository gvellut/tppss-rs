use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::types::LatLon;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SunPosition {
    pub azimuth: f64,
    pub elevation: f64,
}

pub fn sunpos<Tz: TimeZone>(when: DateTime<Tz>, location: LatLon, refraction: bool) -> SunPosition {
    let latitude = location.lat;
    let longitude = location.lon;
    let rlat = latitude.to_radians();
    let rlon = longitude.to_radians();
    let when_utc = when.with_timezone(&Utc);
    let dec_utc = when_utc.hour() as f64
        + when_utc.minute() as f64 / 60.0
        + when_utc.second() as f64 / 3600.0;
    let year = when_utc.date_naive().year();
    let month = when_utc.date_naive().month() as i32;
    let day = when_utc.date_naive().day() as i32;
    let daynum = 367 * year - 7 * (year + (month + 9) / 12) / 4 + 275 * month / 9 + day;
    let daynum = daynum as f64 - 730_531.5 + dec_utc / 24.0;
    let mean_long = daynum * 0.017_202_792_39 + 4.894_967_873;
    let mean_anom = daynum * 0.017_201_970_34 + 6.240_040_768;
    let eclip_long = mean_long
        + 0.033_423_055_18 * mean_anom.sin()
        + 0.000_349_065_850_4 * (2.0 * mean_anom).sin();
    let obliquity = 0.409_087_723_4 - 0.000_000_006_981_317_008 * daynum;
    let rasc = (obliquity.cos() * eclip_long.sin()).atan2(eclip_long.cos());
    let decl = (obliquity.sin() * eclip_long.sin()).asin();
    let sidereal = 4.894_961_213 + 6.300_388_099 * daynum + rlon;
    let hour_ang = sidereal - rasc;
    let mut elevation = (decl.sin() * rlat.sin() + decl.cos() * rlat.cos() * hour_ang.cos()).asin();
    let mut azimuth = (-decl.cos() * rlat.cos() * hour_ang.sin())
        .atan2(decl.sin() - rlat.sin() * elevation.sin());
    azimuth = into_range(azimuth.to_degrees(), 0.0, 360.0);
    elevation = into_range(elevation.to_degrees(), -180.0, 180.0);

    if refraction {
        let targ = (elevation + (10.3 / (elevation + 5.11))).to_radians();
        elevation += (1.02 / targ.tan()) / 60.0;
    }

    SunPosition {
        azimuth: round2(azimuth),
        elevation: round2(elevation),
    }
}

fn into_range(x: f64, range_min: f64, range_max: f64) -> f64 {
    let shifted = x - range_min;
    let delta = range_max - range_min;
    shifted.rem_euclid(delta) + range_min
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use chrono_tz::Europe::Paris;

    use super::*;

    #[test]
    fn computes_known_sun_position() {
        let when = Paris.with_ymd_and_hms(2025, 9, 7, 12, 0, 0).unwrap();
        let position = sunpos(when, LatLon::new(45.902351, 6.144737), false);
        assert!(position.azimuth > 100.0 && position.azimuth < 220.0);
        assert!(position.elevation > 40.0);
    }
}
