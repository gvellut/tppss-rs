use ndarray::{Array1, Array2};

use crate::dem::{AffineTransform, DemReader, Ellipsoid, StudyArea};
use crate::error::Result;
use crate::types::{Horizon, LatLon};

pub async fn compute_horizon(
    latlon: LatLon,
    dem: &DemReader,
    distance_m: f64,
    precision: usize,
    observer_height_m: f64,
) -> Result<Horizon> {
    let study_area = dem.extract_study_area(latlon, distance_m).await?;
    Ok(compute_horizon_from_study_area(
        latlon,
        study_area,
        precision,
        observer_height_m,
    ))
}

pub fn compute_horizon_from_study_area(
    latlon: LatLon,
    study_area: StudyArea,
    precision: usize,
    observer_height_m: f64,
) -> Horizon {
    let (lats, lons) = pixel_positions(&study_area.data, study_area.transform);
    let (lat_res, lon_res) = study_area.transform.resolution();
    let corner_lats = lats.mapv(|v| v - lat_res / 2.0);
    let corner_lons = lons.mapv(|v| v + lon_res / 2.0);
    let azimuth = azimuth(latlon, &corner_lats, &corner_lons, study_area.ellipsoid);
    let z_obs =
        study_area.data[(study_area.observer_row, study_area.observer_col)] + observer_height_m;
    let elevation = elevation_angle(
        z_obs,
        &study_area.data,
        latlon,
        &lats,
        &lons,
        study_area.ellipsoid,
    );
    let mut helevations = compute_mask(
        study_area.observer_col,
        study_area.observer_row,
        &study_area.data,
        precision,
        &azimuth,
        &elevation,
    );
    helevations.mapv_inplace(|v| v.max(0.0));
    let zeniths = helevations.mapv(|v| 90.0 - v);
    let azimuths = linspace(0.0, 360.0, helevations.len());

    Horizon {
        elevations: helevations,
        zeniths,
        azimuths,
    }
}

