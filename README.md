# tppss-rs

Rust port of my [TPPSS](https://github.com/gvellut/tppss) Python lib/cli, a tool for computing sunrise and sunset times while accounting for local topography from a DEM GeoTIFF.

The workspace contains:

- `crates/tppss`: the library used by the CLI and API server.
- `crates/tppss-cli`: the `tppss` command line binary.

## Build

```sh
cargo build --workspace
```

Release build:

```sh
cargo build --workspace --release
```

The CLI binary is written to:

```sh
target/release/tppss
```

Enable cloud object-store backends when needed:

```sh
cargo build --workspace --features tppss/gcs,tppss-cli/gcs
cargo build --workspace --features tppss/s3,tppss-cli/s3
```

Release builds with cloud object-store backends:

```sh
cargo build --workspace --release --features tppss/gcs,tppss-cli/gcs
cargo build --workspace --release --features tppss/s3,tppss-cli/s3
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
  -m gs://data-mj3kvpi7h1xq3omdt49t39po/tppss/savoie/dem_wgs84_cog.tif \
  -j 2025-09-07 \
  -p "45.902351,6.144737" \
  --distance 25 \
  --angle-precision 1 \
  -h 30 \
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

## VS Code Code Signing in macOS for development

On macOS, debugging a rebuilt unsigned binary can repeatedly trigger privacy prompts when the CLI runs or reads DEMs from protected locations (such as `Documents`, `Downloads`, external volumes, or removable drives). The repository includes a VS Code setup that builds and signs the CLI before CodeLLDB launches it.

The signing script is:

```sh
scripts/sign-built-product.sh
```

It builds the requested binary and signs it with the local signing identity from `SIGNING_IDENTITY`. If unset, it uses:

```sh
My Swift Dev Cert
```

See https://www.simplified.guide/macos/keychain-cert-code-signing-create on how to create a local dev cert.

Manual debug build and sign:

```sh
./scripts/sign-built-product.sh tppss debug
```

Manual release build and sign:

```sh
./scripts/sign-built-product.sh tppss release
```

Pass any additional Cargo build arguments after `debug` or `release`. For example, build and sign with GCS support:

```sh
./scripts/sign-built-product.sh tppss debug --features tppss/gcs,tppss-cli/gcs
```

Build and sign with S3 support:

```sh
./scripts/sign-built-product.sh tppss debug --features tppss/s3,tppss-cli/s3
```

Use a different local certificate:

```sh
SIGNING_IDENTITY="Your Certificate Name" ./scripts/sign-built-product.sh tppss debug
```

The VS Code tasks in `.vscode/tasks.json` call the script:

- `rust: Build Debug tppss CLI signed`
- `rust: Build Debug tppss CLI signed + GCS`
- `rust: Build Debug tppss CLI signed + S3`

The CodeLLDB launch configs in `.vscode/launch.json` use those tasks through `preLaunchTask`, then launch the signed binary directly from `target/debug/tppss` or `target/release/tppss`. Do not put a `cargo` block in these launch configs if you need signing; CodeLLDB would build and launch the binary before the script can sign it.

## GeoTIFF Support

The current port supports single-band, tiled GeoTIFF/COG DEMs in a geographic CRS. Projected CRS inputs are rejected, matching the Python implementation for now. The reader uses GeoTIFF model pixel scale/tiepoint or model transformation metadata and resolves ellipsoid information from GeoTIFF keys, EPSG metadata, or WGS84 fallback for EPSG:4326/4979.

The DEM reader batches all tiles needed by a window read by default. Library callers can use `DemReader::open_with_options` and `DemReaderOptions { tile_batch_size: Some(n) }` to split tile requests into smaller batches, or `None` to keep the single batched request default.

For GCS, enable the `gcs` feature and use standard Google authentication supported by `object_store`, such as Application Default Credentials or service account environment variables. For S3, enable the `s3` feature and use standard AWS environment variables.

## Verification

Run checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
```

## TODO

- Add projected CRS support.
- Support negative horizon elevation angles for observers on peaks instead of clamping them to zero.
- Handle nodata more explicitly.
- Add horizon/sun-course rendering examples.
- Optimize horizon computation further.
- Add more DEM and polar-region regression fixtures.
