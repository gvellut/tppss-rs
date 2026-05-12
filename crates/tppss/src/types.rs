use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;
use ndarray::Array1;
use serde::{Deserialize, Serialize};

pub const KM: f64 = 1000.0;
pub const MILES: f64 = 1609.34;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

#[derive(Debug, Clone)]
pub struct Horizon {
    pub elevations: Array1<f64>,
    pub zeniths: Array1<f64>,
    pub azimuths: Array1<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SunriseSunset {
    Times {
        sunrise: DateTime<Tz>,
        sunset: DateTime<Tz>,
    },
    LightAllDay,
    NightAllDay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SunriseSunsetDetails {
    Times {
        sunrises: Vec<DateTime<Tz>>,
        sunsets: Vec<DateTime<Tz>>,
    },
    LightAllDay,
    NightAllDay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaySunriseSunset {
    pub day: NaiveDate,
    pub result: SunriseSunset,
}
