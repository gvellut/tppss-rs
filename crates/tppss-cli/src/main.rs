use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use chrono_tz::Tz;
use clap::{ArgAction, Parser, Subcommand};
use tppss::{
    DemReader, DemReaderOptions, KM, LatLon, SunriseSunset, SunriseSunsetDetails, compute_horizon,
    sunrise_sunset, sunrise_sunset_details, sunrise_sunset_year,
};

#[derive(Debug, Parser)]
#[command(
    name = "tppss",
    about = "Computes sunset / sunrise time taking into account local topography",
    disable_help_flag = true
)]
struct Cli {
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    help: Option<bool>,

    #[arg(short = 'd', long = "debug")]
    debug: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Compute sunset / sunrise time for a single day")]
    Day(DayArgs),

    #[command(about = "Compute sunset / sunrise time for a whole year")]
    Year(YearArgs),
}

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct DayArgs {
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    help: Option<bool>,

    #[arg(short = 'p', long = "position", value_parser = parse_latlon)]
    latlon: LatLon,

    #[arg(short = 'm', long = "dem", value_parser = parse_dem)]
    dem_filepath: String,

    #[arg(short = 'j', long = "day", value_parser = parse_day)]
    day: NaiveDate,

    #[arg(short = 'v', long = "details")]
    details: bool,

    #[arg(short = 't', long = "timezone")]
    timezone: Option<String>,

    #[arg(long = "distance", default_value_t = 25)]
    distance: u32,

    #[arg(short = 'h', long = "height", default_value_t = 2)]
    height: i32,

    #[arg(long = "angle-precision", default_value_t = 1)]
    angle_precision: usize,

    #[arg(long = "time-precision", default_value_t = 60)]
    time_precision: usize,

    #[arg(long = "tile-batch-size", value_parser = parse_positive_usize)]
    tile_batch_size: Option<usize>,
}

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct YearArgs {
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    help: Option<bool>,

    #[arg(short = 'p', long = "position", value_parser = parse_latlon)]
    latlon: LatLon,

    #[arg(short = 'm', long = "dem", value_parser = parse_dem)]
    dem_filepath: String,

    #[arg(short = 'y', long = "year")]
    year: i32,

    #[arg(short = 't', long = "timezone")]
    timezone: Option<String>,

    #[arg(short = 'o', long = "csv")]
    csv_filepath: PathBuf,

    #[arg(long = "distance", default_value_t = 25)]
    distance: u32,

    #[arg(short = 'h', long = "height", default_value_t = 2)]
    height: i32,

    #[arg(long = "angle-precision", default_value_t = 1)]
    angle_precision: usize,

    #[arg(long = "time-precision", default_value_t = 60)]
    time_precision: usize,

    #[arg(long = "tile-batch-size", value_parser = parse_positive_usize)]
    tile_batch_size: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    match cli.command {
        Command::Day(args) => run_day(args).await,
        Command::Year(args) => run_year(args).await,
    }
}

async fn run_day(args: DayArgs) -> Result<()> {
    let tz = get_tz_info(args.timezone.as_deref())?;
    println!("Compute horizon with DEM {}...", args.dem_filepath);
    let dem =
        DemReader::open_with_options(&args.dem_filepath, dem_reader_options(args.tile_batch_size))
            .await?;
    let horizon = compute_horizon(
        args.latlon,
        &dem,
        args.distance as f64 * KM,
        args.angle_precision,
        args.height as f64,
    )
    .await?;

    println!("Compute sunrise / sunset...");
    if args.details {
        match sunrise_sunset_details(args.latlon, &horizon, args.day, tz, args.time_precision)? {
            SunriseSunsetDetails::Times { sunrises, sunsets } => {
                println!("{} sunrises", sunrises.len());
                for (sunrise, sunset) in sunrises.iter().zip(sunsets.iter()) {
                    println!(
                        "Sunrise: {} / Sunset: {}",
                        format_cli_datetime(sunrise),
                        format_cli_datetime(sunset)
                    );
                }
            }
            SunriseSunsetDetails::LightAllDay => println!("Light all day!"),
            SunriseSunsetDetails::NightAllDay => println!("Night all day!"),
        }
    } else {
        match sunrise_sunset(args.latlon, &horizon, args.day, tz, args.time_precision)? {
            SunriseSunset::Times { sunrise, sunset } => {
                println!(
                    "Sunrise: {} / Sunset: {}",
                    format_cli_datetime(&sunrise),
                    format_cli_datetime(&sunset)
                );
            }
            SunriseSunset::LightAllDay => println!("Light all day!"),
            SunriseSunset::NightAllDay => println!("Night all day!"),
        }
    }

    Ok(())
}

