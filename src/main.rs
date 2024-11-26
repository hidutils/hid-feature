// SPDX-License-Identifier: MIT

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use hidreport::*;
use owo_colors::{OwoColorize, Stream::Stdout, Style};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type FeatureReport = [u8; 1024];

fn print_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<String>>()
        .join(" ")
}

#[allow(unused)]
enum Styles {
    None,
    Header,
}

impl Styles {
    fn style(&self) -> Style {
        match self {
            Styles::None => Style::new(),
            Styles::Header => Style::new().bold(),
        }
    }
}

// Usage: cprintln!(Sytles::Data, <normal println args>)
macro_rules! cprintln {
    () => { println!(); };
    ($style:expr, $($arg:tt)*) => {{
        println!("{}", format!($($arg)*).if_supports_color(Stdout, |text| text.style($style.style())));
    }};
}

#[allow(unused)]
macro_rules! cprint {
    () => { print!(); };
    ($style:expr, $($arg:tt)*) => {{
        print!("{}", format!($($arg)*).if_supports_color(Stdout, |text| text.style($style.style())));
    }};
}

#[derive(ValueEnum, Clone, Debug)]
enum ClapColorArg {
    Auto,
    Never,
    Always,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Print debugging information
    #[arg(short, long, default_value_t = false)]
    debug: bool,

    #[arg(long, value_enum, default_value_t = ClapColorArg::Auto)]
    color: ClapColorArg,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available hidraw devices
    ListDevices {},
    /// List available Feature Reports on a device.
    ///
    /// The output lists the Report ID (see --report-id), the Feature's usage,
    /// the number of bits and their position in the report as well as the logical
    /// value range and the Report Count for the respective field.
    ///
    /// A Report ID of -1 indicates the report has no ID.
    ///
    /// If the device can be opened, the current values for each feature report
    /// are fetched from the device and printed, together with the full byte(s) at
    /// the field's position.
    ///
    /// The byte value can be used with the 'set' command provided by this tool.
    List {
        /// Filter by the given Report ID
        #[arg(long)]
        report_id: Option<u8>,

        /// Path to the /dev/hidraw node
        path: PathBuf,
    },

    Set {
        /// Path to the /dev/hidraw node
        path: PathBuf,

        /// Specifies the Report ID
        ///
        /// If the device uses Report IDs and has more
        /// than one Feature Report, this option is required.
        #[arg(long)]
        report_id: Option<u8>,

        /// Sets the offset (in bytes) for the byte argument.
        #[arg(long, default_value_t = 0)]
        offset: usize,

        /// The set of bytes in hexadecimal values to set for this report.
        ///
        /// Values may be literal 'xx' or a hexadecimal 1-byte value
        /// without a 0x prefix (e.g. "0a"). Any 'xx' is ignored
        /// all other values overwrite the fetched value
        /// from the report.
        ///
        /// For example:
        ///    hid-feature set xx xx 4a xx 6c
        /// set the third and fifth byte only. The same behaviour
        /// be achieved with an offset:
        ///    hid-feature set --offset=2 4a xx 6c
        ///
        /// The values exclude the Report ID, use --report-id if required.
        bytes: Vec<String>,
    },
}

fn hidraw_name(file: &String) -> Result<String> {
    let uevent_path = PathBuf::from(format!("/sys/class/hidraw/{}/device/uevent", file));
    let uevent = std::fs::read_to_string(uevent_path)?;
    let name = uevent
        .lines()
        .find(|l| l.starts_with("HID_NAME"))
        .context("Unable to find HID_NAME in uevent")?;
    let (_, name) = name
        .split_once('=')
        .context("Unexpected HID_NAME= format")?;
    Ok(name.to_string())
}

fn list_devices() -> Result<()> {
    println!("Available HID devices:");

    let mut hidraws: Vec<String> = std::fs::read_dir("/dev/")?
        .flatten()
        .flat_map(|f| f.file_name().into_string())
        .filter(|name| name.starts_with("hidraw"))
        .collect();

    hidraws.sort_by(|a, b| human_sort::compare(a, b));
    for path in hidraws.iter() {
        let name = hidraw_name(path)?;
        println!("{path:13} - {name}");
    }
    Ok(())
}

fn report_descriptor(path: &Path) -> Result<ReportDescriptor> {
    let filename = path.file_name().unwrap().to_string_lossy();
    let rdesc_path = PathBuf::from(format!(
        "/sys/class/hidraw/{filename}/device/report_descriptor"
    ));

    let bytes = std::fs::read(rdesc_path)?;
    Ok(ReportDescriptor::try_from(&bytes)?)
}

