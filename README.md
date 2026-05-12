# tppss-rs

Rust port of TPPSS, a tool for computing sunrise and sunset times while accounting for local topography from a DEM GeoTIFF.

The workspace contains:

- `crates/tppss`: the library used by the CLI and API server.
- `crates/tppss-cli`: the `tppss` command line binary.

## Build

```sh
cargo build --workspace
```

Enable cloud object-store backends when needed:

```sh
cargo build --workspace --features tppss/gcs,tppss-cli/gcs
cargo build --workspace --features tppss/s3,tppss-cli/s3
```

Remote DEMs should be tiled GeoTIFFs, preferably Cloud Optimized GeoTIFFs, so `async-tiff` can use range reads efficiently.

## CLI

Local DEM example:

```sh
cargo run -p tppss-cli -- day \
  -m /Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif \
  -j 2025-09-07 \
  -p "45.902351,6.144737" \
  --distance 25 \
  --angle-precision 1 \
  -h 30 \
  -t Europe/Paris
```

GCS example:

```sh
cargo run -p tppss-cli --features gcs -- day \
  -m gs://data-mv6uxwxwxy2vz7k0/tppss/savoie/dem_wgs84_b.tif \
  -j 2025-09-07 \
  -p "45.902351,6.144737" \
  -t Europe/Paris
```

S3 example:

```sh
cargo run -p tppss-cli --features s3 -- day \
  -m s3://bucket/path/dem.tif \
  -j 2025-09-07 \
  -p "45.902351,6.144737" \
  -t Europe/Paris
```

Year CSV:

```sh
cargo run -p tppss-cli -- year \
  -m /Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif \
  -y 2025 \
  -p "45.902351,6.144737" \
  -t Europe/Paris \
  -o ss2025.csv
```

## GeoTIFF Support

The current port supports single-band, tiled GeoTIFF/COG DEMs in a geographic CRS. Projected CRS inputs are rejected, matching the Python implementation for now. The reader uses GeoTIFF model pixel scale/tiepoint or model transformation metadata and resolves ellipsoid information from GeoTIFF keys, EPSG metadata, or WGS84 fallback for EPSG:4326/4979.

For GCS, enable the `gcs` feature and use standard Google authentication supported by `object_store`, such as Application Default Credentials or service account environment variables. For S3, enable the `s3` feature and use standard AWS environment variables.

## Verification

Reference fixtures from the Python implementation:

```sh
cargo run -p tppss-cli -- day \
  -m /Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif \
  -j 2026-01-07 \
  -p "46.010148,6.112227" \
  --distance 25 \
  --angle-precision 1 \
  -h 5 \
  -t Europe/Paris
# Night all day!
```

```sh
cargo run -p tppss-cli -- day \
  -m /Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif \
  -j 2025-09-07 \
  -p "45.902351,6.144737" \
  --distance 25 \
  --angle-precision 1 \
  -h 30 \
  -t Europe/Paris
# Sunrise: 2025-09-07 08:32:00+02:00 / Sunset: 2025-09-07 19:51:00+02:00
```

Run checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
```

## TODO

- Add projected CRS support.
- Handle nodata more explicitly.
- Add horizon/sun-course rendering examples.
- Optimize horizon computation further.
- Add more DEM and polar-region regression fixtures.