fn compute_mask(
    x_obs: usize,
    y_obs: usize,
    study_area: &Array2<f64>,
    precision: usize,
    azimuth: &Array2<f64>,
    elevation: &Array2<f64>,
) -> Array1<f64> {
    let length_elevation = precision * 90 + 1;
    let height = study_area.nrows();
    let width = study_area.ncols();
    let mut azimuth_ne = azimuth.clone();
    let mut azimuth_nw = azimuth.clone();

    if x_obs > 0 {
        for row in 0..y_obs {
            azimuth_ne[(row, x_obs - 1)] -= 360.0;
        }
    }
    for row in 0..y_obs {
        azimuth_nw[(row, x_obs)] += 360.0;
    }

    let north_distance = y_obs;
    let east_distance = width - x_obs;
    let south_distance = height - y_obs;
    let west_distance = x_obs;

    let mut elevation_ne = Array2::<f64>::zeros((north_distance, length_elevation));
    let mut elevation_e = Array2::<f64>::zeros((east_distance, 2 * length_elevation - 1));
    let mut elevation_s = Array2::<f64>::zeros((south_distance, 2 * length_elevation - 1));
    let mut elevation_w = Array2::<f64>::zeros((west_distance, 2 * length_elevation - 1));
    let mut elevation_nw = Array2::<f64>::zeros((north_distance, length_elevation));

    let az_ne = linspace(-180.0, -90.0, length_elevation);
    let az_e = linspace(-180.0, 0.0, 2 * length_elevation - 1);
    let az_s = linspace(-90.0, 90.0, 2 * length_elevation - 1);
    let az_w = linspace(0.0, 180.0, 2 * length_elevation - 1);
    let az_nw = linspace(90.0, 180.0, length_elevation);

    if x_obs > 0 {
        for isoline in 0..north_distance {
            let bins: Vec<f64> = (x_obs - 1..width)
                .map(|col| azimuth_ne[(isoline, col)])
                .collect();
            let k = digitize(&az_ne, &bins);
            for (idx, &bin_idx) in k.iter().enumerate() {
                if bin_idx != 0 && bin_idx != east_distance + 1 {
                    let col = x_obs - 1 + bin_idx;
                    if col < width {
                        elevation_ne[(isoline, idx)] = elevation[(isoline, col)];
                    }
                }
            }
        }
    }

    for isoline in 0..north_distance {
        let bins: Vec<f64> = (0..=x_obs).map(|col| azimuth_nw[(isoline, col)]).collect();
        let k = digitize(&az_nw, &bins);
        for (idx, &bin_idx) in k.iter().enumerate() {
            if bin_idx != 0 && bin_idx != west_distance + 1 {
                let col = bin_idx - 1;
                if col <= x_obs {
                    elevation_nw[(isoline, idx)] = elevation[(isoline, col)];
                }
            }
        }
    }

    for isoline in 0..east_distance {
        let col = x_obs + isoline;
        let bins: Vec<f64> = (0..height).map(|row| azimuth[(row, col)]).collect();
        let k = digitize(&az_e, &bins);
        for (idx, &bin_idx) in k.iter().enumerate() {
            if bin_idx != 0 && bin_idx != height && bin_idx < height {
                elevation_e[(isoline, idx)] = elevation[(bin_idx, col)];
            }
        }
    }

    for isoline in 0..south_distance {
        let row = y_obs + isoline;
        let bins: Vec<f64> = (0..width).rev().map(|col| azimuth[(row, col)]).collect();
        let k = digitize(&az_s, &bins);
        for (idx, &bin_idx) in k.iter().enumerate() {
            if bin_idx != 0 && bin_idx != width && bin_idx < width {
                elevation_s[(isoline, idx)] = elevation[(row, width - 1 - bin_idx)];
            }
        }
    }

    for isoline in 0..west_distance {
        let bins: Vec<f64> = (0..height)
            .rev()
            .map(|row| azimuth[(row, isoline)])
            .collect();
        let k = digitize(&az_w, &bins);
        for (idx, &bin_idx) in k.iter().enumerate() {
            if bin_idx != 0 && bin_idx != height && bin_idx < height {
                elevation_w[(isoline, idx)] = elevation[(height - 1 - bin_idx, isoline)];
            }
        }
    }

    let sun_mask_ne = max_axis0(&elevation_ne);
    let sun_mask_e = max_axis0(&elevation_e);
    let sun_mask_s = max_axis0(&elevation_s);
    let sun_mask_w = max_axis0(&elevation_w);
    let sun_mask_nw = max_axis0(&elevation_nw);
    let az_n_to_n = concat_arrays(&[&az_ne, &az_e, &az_s, &az_w, &az_nw]);
    let sun_mask = concat_arrays(&[
        &sun_mask_ne,
        &sun_mask_e,
        &sun_mask_s,
        &sun_mask_w,
        &sun_mask_nw,
    ]);

    let total_length_elevation = precision * 360 + 1;
    let az = linspace(-180.0, 180.0, total_length_elevation);
    let mut helevation = Array1::<f64>::zeros(total_length_elevation);
    for (i, target_az) in az.iter().enumerate() {
        let mut max_value = 0.0_f64;
        for (idx, candidate) in az_n_to_n.iter().enumerate() {
            if (*candidate - *target_az).abs() < 1e-9 {
                max_value = max_value.max(sun_mask[idx]);
            }
        }
        helevation[i] = max_value;
    }

    helevation
}

fn elevation_angle(
    z_obs: f64,
    study_area: &Array2<f64>,
    latlon: LatLon,
    lats: &Array1<f64>,
    lons: &Array1<f64>,
    ellipsoid: Ellipsoid,
) -> Array2<f64> {
    let lat_rad = latlon.lat.to_radians();
    let lon_rad = latlon.lon.to_radians();
    let (x_a, y_a, z_a) = geographic_to_cartesian(lat_rad, lon_rad, z_obs, ellipsoid);
    let mut out = Array2::<f64>::zeros(study_area.raw_dim());

    for row in 0..study_area.nrows() {
        let lat_grid = lats[row].to_radians();
        for col in 0..study_area.ncols() {
            let lon_grid = lons[col].to_radians();
            let (x_b, y_b, z_b) =
                geographic_to_cartesian(lat_grid, lon_grid, study_area[(row, col)], ellipsoid);
            let inner_product = (x_b - x_a) * lon_rad.cos() * lat_rad.cos()
                + (y_b - y_a) * lon_rad.sin() * lat_rad.cos()
                + (z_b - z_a) * lat_rad.sin();
            let norm = ((x_b - x_a).powi(2) + (y_b - y_a).powi(2) + (z_b - z_a).powi(2)).sqrt();
            out[(row, col)] = (inner_product / norm).asin().to_degrees();
        }
    }

    out
}

