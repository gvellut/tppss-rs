use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use async_tiff::decoder::DecoderRegistry;
use async_tiff::metadata::TiffMetadataReader;
use async_tiff::metadata::cache::ReadaheadMetadataCache;
use async_tiff::reader::ObjectReader;
use async_tiff::{ImageFileDirectory, TIFF, TypedArray};
use ndarray::Array2;
use object_store::local::LocalFileSystem;
use object_store::path::Path;
use object_store::{ObjectStore, parse_url};
use serde_json::Value;
use url::Url;

use crate::error::{Result, TppssError};
use crate::types::LatLon;

#[derive(Debug, Clone, Copy)]
pub struct AffineTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl AffineTransform {
    pub fn row_col(&self, x: f64, y: f64) -> (isize, isize) {
        let col = ((x - self.c) / self.a).floor() as isize;
        let row = ((y - self.f) / self.e).floor() as isize;
        (row, col)
    }

    pub fn translated(&self, row_offset: usize, col_offset: usize) -> Self {
        Self {
            c: self.c + self.a * col_offset as f64 + self.b * row_offset as f64,
            f: self.f + self.d * col_offset as f64 + self.e * row_offset as f64,
            ..*self
        }
    }

    pub fn resolution(&self) -> (f64, f64) {
        (-self.e, self.a)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ellipsoid {
    pub semi_major_metre: f64,
    pub inverse_flattening: f64,
}

impl Ellipsoid {
    pub const WGS84: Self = Self {
        semi_major_metre: 6_378_137.0,
        inverse_flattening: 298.257_223_563,
    };

    pub fn eccentricity(self) -> f64 {
        let f = 1.0 / self.inverse_flattening;
        (2.0 * f - f.powi(2)).sqrt()
    }
}

#[derive(Debug, Clone)]
pub struct StudyArea {
    pub data: Array2<f64>,
    pub transform: AffineTransform,
    pub observer_row: usize,
    pub observer_col: usize,
    pub ellipsoid: Ellipsoid,
}

#[derive(Debug, Clone)]
pub enum DemSource {
    Local(PathBuf),
    Url(String),
}

impl DemSource {
    pub fn parse(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        if text.starts_with("gs://")
            || text.starts_with("s3://")
            || text.starts_with("http://")
            || text.starts_with("https://")
        {
            Self::Url(text.to_owned())
        } else {
            Self::Local(PathBuf::from(text))
        }
    }
}

#[derive(Clone)]
pub struct DemReader {
    reader: ObjectReader,
    ifd: ImageFileDirectory,
    width: usize,
    height: usize,
    tile_width: usize,
    tile_height: usize,
    transform: AffineTransform,
    ellipsoid: Ellipsoid,
}

impl DemReader {
    pub async fn open(source: impl AsRef<str>) -> Result<Self> {
        let source = DemSource::parse(source);
        let (store, path) = open_object_store(source)?;
        let reader = ObjectReader::new(store, path);
        let cache = ReadaheadMetadataCache::new(reader.clone());
        let mut metadata = TiffMetadataReader::try_open(&cache).await?;
        let ifds = metadata.read_all_ifds(&cache).await?;
        let tiff = TIFF::new(ifds, metadata.endianness());
        let ifd = tiff
            .ifds()
            .first()
            .ok_or_else(|| TppssError::InvalidInput("TIFF contains no image directories".into()))?
            .clone();

        if ifd.samples_per_pixel() != 1 {
            return Err(TppssError::UnsupportedSamplesPerPixel);
        }

        let (tile_count_x, tile_count_y) = ifd.tile_count().ok_or(TppssError::UntiledTiff)?;
        let tile_width = ifd.tile_width().ok_or(TppssError::UntiledTiff)? as usize;
        let tile_height = ifd.tile_height().ok_or(TppssError::UntiledTiff)? as usize;
        if tile_count_x == 0 || tile_count_y == 0 || tile_width == 0 || tile_height == 0 {
            return Err(TppssError::UntiledTiff);
        }

        let transform = affine_transform(&ifd)?;
        let ellipsoid = ellipsoid(&ifd)?;

        Ok(Self {
            reader,
            width: ifd.image_width() as usize,
            height: ifd.image_height() as usize,
            tile_width,
            tile_height,
            transform,
            ellipsoid,
            ifd,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn transform(&self) -> AffineTransform {
        self.transform
    }

    pub fn ellipsoid(&self) -> Ellipsoid {
        self.ellipsoid
    }

    pub async fn extract_study_area(&self, latlon: LatLon, distance_m: f64) -> Result<StudyArea> {
        let (row, col) = self.transform.row_col(latlon.lon, latlon.lat);
        if row < 0 || col < 0 || row >= self.height as isize || col >= self.width as isize {
            return Err(TppssError::DemCoverage {
                lat: latlon.lat,
                lon: latlon.lon,
            });
        }

        let (lat_distance, lon_distance) = dist_to_degrees(latlon, distance_m, self.ellipsoid);
        let (lat_resolution, lon_resolution) = self.transform.resolution();
        let topo_lat_distance = (lat_distance / lat_resolution).round() as isize;
        let topo_lon_distance = (lon_distance / lon_resolution).round() as isize;

        let row_start = (row - topo_lat_distance).max(0) as usize;
        let col_start = (col - topo_lon_distance).max(0) as usize;
        let row_end = (row + topo_lat_distance)
            .min(self.height as isize)
            .max(row_start as isize) as usize;
        let col_end = (col + topo_lon_distance)
            .min(self.width as isize)
            .max(col_start as isize) as usize;

        if row_end <= row_start || col_end <= col_start {
            return Err(TppssError::DemCoverage {
                lat: latlon.lat,
                lon: latlon.lon,
            });
        }

        let data = self
            .read_window(
                row_start,
                col_start,
                row_end - row_start,
                col_end - col_start,
            )
            .await?;
        let study_transform = self.transform.translated(row_start, col_start);
        let (observer_row, observer_col) = study_transform.row_col(latlon.lon, latlon.lat);

        Ok(StudyArea {
            data,
            transform: study_transform,
            observer_row: observer_row as usize,
            observer_col: observer_col as usize,
            ellipsoid: self.ellipsoid,
        })
    }

    async fn read_window(
        &self,
        row_start: usize,
        col_start: usize,
        height: usize,
        width: usize,
    ) -> Result<Array2<f64>> {
        let mut out = Array2::<f64>::zeros((height, width));
        let row_end = row_start + height;
        let col_end = col_start + width;
        let tile_x_start = col_start / self.tile_width;
        let tile_x_end = (col_end - 1) / self.tile_width;
        let tile_y_start = row_start / self.tile_height;
        let tile_y_end = (row_end - 1) / self.tile_height;
        let registry = DecoderRegistry::default();

        for tile_y in tile_y_start..=tile_y_end {
            for tile_x in tile_x_start..=tile_x_end {
                let tile = self.ifd.fetch_tile(tile_x, tile_y, &self.reader).await?;
                let array = tile.decode(&registry)?;
                let shape = array.shape();
                let tile_data = typed_array_to_f64(array.into_inner().0)?;
                let tile_h = shape[0];
                let tile_w = shape[1];
                let global_row_start = tile_y * self.tile_height;
                let global_col_start = tile_x * self.tile_width;
                let copy_row_start = row_start.max(global_row_start);
                let copy_col_start = col_start.max(global_col_start);
                let copy_row_end = row_end.min(global_row_start + tile_h);
                let copy_col_end = col_end.min(global_col_start + tile_w);

                for global_row in copy_row_start..copy_row_end {
                    let src_row = global_row - global_row_start;
                    let dst_row = global_row - row_start;
                    for global_col in copy_col_start..copy_col_end {
                        let src_col = global_col - global_col_start;
                        let dst_col = global_col - col_start;
                        out[(dst_row, dst_col)] = tile_data[src_row * tile_w + src_col];
                    }
                }
            }
        }

        Ok(out)
    }
}

fn open_object_store(source: DemSource) -> Result<(Arc<dyn ObjectStore>, Path)> {
    match source {
        DemSource::Local(path) => open_local(&path),
        DemSource::Url(url) => open_url(&url),
    }
}

fn open_local(path: &FsPath) -> Result<(Arc<dyn ObjectStore>, Path)> {
    let absolute = std::fs::canonicalize(path)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| TppssError::UnsupportedSource(path.display().to_string()))?;
    let file_name = absolute
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| TppssError::UnsupportedSource(path.display().to_string()))?;
    let store = LocalFileSystem::new_with_prefix(parent)?;
    Ok((Arc::new(store), Path::from(file_name)))
}

fn open_url(url: &str) -> Result<(Arc<dyn ObjectStore>, Path)> {
    if url.starts_with("gs://") {
        #[cfg(not(feature = "gcs"))]
        return Err(TppssError::UnsupportedSource(
            "GCS support requires the `gcs` feature".into(),
        ));
    }
    if url.starts_with("s3://") {
        #[cfg(not(feature = "s3"))]
        return Err(TppssError::UnsupportedSource(
            "S3 support requires the `s3` feature".into(),
        ));
    }
    let parsed = Url::parse(url)?;
    let (store, path) = parse_url(&parsed)?;
    Ok((store.into(), path))
}

fn affine_transform(ifd: &ImageFileDirectory) -> Result<AffineTransform> {
    if let Some(matrix) = ifd.model_transformation() {
        if matrix.len() < 16 {
            return Err(TppssError::MissingGeoTransform);
        }
        let transform = AffineTransform {
            a: matrix[0],
            b: matrix[1],
            c: matrix[3],
            d: matrix[4],
            e: matrix[5],
            f: matrix[7],
        };
        if transform.b != 0.0 || transform.d != 0.0 {
            return Err(TppssError::MissingGeoTransform);
        }
        return Ok(transform);
    }

    let scale = ifd
        .model_pixel_scale()
        .ok_or(TppssError::MissingGeoTransform)?;
    let tiepoint = ifd
        .model_tiepoint()
        .ok_or(TppssError::MissingGeoTransform)?;
    if scale.len() < 2 || tiepoint.len() < 6 {
        return Err(TppssError::MissingGeoTransform);
    }

    let tie_i = tiepoint[0];
    let tie_j = tiepoint[1];
    let tie_x = tiepoint[3];
    let tie_y = tiepoint[4];
    Ok(AffineTransform {
        a: scale[0],
        b: 0.0,
        c: tie_x - tie_i * scale[0],
        d: 0.0,
        e: -scale[1],
        f: tie_y + tie_j * scale[1],
    })
}

fn ellipsoid(ifd: &ImageFileDirectory) -> Result<Ellipsoid> {
    let geo = ifd.geo_key_directory().ok_or(TppssError::UnsupportedCrs)?;

    if geo.projected_type.is_some() {
        return Err(TppssError::NonGeographicCrs);
    }

    if let (Some(semi_major), Some(inv_flattening)) =
        (geo.geog_semi_major_axis, geo.geog_inv_flattening)
    {
        return Ok(Ellipsoid {
            semi_major_metre: semi_major,
            inverse_flattening: inv_flattening,
        });
    }

    if let Some(code) = geo.epsg_code() {
        if matches!(code, 4326 | 4979) {
            return Ok(Ellipsoid::WGS84);
        }
        if let Some(ellipsoid) = ellipsoid_from_epsg(code) {
            return Ok(ellipsoid);
        }
    }

    Err(TppssError::UnsupportedCrs)
}

fn ellipsoid_from_epsg(code: u16) -> Option<Ellipsoid> {
    let projjson = epsg_utils::epsg_to_projjson(code as i32).ok()?;
    let value: Value = serde_json::from_str(projjson).ok()?;
    let ellipsoid = value
        .pointer("/datum/ellipsoid")
        .or_else(|| value.pointer("/base_crs/datum/ellipsoid"))?;
    let semi_major = ellipsoid.get("semi_major_axis")?.as_f64()?;
    let inverse_flattening = ellipsoid
        .get("inverse_flattening")
        .and_then(Value::as_f64)
        .or_else(|| {
            let semi_minor = ellipsoid.get("semi_minor_axis")?.as_f64()?;
            Some(semi_major / (semi_major - semi_minor))
        })?;
    Some(Ellipsoid {
        semi_major_metre: semi_major,
        inverse_flattening,
    })
}

fn typed_array_to_f64(data: TypedArray) -> Result<Vec<f64>> {
    let values = match data {
        TypedArray::UInt8(v) => v.into_iter().map(f64::from).collect(),
        TypedArray::UInt16(v) => v.into_iter().map(f64::from).collect(),
        TypedArray::UInt32(v) => v.into_iter().map(|v| v as f64).collect(),
        TypedArray::UInt64(v) => v.into_iter().map(|v| v as f64).collect(),
        TypedArray::Int8(v) => v.into_iter().map(f64::from).collect(),
        TypedArray::Int16(v) => v.into_iter().map(f64::from).collect(),
        TypedArray::Int32(v) => v.into_iter().map(|v| v as f64).collect(),
        TypedArray::Int64(v) => v.into_iter().map(|v| v as f64).collect(),
        TypedArray::Float32(v) => v.into_iter().map(f64::from).collect(),
        TypedArray::Float64(v) => v,
        TypedArray::Bool(_) => return Err(TppssError::UnsupportedDataType),
    };
    Ok(values)
}

fn dist_to_degrees(latlon: LatLon, distance: f64, ellipsoid: Ellipsoid) -> (f64, f64) {
    let lat = latlon.lat.to_radians();
    let a = ellipsoid.semi_major_metre;
    let e = ellipsoid.eccentricity();
    let sin_lat = lat.sin();
    let dlat = a * (1.0 - e.powi(2)) / (1.0 - e.powi(2) * sin_lat.powi(2)).powf(1.5);
    let dlon = a * lat.cos() / (1.0 - e.powi(2) * sin_lat.powi(2)).sqrt();
    let distance_eps = 0.01;
    let mut lat_min = 0.0;
    let mut lat_max = 10_f64.to_radians();
    let mut lon_min = 0.0;
    let mut lon_max = 10_f64.to_radians();

    loop {
        let delta_lat = (lat_min + lat_max) / 2.0;
        let delta_lon = (lon_min + lon_max) / 2.0;
        let dist_var_lat = dlat * delta_lat;
        let dist_var_lon = dlon * delta_lon;

        if (dist_var_lat - distance).abs() < distance_eps
            && (dist_var_lon - distance).abs() < distance_eps
        {
            return (delta_lat.to_degrees(), delta_lon.to_degrees());
        }
        if dist_var_lat < distance {
            lat_min = delta_lat;
        } else {
            lat_max = delta_lat;
        }
        if dist_var_lon < distance {
            lon_min = delta_lon;
        } else {
            lon_max = delta_lon;
        }
    }
}
