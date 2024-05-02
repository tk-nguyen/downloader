use std::{
    env,
    fs::OpenOptions,
    io::{BufWriter, Write},
    thread,
    time::Instant,
};

use log::info;
use miette::{bail, ensure, IntoDiagnostic, Result};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use ureq::Agent;
use url::Url;

const CONNECTIONS: usize = 10;

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .into_diagnostic()?;
    ensure!(env::args().len() == 2, "Invalid number of arguments.");
    // We already make sure there's only 1 arg
    let url = Url::parse(&env::args().nth(1).unwrap()).into_diagnostic()?;
    let agent = Agent::new();
    let res = agent.head(url.as_str()).send_bytes(&[]).into_diagnostic()?;
    let filesize = match res.has("Content-Length") {
        true => res
            .header("Content-Length")
            .unwrap()
            .parse::<usize>()
            .into_diagnostic()?,
        false => bail!("No file size found! Cannot download!"),
    };
    let segment_size = match res.has("Accept-Ranges") {
        true => filesize / CONNECTIONS,
        false => filesize,
    };
    let start = Instant::now();
    let threads = (0..CONNECTIONS)
        .map(|i| {
            let conn = agent.clone();
            let url = url.clone();
            thread::spawn(move || {
                let mut body = Vec::with_capacity(segment_size);
                let byte_range = match i < CONNECTIONS - 1 {
                    true => format!("bytes={}-{}", i * segment_size, (i + 1) * segment_size - 1),
                    false => format!("bytes={}-", i * segment_size),
                };
                conn.get(url.as_str())
                    .set("Range", &byte_range)
                    .send_bytes(&[])
                    .unwrap()
                    .into_reader()
                    .read_to_end(&mut body)
                    .unwrap();
                let duration = start.elapsed().as_secs_f64();
                info!(
                    "Segment {i} downloaded in {duration}s, speed {:.2} MB/s",
                    segment_size as f64 / (duration * 1_000_000.0)
                );
                body
            })
        })
        .collect::<Vec<_>>();
    let mut data = vec![];
    for t in threads {
        data.push(t.join().unwrap());
    }
    info!("Writing file...");
    let filename = res.get_url().split('/').last().unwrap();
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(filename)
        .into_diagnostic()?;
    let mut writer = BufWriter::new(file);
    for d in data {
        writer.write_all(&d).into_diagnostic()?;
        writer.flush().into_diagnostic()?;
    }
    info!("Finished downloading file {filename}");

    Ok(())
}