fn geographic_to_cartesian(lat: f64, lon: f64, h: f64, ellipsoid: Ellipsoid) -> (f64, f64, f64) {
    let a = ellipsoid.semi_major_metre;
    let e = ellipsoid.eccentricity();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let n = a / (1.0 - e.powi(2) * sin_lat.powi(2)).sqrt();
    let x = (n + h) * lon.cos() * cos_lat;
    let y = (n + h) * lon.sin() * cos_lat;
    let z = (n * (1.0 - e.powi(2)) + h) * sin_lat;
    (x, y, z)
}

fn azimuth(
    latlon: LatLon,
    lats: &Array1<f64>,
    lons: &Array1<f64>,
    ellipsoid: Ellipsoid,
) -> Array2<f64> {
    let lat_rad = latlon.lat.to_radians();
    let lon_rad = latlon.lon.to_radians();
    let l1 = isometric_latitude(lat_rad, ellipsoid);
    let mut out = Array2::<f64>::zeros((lats.len(), lons.len()));

    for row in 0..lats.len() {
        let l2 = isometric_latitude(lats[row].to_radians(), ellipsoid);
        for col in 0..lons.len() {
            let dlons = lon_rad - lons[col].to_radians();
            out[(row, col)] = dlons.atan2(l1 - l2).to_degrees();
        }
    }

    out
}

fn isometric_latitude(lat: f64, ellipsoid: Ellipsoid) -> f64 {
    let e = ellipsoid.eccentricity();
    let term1 = ((std::f64::consts::PI / 4.0) + (lat / 2.0)).tan();
    let sin_lat = lat.sin();
    let term2 = ((1.0 - e * sin_lat) / (1.0 + e * sin_lat)).powf(e / 2.0);
    (term1 * term2).ln()
}

fn pixel_positions(dem: &Array2<f64>, transform: AffineTransform) -> (Array1<f64>, Array1<f64>) {
    let height = dem.nrows();
    let width = dem.ncols();
    let (dlat, dlon) = transform.resolution();
    let west = transform.c;
    let north = transform.f;
    let east = transform.c + transform.a * width as f64;
    let south = transform.f + transform.e * height as f64;
    let lats = linspace(north - dlat / 2.0, south + dlat / 2.0, height);
    let lons = linspace(west + dlon / 2.0, east - dlon / 2.0, width);
    (lats, lons)
}

fn max_axis0(array: &Array2<f64>) -> Array1<f64> {
    let mut out = Array1::<f64>::zeros(array.ncols());
    for col in 0..array.ncols() {
        let mut max_value = f64::NEG_INFINITY;
        for row in 0..array.nrows() {
            max_value = max_value.max(array[(row, col)]);
        }
        out[col] = if max_value.is_finite() {
            max_value
        } else {
            0.0
        };
    }
    out
}

fn concat_arrays(arrays: &[&Array1<f64>]) -> Array1<f64> {
    let len: usize = arrays.iter().map(|a| a.len()).sum();
    let mut out = Array1::<f64>::zeros(len);
    let mut offset = 0;
    for array in arrays {
        for value in array.iter() {
            out[offset] = *value;
            offset += 1;
        }
    }
    out
}

fn digitize(values: &Array1<f64>, bins: &[f64]) -> Vec<usize> {
    if bins.is_empty() {
        return vec![0; values.len()];
    }
    let increasing = bins.first() <= bins.last();
    values
        .iter()
        .map(|value| {
            if increasing {
                bins.iter().take_while(|bin| **bin <= *value).count()
            } else {
                bins.iter().take_while(|bin| **bin > *value).count()
            }
        })
        .collect()
}

fn linspace(start: f64, end: f64, len: usize) -> Array1<f64> {
    if len == 0 {
        return Array1::zeros(0);
    }
    if len == 1 {
        return Array1::from(vec![start]);
    }
    let step = (end - start) / (len - 1) as f64;
    Array1::from_iter((0..len).map(|i| start + step * i as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digitize_matches_numpy_for_increasing_bins() {
        let values = Array1::from(vec![0.0, 1.0, 1.5, 2.0, 3.0]);
        assert_eq!(digitize(&values, &[1.0, 2.0]), vec![0, 1, 1, 2, 2]);
    }
}
