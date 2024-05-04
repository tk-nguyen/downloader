use std::{
    fs::File,
    io::{BufWriter, Write},
    str::FromStr,
    thread,
};

use clap::Parser;
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};
use miette::{bail, ensure, IntoDiagnostic, Result};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use ureq::Agent;
use url::Url;

#[derive(Debug, Parser)]
#[command(version, about)]
/// A multi-threaded downloader
struct DownloaderOptions {
    /// The file URL to download
    #[arg(value_parser(Url::parse))]
    url: Url,

    /// Number of connections to make
    #[arg(long, short, default_value_t = 10)]
    connections: usize,

    /// Minimum split size
    #[arg(long, short, default_value_t = 20_000_000)] // 20M
    split_size: usize,
}

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .into_diagnostic()?;

    let opt = DownloaderOptions::parse();
    let url = opt.url;
    let connections = opt.connections;
    let split_size = opt.split_size;

    let agent = Agent::new();

    let res = agent.head(url.as_str()).send_bytes(&[]).into_diagnostic()?;

    ensure!(
        res.has("Accept-Ranges"),
        "Server does not accept Range header! Cannot proceed."
    );

    let filesize = match res.has("Content-Length") {
        true => res
            .header("Content-Length")
            .unwrap()
            .parse::<usize>()
            .into_diagnostic()?,
        false => bail!("No file size found! Cannot download!"),
    };
    let filename = res.get_url().split('/').last().unwrap();
    let msg = String::from_str(filename).into_diagnostic()?;
    let bar = ProgressBar::new(filesize as u64)
        .with_style(
            ProgressStyle::with_template("{msg}\n{wide_bar} {decimal_bytes}/{decimal_total_bytes}")
                .into_diagnostic()?,
        )
        .with_message(msg)
        .with_finish(ProgressFinish::Abandon);

    let num_parts = filesize / split_size;

    let file = File::create(filename).into_diagnostic()?;
    let mut writer = BufWriter::new(file);

    for part in 0..=num_parts {
        let segment_size = match part < num_parts {
            true => split_size / connections,
            false => (filesize - part * split_size) / connections,
        };

        (0..connections)
            .map(|i| {
                let conn = agent.clone();
                let url = url.clone();
                thread::spawn(move || {
                    let mut body = Vec::with_capacity(segment_size);
                    let byte_range = match (part < num_parts, i < connections - 1) {
                        (true, true) | (true, false) | (false, true) => format!(
                            "bytes={}-{}",
                            i * segment_size + part * split_size,
                            (i + 1) * segment_size + part * split_size - 1
                        ),
                        (false, false) => {
                            format!("bytes={}-", i * segment_size + part * split_size)
                        }
                    };
                    conn.get(url.as_str())
                        .set("Range", &byte_range)
                        .send_bytes(&[])
                        .unwrap()
                        .into_reader()
                        .read_to_end(&mut body)
                        .unwrap();
                    body
                })
            })
            .for_each(|t| {
                let written = writer
                    .write(&t.join().expect("Cannot join thread!"))
                    .expect("Cannot write to file!");
                bar.inc(written as u64)
            });
    }
    // Flush a final time to make sure all data are written to disk
    writer.flush().into_diagnostic()?;

    Ok(())
}
