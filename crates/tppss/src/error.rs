use thiserror::Error;

#[derive(Debug, Error)]
pub enum TppssError {
    #[error("coordinates ({lat}, {lon}) are not covered by the DEM")]
    DemCoverage { lat: f64, lon: f64 },

    #[error("only geographic CRS are supported")]
    NonGeographicCrs,

    #[error("DEM CRS metadata is missing or unsupported")]
    UnsupportedCrs,

    #[error("DEM geotransform metadata is missing or unsupported")]
    MissingGeoTransform,

    #[error("DEM must contain exactly one sample per pixel")]
    UnsupportedSamplesPerPixel,

    #[error("DEM must be tiled; convert remote files to Cloud Optimized GeoTIFF")]
    UntiledTiff,

    #[error("unsupported DEM data type")]
    UnsupportedDataType,

    #[error("unsupported DEM path or URL: {0}")]
    UnsupportedSource(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("invalid timezone: {0}")]
    InvalidTimezone(String),

    #[error("ambiguous or invalid local time for timezone {timezone}: {time}")]
    InvalidLocalTime { timezone: String, time: String },

    #[error(transparent)]
    Tiff(#[from] async_tiff::error::AsyncTiffError),

    #[error(transparent)]
    ObjectStore(#[from] object_store::Error),

    #[error(transparent)]
    Url(#[from] url::ParseError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TppssError>;