fn list(path: &Path, filter_id: &Option<u8>) -> Result<()> {
    let rdesc = report_descriptor(path)?;

    let reports = rdesc.feature_reports();
    if reports.is_empty() {
        println!("This device does not have any Feature Reports");
        return Ok(());
    }
    let usage_header = format!("{:^48}", "Usage");
    let headers: Vec<&str> = vec![
        "Report",
        usage_header.as_str(),
        "Bits",
        "Bit Range",
        "Value Range",
        "Count",
        "Value",
        "Bytes",
    ];

    cprintln!(Styles::Header, "{}", headers.join(" ┃ "));
    cprintln!(
        Styles::Header,
        "{}",
        headers
            .iter()
            .map(|h| str::repeat("━", h.len()))
            .collect::<Vec<String>>()
            .join("━╇━")
    );

    for report in reports {
        let report_id: u8 = match report.report_id() {
            None => 0xff,
            Some(id) => u8::from(id),
        };
        if let Some(filter_id) = filter_id {
            if report_id == *filter_id {
                continue;
            }
        }

        // Our report's length only includes the report ID if there is one but the ioctl
        // always needs the first byte to be the report ID.
        //
        // The return value is properly sized, the report ID is not returned.
        let report_size = report.size_in_bytes();
        let fetch_size = match report.report_id() {
            Some(_) => report_size,
            None => report_size + 1,
        };
        let rid = report.report_id().map_or(0, u8::from);
        let mut device = hidraw::Device::open(path)?;
        let r = unsafe { device.get_feature_report_with_size::<FeatureReport>(rid, fetch_size) }?;
        let values = r[..report_size].to_vec();
        for field in report.fields() {
            let min: i32;
            let max: u32;
            let count: usize;
            let hutstr: String;

            let offset = field.bits().start / 8;
            let end = (field.bits().end - 1) / 8;

            let value: i32;

            match field {
                Field::Variable(var) => {
                    min = i32::from(var.logical_minimum);
                    max = i32::from(var.logical_maximum) as u32;
                    count = 1;
                    value = var.extract(&values)?.into();
                    hutstr = match hut::Usage::new_from_page_and_id(
                        u16::from(var.usage.usage_page),
                        u16::from(var.usage.usage_id),
                    ) {
                        Err(_) => "<unknown>".into(),
                        Ok(u) => format!("{} / {}", hut::UsagePage::from(&u), u),
                    };
                }
                Field::Array(arr) => {
                    min = i32::from(arr.logical_minimum);
                    max = i32::from(arr.logical_maximum) as u32;
                    count = usize::from(arr.report_count);
                    value = arr.extract_one(&values, 0)?.into();
                    hutstr = "<not implemented>".into();
                }
                _ => continue,
            };

            println!(
                "{:^6} │ {hutstr:48} │ {:^4} │ {:3}..={:<3} │ {min:4}..={max:<4} │ {count:^5} │ {value:5} │ {}",
                report_id as i8,
                field.bits().end - field.bits().start,
                field.bits().start,
                field.bits().end - 1,
                print_bytes(&values[offset..=end])
            );
        }
    }

    Ok(())
}

fn set(path: &Path, filter_id: &Option<u8>, bytes: &[String], offset: usize) -> Result<()> {
    let rdesc = report_descriptor(path)?;

    let reports = rdesc.feature_reports();
    if reports.is_empty() {
        bail!("This device does not have any Feature Reports");
    }

    for v in bytes.iter().filter(|v| v != &"xx") {
        u8::from_str_radix(v, 16).context("Invalid value, must be 'xx' or 1-byte hex")?;
    }

    let report = match filter_id {
        Some(id) => reports
            .iter()
            .find(|r| u8::from(r.report_id().unwrap()) == *id)
            .context("Unable to find report {id}")?,
        None => reports.first().unwrap(),
    };

    // ioctl uses 0 for Report ID None
    let rid = report.report_id().map_or(0, u8::from);

    // Our report's length only includes the report ID if there is one but the ioctl
    // always needs the first byte to be the report ID.
    //
    // The return value is properly sized, the report ID is not returned.
    let report_size = report.size_in_bytes();
    let fetch_size = match report.report_id() {
        Some(_) => report_size,
        None => report_size + 1,
    };
    let mut device = hidraw::Device::open(path)?;
    let r = unsafe { device.get_feature_report_with_size::<[u8; 20]>(rid, fetch_size) }?;

    // prepend the report ID again if need be
    let mut values: FeatureReport = [0; 1024];
    let rid_off = match report.report_id() {
        Some(_) => 0,
        None => {
            values[0] = rid;
            1
        }
    };
    for (i, v) in r[0..report_size].iter().enumerate() {
        values[i + rid_off] = *v;
    }

    for (i, val) in bytes.iter().enumerate() {
        let idx = offset + rid_off + i;
        if val != "xx" {
            values[idx] = u8::from_str_radix(val, 16)?;
        } else {
            values[idx] = r[i];
        }
    }

    unsafe { device.send_feature_report_with_size::<FeatureReport>(&values, fetch_size) }?;

    Ok(())
}

fn hid_feature() -> Result<()> {
    let cli = Cli::parse();

    // Bit lame but easier to just set the env for owo_colors to figure out the rest
    match cli.color {
        ClapColorArg::Never => std::env::set_var("NO_COLOR", "1"),
        ClapColorArg::Auto => {}
        ClapColorArg::Always => std::env::set_var("FORCE_COLOR", "1"),
    }

    match cli.command {
        Commands::ListDevices {} => list_devices(),
        Commands::List { report_id, path } => list(&path, &report_id),
        Commands::Set {
            report_id,
            bytes,
            path,
            offset,
        } => set(&path, &report_id, &bytes, offset),
    }
}

fn main() -> ExitCode {
    let rc = hid_feature();
    match rc {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