async fn run_year(args: YearArgs) -> Result<()> {
    if !(1901..=2099).contains(&args.year) {
        eprintln!("Sun position computation may not be accurate outside years 1901 to 2099!");
    }

    let tz = get_tz_info(args.timezone.as_deref())?;
    println!("Compute horizon...");
    let dem =
        DemReader::open_with_options(&args.dem_filepath, dem_reader_options(args.tile_batch_size))
            .await?;
    let horizon = compute_horizon(
        args.latlon,
        &dem,
        args.distance as f64 * KM,
        args.angle_precision,
        args.height as f64,
    )
    .await?;

    println!("Compute sunrise / sunset for year {}...", args.year);
    let sunsuns = sunrise_sunset_year(args.latlon, &horizon, args.year, tz, args.time_precision)?;

    println!("Write results to {}...", args.csv_filepath.display());
    let file = File::create(&args.csv_filepath)
        .with_context(|| format!("failed to create {}", args.csv_filepath.display()))?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "DAY,SUNRISE,SUNSET")?;
    for row in sunsuns {
        match row.result {
            SunriseSunset::Times { sunrise, sunset } => {
                writeln!(
                    writer,
                    "{},{},{}",
                    row.day.format("%Y-%m-%d"),
                    sunrise.format("%H:%M:%S%z"),
                    sunset.format("%H:%M:%S%z")
                )?;
            }
            SunriseSunset::LightAllDay | SunriseSunset::NightAllDay => {
                writeln!(writer, "{},NA,NA", row.day.format("%Y-%m-%d"))?;
            }
        }
    }

    Ok(())
}

fn parse_latlon(value: &str) -> std::result::Result<LatLon, String> {
    let parts: Vec<_> = value.split(',').collect();
    if parts.len() != 2 {
        return Err(format!(
            "{value:?} is not a valid Lat Lon (eg '45.235555,5.83890')"
        ));
    }
    let lat = parts[0]
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{value:?} is not a valid Lat Lon"))?;
    let lon = parts[1]
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{value:?} is not a valid Lat Lon"))?;
    Ok(LatLon::new(lat, lon))
}

fn parse_day(value: &str) -> std::result::Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("{value:?} is not a valid date in YYYY-MM-DD format"))
}

fn parse_dem(value: &str) -> std::result::Result<String, String> {
    if value.starts_with("gs://")
        || value.starts_with("s3://")
        || value.starts_with("http://")
        || value.starts_with("https://")
    {
        return Ok(value.to_owned());
    }
    let path = PathBuf::from(value);
    if !path.exists() {
        return Err(format!("File {value:?} does not exist."));
    }
    if path.is_dir() {
        return Err(format!("Path {value:?} is a directory, not a file."));
    }
    path.canonicalize()
        .map(|v| v.display().to_string())
        .map_err(|err| err.to_string())
}

fn parse_positive_usize(value: &str) -> std::result::Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{value:?} is not a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{value:?} must be greater than zero"));
    }
    Ok(parsed)
}

fn dem_reader_options(tile_batch_size: Option<usize>) -> DemReaderOptions {
    DemReaderOptions { tile_batch_size }
}

fn get_tz_info(timezone: Option<&str>) -> Result<Tz> {
    if let Some(timezone) = timezone {
        return timezone
            .parse::<Tz>()
            .with_context(|| format!("invalid timezone: {timezone}"));
    }

    let timezone = iana_time_zone::get_timezone().context("failed to get local timezone")?;
    eprintln!("Timezone set to local: '{timezone}'");
    timezone
        .parse::<Tz>()
        .with_context(|| format!("invalid local timezone: {timezone}"))
}

fn init_logging(debug: bool) {
    let level = if debug { "debug" } else { "info" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(level)
        .without_time()
        .try_init();
}

fn format_cli_datetime(value: &chrono::DateTime<Tz>) -> String {
    value.format("%Y-%m-%d %H:%M:%S%:z").to_string()
}
